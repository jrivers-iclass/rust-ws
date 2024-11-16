use futures_util::{StreamExt, SinkExt};
use tokio::sync::mpsc;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};
use redis::{AsyncCommands, RedisError};
use std::sync::Arc;
use serde_json::Value;
use std::time::Duration;
use tokio::time::interval;
use lapin::{
    options::*, types::FieldTable, BasicProperties,
    ExchangeKind,
};

use crate::state::{Clients, RedisState, Client, RabbitMQState};
use crate::models::{WSMessage, WSError};
use crate::utils::logger::{
    log_connection, log_disconnection, log_error_msg,
    log_rejected,
};

pub async fn ws_handler(
    ws: warp::ws::Ws,
    clients: Clients,
    redis_state: Arc<RedisState>,
    rabbitmq_state: Arc<RabbitMQState>,
) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(ws.on_upgrade(move |socket| client_connection(
        socket, 
        clients, 
        redis_state,
        rabbitmq_state
    )))
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
    redis_state: Arc<RedisState>,
    rabbitmq_state: Arc<RabbitMQState>,
) {
    let client_id = Uuid::new_v4().to_string();
    log_connection(&client_id);

    // Add client to Redis with server_id
    let mut conn = redis_state.connection.clone();
    let _: Result<(), RedisError> = redis::pipe()
        .hset(
            format!("clients:{}", client_id),
            "server_id",
            &redis_state.server_id
        )
        .hset(
            format!("clients:{}", client_id),
            "connected_at",
            chrono::Utc::now().to_rfc3339()
        )
        .query_async(&mut conn)
        .await;

    // Split the WebSocket
    let (mut client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, mut client_rcv) = mpsc::unbounded_channel();

    // Add the client to our clients map
    clients.write().await.insert(
        client_id.clone(),
        Client {
            sender: client_sender,
        },
    );

    let clients_clone = clients.clone();
    let client_id_for_recv = client_id.clone();
    let client_id_for_cleanup = client_id.clone();
    let redis_state_for_recv = Arc::clone(&redis_state);
    let redis_state_for_cleanup = Arc::clone(&redis_state);
    let rabbitmq_state = Arc::clone(&rabbitmq_state);

    let mut recv_task = tokio::spawn(async move {
        while let Some(result) = client_ws_rcv.next().await {
            match result {
                Ok(msg) => {
                    handle_client_message(
                        &client_id_for_recv,
                        msg,
                        &clients_clone,
                        &redis_state_for_recv,
                        &rabbitmq_state,
                    ).await;
                }
                Err(e) => {
                    eprintln!("Error receiving message for client {}: {}", client_id_for_recv, e);
                    break;
                }
            }
        }

        // Client disconnected
        log_disconnection(&client_id_for_recv);
        clients_clone.write().await.remove(&client_id_for_recv);
    });

    let mut send_task = tokio::spawn(async move {
        while let Some(message) = client_rcv.recv().await {
            if let Err(e) = client_ws_sender.send(message).await {
                eprintln!("Error sending websocket msg: {}", e);
                break;
            }
        }
    });

    // If any one of the tasks exit, abort the other and cleanup
    tokio::select! {
        _ = (&mut send_task) => {
            recv_task.abort();
            cleanup_on_disconnect(&client_id_for_cleanup, &redis_state_for_cleanup).await;
        }
        _ = (&mut recv_task) => {
            send_task.abort();
            cleanup_on_disconnect(&client_id_for_cleanup, &redis_state_for_cleanup).await;
        }
    };
}

