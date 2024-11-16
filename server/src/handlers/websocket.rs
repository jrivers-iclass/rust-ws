use futures_util::{StreamExt, SinkExt};
use tokio::sync::mpsc;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};
use redis::{AsyncCommands, RedisError};
use std::sync::Arc;
use serde_json::Value;
use std::time::Duration;
use tokio::time::interval;

use crate::state::{Clients, Topics, RedisState, Client};
use crate::models::{WSMessage, WSError, Topic};
use crate::utils::logger::{
    log_connection, log_disconnection, log_error_msg,
    log_rejected,
};

pub async fn ws_handler(
    ws: warp::ws::Ws,
    clients: Clients,
    topics: Topics,
    redis_state: Arc<RedisState>,
) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(ws.on_upgrade(move |socket| client_connection(socket, clients, topics, redis_state)))
}

async fn send_success(
    client_id: &str,
    action: &str,
    topic: &str,
    _message: &str,
    clients: &Clients,
) {
    if let Some(client) = clients.read().await.get(client_id) {
        let success_message = serde_json::json!({
            "action": action,
            "topic": topic,
            "message": "Received",
            "status": "success"
        });

        if let Ok(msg_str) = serde_json::to_string(&success_message) {
            let _ = client.sender.send(Message::text(msg_str));
        }
    }
}

async fn send_error(
    client_id: &str,
    action: &str,
    topic: &str,
    code: u16,
    message: &str,
    clients: &Clients,
) {
    let error = WSError::new(action, topic, code, message);
    if let Some(client) = clients.read().await.get(client_id) {
        let _ = client
            .sender
            .send(Message::text(serde_json::to_string(&error).unwrap()));
    }
}

pub async fn client_connection(
    ws: WebSocket,
    clients: Clients,
    topics: Topics,
    redis_state: Arc<RedisState>,
) {
    let (mut client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, mut client_rcv) = mpsc::unbounded_channel();

    let client_id = Uuid::new_v4().to_string();
    log_connection(&client_id);

    // Add client to Redis and local state
    {
        let mut clients = clients.write().await;
        clients.insert(client_id.clone(), Client {
            sender: client_sender,
        });

        // Add to Redis clients hash
        let mut conn = redis_state.connection.clone();
        let client_key = format!("clients:{}", client_id);
        let _: Result<(), RedisError> = redis::pipe()
            .hset(&client_key, "server_id", &redis_state.server_id)
            .hset(&client_key, "connected_at", chrono::Utc::now().to_rfc3339())
            .query_async(&mut conn)
            .await;
    }

    // Clone client_id for the send task
    let send_task_client_id = client_id.clone();

    // Forward messages from the channel to the websocket
    let _send_task = tokio::spawn(async move {
        while let Some(message) = client_rcv.recv().await {
            if let Err(e) = client_ws_sender.send(message).await {
                log_error_msg(&send_task_client_id, "send", &e.to_string());
                break;
            }
        }
    });

    // Handle incoming messages
    while let Some(result) = client_ws_rcv.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                log_error_msg(&client_id, "receive", &e.to_string());
                break;
            }
        };

        if msg.is_text() {
            handle_client_message(&client_id, msg, &clients, &topics, &redis_state).await;
        }
    }

    // Cleanup on disconnect
    log_disconnection(&client_id);
    clients.write().await.remove(&client_id);

    // Remove from Redis clients hash
    let mut conn = redis_state.connection.clone();
    let _: Result<(), RedisError> = conn.del(format!("clients:{}", client_id)).await;
}

async fn handle_client_message(
    client_id: &str,
    msg: Message,
    clients: &Clients,
    topics: &Topics,
    redis_state: &RedisState,
) {
    let message = match msg.to_str() {
        Ok(v) => v,
        Err(_) => {
            log_error_msg(client_id, "message parsing", "invalid message format");
            send_error(client_id, "unknown", "", 500, "Invalid message format", clients).await;
            return;
        }
    };

    let ws_message: WSMessage = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            log_error_msg(client_id, "JSON parsing", &e.to_string());
            send_error(client_id, "unknown", "", 500, "Invalid JSON format", clients).await;
            return;
        }
    };

    match ws_message.action.as_str() {
        "create_topic" => {
            let topic_name = ws_message.topic.trim();
            if topic_name.is_empty() {
                log_rejected(client_id, "create_topic", topic_name, "topic name is empty");
                send_error(client_id, "create_topic", topic_name, 400, "Topic name cannot be empty", clients).await;
                return;
            }

            let mut topics_guard = topics.write().await;
            if topics_guard.contains_key(topic_name) {
                log_rejected(client_id, "create_topic", topic_name, "topic already exists");
                send_error(client_id, "create_topic", topic_name, 409, "Topic already exists", clients).await;
                return;
            }

            // Create new topic
            let new_topic = Topic {
                name: topic_name.to_string(),
                creator: client_id.to_string(),
                created_at: chrono::Utc::now(),
                subscribers: vec![client_id.to_string()],
            };

            // Store in Redis
            let mut conn = redis_state.connection.clone();
            let topic_key = format!("topics:{}", topic_name);
            
            // Store topic metadata using individual HSET commands
            let _: Result<(), RedisError> = redis::pipe()
                .hset(&topic_key, "name", topic_name)
                .hset(&topic_key, "creator", client_id)
                .hset(&topic_key, "created_at", new_topic.created_at.to_rfc3339())
                .query_async(&mut conn)
                .await;

            // Store subscribers
            let _: Result<(), RedisError> = conn.sadd(
                format!("{}:subscribers", topic_key),
                client_id
            ).await;

            // Store in local state
            topics_guard.insert(topic_name.to_string(), new_topic);

            send_success(
                client_id,
                "create_topic",
                topic_name,
                "Topic created successfully",
                clients
            ).await;
        }
        "publish" => {
            println!("Publishing message to topic: {}", ws_message.topic);
            let topics_guard = topics.read().await;
            let topic = match topics_guard.get(&ws_message.topic) {
                Some(topic) => topic,
                None => {
                    log_rejected(client_id, "publish", &ws_message.topic, "topic does not exist");
                    send_error(client_id, "publish", &ws_message.topic, 404, "Topic does not exist", clients).await;
                    return;
                }
            };

            if !topic.subscribers.contains(&client_id.to_string()) {
                log_rejected(client_id, "publish", &ws_message.topic, "not subscribed to topic");
                send_error(client_id, "publish", &ws_message.topic, 403, "Not subscribed to topic", clients).await;
                return;
            }

            let message_content = match &ws_message.message {
                Some(content) => content,
                None => {
                    log_rejected(client_id, "publish", &ws_message.topic, "message content is required");
                    send_error(client_id, "publish", &ws_message.topic, 400, "Message content is required", clients).await;
                    return;
                }
            };

            let publish_message = serde_json::json!({
                "action": "publish",
                "topic": ws_message.topic,
                "message": message_content,
                "sender": client_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "origin_server": redis_state.server_id
            });

            println!("Publishing message to Redis: {}", publish_message);

            // Publish to Redis with explicit type annotation
            let mut conn = redis_state.connection.clone();
            let msg_string = publish_message.to_string();
            let result: Result<i64, redis::RedisError> = conn.publish("messages", msg_string).await;
            
            match result {
                Ok(_) => println!("Successfully published to Redis"),
                Err(e) => eprintln!("Failed to publish to Redis: {}", e),
            };
            
            send_success(
                client_id,
                "publish",
                &ws_message.topic,
                message_content,
                clients
            ).await;
        }
        "subscribe" => {
            let topic_name = ws_message.topic.trim();
            if topic_name.is_empty() {
                log_rejected(client_id, "subscribe", topic_name, "topic name is empty");
                send_error(client_id, "subscribe", topic_name, 400, "Topic name cannot be empty", clients).await;
                return;
            }

            let mut topics_guard = topics.write().await;
            let topic = match topics_guard.get_mut(topic_name) {
                Some(topic) => topic,
                None => {
                    log_rejected(client_id, "subscribe", topic_name, "topic does not exist");
                    send_error(client_id, "subscribe", topic_name, 404, "Topic does not exist", clients).await;
                    return;
                }
            };

            // Check Redis first
            let mut conn = redis_state.connection.clone();
            let is_subscribed: bool = conn.sismember(
                format!("topics:{}:subscribers", topic_name),
                client_id
            ).await.unwrap_or(false);

            if is_subscribed {
                log_rejected(client_id, "subscribe", topic_name, "already subscribed in Redis");
                send_error(client_id, "subscribe", topic_name, 409, "Already subscribed to topic", clients).await;
                return;
            }

            // Then check local state
            if topic.subscribers.contains(&client_id.to_string()) {
                // If not in Redis but in local state, remove from local state to maintain consistency
                topic.subscribers.retain(|id| id != &client_id.to_string());
            }

            // Add subscriber to Redis
            let _: Result<(), RedisError> = conn.sadd(
                format!("topics:{}:subscribers", topic_name),
                client_id
            ).await;

            // Add subscriber to local state
            topic.subscribers.push(client_id.to_string());

            send_success(
                client_id,
                "subscribe",
                topic_name,
                "Subscribed to topic successfully",
                clients
            ).await;
        }
        _ => {
            log_rejected(client_id, &ws_message.action, &ws_message.topic, "unknown action");
            send_error(client_id, &ws_message.action, &ws_message.topic, 400, "Unknown action", clients).await;
        }
    }
}