async fn handle_client_message(
    client_id: &str,
    msg: Message,
    clients: &Clients,
    redis_state: &RedisState,
    rabbitmq_state: &RabbitMQState,
) {
    let msg = msg.to_str().unwrap_or_default();
    
    let ws_message: WSMessage = match serde_json::from_str(msg) {
        Ok(message) => message,
        Err(e) => {
            log_error_msg(client_id, msg, &e.to_string());
            return;
        }
    };

    match ws_message.action.as_str() {
        "create_topic" => {
            handle_create_topic(
                client_id,
                &ws_message.topic,
                clients,
                redis_state,
            ).await;
        }
        "subscribe" => {
            handle_subscribe(
                client_id,
                &ws_message.topic,
                clients,
                redis_state,
            ).await;
        }
        "publish" => {
            if let Some(message) = ws_message.message {
                let payload = serde_json::json!({
                    "action": "message",
                    "topic": ws_message.topic,
                    "message": message,
                    "sender": client_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });

                if let Ok(payload_str) = serde_json::to_string(&payload) {
                    println!("Publishing message to RabbitMQ: {}", payload_str);
                    if let Err(e) = rabbitmq_state.channel
                        .basic_publish(
                            "message_fanout",
                            "",  // No routing key for fanout
                            BasicPublishOptions::default(),
                            payload_str.as_bytes(),
                            BasicProperties::default(),
                        )
                        .await
                    {
                        eprintln!("Failed to publish to RabbitMQ: {}", e);
                        return;
                    }
                }
            }
        }
        _ => {
            log_error_msg(client_id, msg, "unknown action");
        }
    }
}

async fn handle_rabbitmq_message(
    payload: &str,
    clients: &Clients,
    redis_state: &RedisState,
) {
    println!("Received RabbitMQ message: {}", payload);
    if let Ok(message) = serde_json::from_str::<serde_json::Value>(payload) {
        if let (Some(topic), Some(sender)) = (
            message.get("topic").and_then(Value::as_str),
            message.get("sender").and_then(Value::as_str)
        ) {
            let mut conn = redis_state.connection.clone();
            let subscribers: Vec<String> = conn
                .smembers(format!("topics:{}:subscribers", topic))
                .await
                .unwrap_or_default();

            // Forward message only to subscribers on this server
            let clients_guard = clients.read().await;
            for subscriber_id in subscribers {
                if subscriber_id == sender {
                    continue; // Skip sender
                }

                // Check if subscriber belongs to this server
                let server_id: String = match conn
                    .hget(
                        format!("topics:{}:subscribers:{}", topic, subscriber_id),
                        "server_id"
                    )
                    .await
                {
                    Ok(sid) => sid,
                    Err(_) => continue,
                };

                if server_id == redis_state.server_id {
                    if let Some(client) = clients_guard.get(&subscriber_id) {
                        println!("Forwarding message to client {}", subscriber_id);
                        let _ = client.sender.send(Message::text(payload.to_string()));
                    }
                }
            }
        }
    }
}