async fn handle_redis_message(
    msg: String,
    clients: &Clients,
    redis_state: &RedisState,
) {
    println!("Received Redis message: {}", msg);

    let message: Value = match serde_json::from_str(&msg) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to parse Redis message: {}", e);
            return;
        }
    };

    // Extract fields from the message
    let topic = match message.get("topic").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => {
            eprintln!("Missing topic in Redis message");
            return;
        }
    };

    let sender = message.get("sender")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    println!("Message for topic: {}, from sender: {}", topic, sender);

    // Get subscribers for this topic
    let mut conn = redis_state.connection.clone();
    let subscribers: Vec<String> = match conn.smembers(format!("topics:{}:subscribers", topic)).await {
        Ok(subs) => subs,
        Err(e) => {
            eprintln!("Failed to get subscribers from Redis: {}", e);
            return;
        }
    };

    println!("Found subscribers: {:?}", subscribers);

    // Prepare message for clients
    let client_message = serde_json::json!({
        "action": "message",
        "topic": topic,
        "message": message.get("message"),
        "sender": sender,
        "timestamp": message.get("timestamp")
    });

    let msg_str = match serde_json::to_string(&client_message) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to serialize message: {}", e);
            return;
        }
    };

    // Send to all subscribers except the sender
    let clients_guard = clients.read().await;
    for subscriber_id in subscribers {
        // Skip sending to the original sender
        if subscriber_id == sender {
            println!("Skipping message to original sender: {}", sender);
            continue;
        }

        println!("Attempting to send to subscriber: {}", subscriber_id);
        match clients_guard.get(&subscriber_id) {
            Some(client) => {
                if let Err(e) = client.sender.send(Message::text(msg_str.clone())) {
                    eprintln!("Failed to send message to client {}: {}", subscriber_id, e);
                } else {
                    println!("Successfully sent message to client {}", subscriber_id);
                }
            }
            None => {
                println!("Client {} not found on this server, cleaning up", subscriber_id);
                // Client not found on this server, clean up Redis
                let _: Result<(), redis::RedisError> = redis::pipe()
                    .srem(
                        format!("topics:{}:subscribers", topic),
                        &subscriber_id
                    )
                    .query_async(&mut conn)
                    .await;
            }
        }
    }
}

pub async fn start_redis_subscriber(
    redis_state: Arc<RedisState>,
    clients: Clients,
) {
    println!("Starting Redis subscriber for server: {}", redis_state.server_id); // Debug log

    let client = redis::Client::open(redis_state.url.as_str())
        .expect("Failed to create Redis client");
    
    let connection = client.get_async_connection()
        .await
        .expect("Failed to get async connection");

    let mut pubsub = connection.into_pubsub();
    
    if let Err(e) = pubsub.subscribe("messages").await {
        eprintln!("Failed to subscribe to Redis channel: {}", e);
        return;
    }

    println!("Successfully subscribed to 'messages' channel"); // Debug log

    let mut messages = pubsub.on_message();
    
    while let Some(msg) = messages.next().await {
        let payload: String = match msg.get_payload() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to get message payload: {}", e);
                continue;
            }
        };

        handle_redis_message(payload, &clients, &redis_state).await;
    }
}

pub async fn start_cleanup_task(
    redis_state: Arc<RedisState>,
    clients: Clients,
) {
    println!("Starting cleanup task for server: {}", redis_state.server_id);
    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await;
        cleanup_disconnected_clients(&redis_state, &clients).await;
    }
}

async fn cleanup_disconnected_clients(
    redis_state: &RedisState,
    clients: &Clients,
) {
    let mut conn = redis_state.connection.clone();
    
    // Get all client keys
    let client_keys: Vec<String> = match conn.keys("clients:*").await {
        Ok(keys) => keys,
        Err(e) => {
            eprintln!("Failed to get client keys: {}", e);
            return;
        }
    };

    // Get currently connected clients on this server
    let connected_clients: Vec<String> = clients.read().await
        .keys()
        .cloned()
        .collect();

    // Clean up disconnected clients
    for client_key in client_keys {
        let client_id = client_key.strip_prefix("clients:").unwrap_or(&client_key).to_string();
        
        // Get the client's server_id from Redis
        let server_id: String = match conn.hget(&client_key, "server_id").await {
            Ok(sid) => sid,
            Err(e) => {
                eprintln!("Failed to get server_id for client {}: {}", client_id, e);
                continue;
            }
        };

        // Only clean up clients that belong to this server
        if server_id == redis_state.server_id && !connected_clients.contains(&client_id) {
            println!("Cleaning up disconnected client {} from Redis", client_id);
            
            // Remove client hash
            let _: Result<(), RedisError> = conn.del(&client_key).await;

            // Clean up topic subscriptions
            let topic_keys: Vec<String> = match conn.keys("topics:*:subscribers").await {
                Ok(keys) => keys,
                Err(e) => {
                    eprintln!("Failed to get topic keys: {}", e);
                    continue;
                }
            };

            for topic_key in topic_keys {
                let _: Result<(), RedisError> = conn.srem(&topic_key, &client_id).await;
            }
        }
    }
} 