pub async fn start_rabbitmq_consumer(
    rabbitmq_state: Arc<RabbitMQState>,
    redis_state: Arc<RedisState>,
    clients: Clients,
) {
    println!("Starting RabbitMQ consumer for server {}", rabbitmq_state.server_id);
    
    // Declare fanout exchange
    if let Err(e) = rabbitmq_state.channel
        .exchange_declare(
            "message_fanout",
            ExchangeKind::Fanout,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
    {
        eprintln!("Failed to declare exchange: {}", e);
        return;
    }

    // Create a queue with a unique name for this server
    let queue_name = format!("queue_{}", rabbitmq_state.server_id);
    if let Err(e) = rabbitmq_state.channel
        .queue_declare(
            &queue_name,
            QueueDeclareOptions {
                auto_delete: true,  // Queue will be deleted when consumer disconnects
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
    {
        eprintln!("Failed to declare queue: {}", e);
        return;
    }

    // Bind queue to fanout exchange
    if let Err(e) = rabbitmq_state.channel
        .queue_bind(
            &queue_name,
            "message_fanout",
            "",  // Routing key is ignored for fanout exchanges
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
    {
        eprintln!("Failed to bind queue: {}", e);
        return;
    }

    // Start consuming messages
    let mut consumer = match rabbitmq_state.channel
        .basic_consume(
            &queue_name,
            &rabbitmq_state.server_id,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
    {
        Ok(consumer) => consumer,
        Err(e) => {
            eprintln!("Failed to create consumer: {}", e);
            return;
        }
    };

    println!("RabbitMQ consumer started successfully");

    // Process messages
    while let Some(delivery) = consumer.next().await {
        if let Ok(delivery) = delivery {
            if let Ok(payload) = String::from_utf8(delivery.data.clone()) {
                handle_rabbitmq_message(&payload, &clients, &redis_state).await;
            }
            let _ = delivery.ack(BasicAckOptions::default()).await;
        }
    }
}

pub async fn start_cleanup_task(
    redis_state: Arc<RedisState>,
    clients: Clients,
) {
    let mut interval = interval(Duration::from_secs(60));
    let redis_state = redis_state.as_ref(); // Get a reference to the inner RedisState
    
    loop {
        interval.tick().await;
        cleanup_disconnected_clients(redis_state, &clients).await;
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

async fn handle_create_topic(
    client_id: &str,
    topic_name: &str,
    clients: &Clients,
    redis_state: &RedisState,
) {
    let mut conn = redis_state.connection.clone();
    let topic_key = format!("topics:{}", topic_name);
    
    // Check if topic exists in Redis
    let exists: bool = conn.exists(&topic_key).await.unwrap_or(false);
    if exists {
        log_rejected(client_id, "create_topic", topic_name, "topic already exists");
        send_error(client_id, "create_topic", topic_name, 409, "Topic already exists", clients).await;
        return;
    }

    // Create new topic in Redis
    let created_at = chrono::Utc::now().to_rfc3339();
    let _: Result<(), RedisError> = redis::pipe()
        .hset(&topic_key, "name", topic_name)
        .hset(&topic_key, "creator", client_id)
        .hset(&topic_key, "created_at", &created_at)
        .query_async(&mut conn)
        .await;

    // Add creator as first subscriber
    let _: Result<(), RedisError> = conn.sadd(
        format!("topics:{}:subscribers", topic_name),
        client_id
    ).await;

    send_success(
        client_id,
        "create_topic",
        topic_name,
        "Topic created successfully",
        clients
    ).await;
}

async fn handle_subscribe(
    client_id: &str,
    topic_name: &str,
    clients: &Clients,
    redis_state: &RedisState,
) {
    let mut conn = redis_state.connection.clone();
    let topic_key = format!("topics:{}", topic_name);

    // Check if topic exists
    let exists: bool = conn.exists(&topic_key).await.unwrap_or(false);
    if !exists {
        send_error(
            client_id,
            "subscribe",
            topic_name,
            404,
            "Topic does not exist",
            clients
        ).await;
        return;
    }

    // Add subscriber to both the set and store their server info
    let _: Result<(), RedisError> = redis::pipe()
        // Add to subscribers set
        .sadd(
            format!("topics:{}:subscribers", topic_name),
            client_id
        )
        // Store subscriber metadata
        .hset(
            format!("topics:{}:subscribers:{}", topic_name, client_id),
            "server_id",
            &redis_state.server_id
        )
        .query_async(&mut conn)
        .await;

    println!("Added subscriber {} to topic {} on server {}", 
        client_id, topic_name, redis_state.server_id);

    send_success(
        client_id,
        "subscribe",
        topic_name,
        "Subscribed successfully",
        clients
    ).await;
}

async fn cleanup_on_disconnect(client_id: &str, redis_state: &RedisState) {
    let mut conn = redis_state.connection.clone();
    
    // Get all topics this client is subscribed to
    let topic_pattern = format!("topics:*:subscribers:{}", client_id);
    let topic_keys: Vec<String> = conn.keys(&topic_pattern).await.unwrap_or_default();
    
    let mut pipe = redis::pipe();
    
    // Remove client from all topic subscriptions
    for topic_key in topic_keys {
        if let Some(topic) = topic_key.split(':').nth(1) {
            pipe.srem(
                format!("topics:{}:subscribers", topic),
                client_id
            );
            pipe.del(format!("topics:{}:subscribers:{}", topic, client_id));
        }
    }
    
    // Remove client record
    pipe.del(format!("clients:{}", client_id));
    
    let _: Result<(), RedisError> = pipe.query_async(&mut conn).await;
} 