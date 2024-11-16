use futures_util::{StreamExt, SinkExt};
use tokio::sync::mpsc;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};
use redis::{AsyncCommands, RedisError};
use redis::aio::ConnectionManager;
use std::sync::Arc;
use std::collections::HashMap;
use serde_json::Value;
use std::time::Duration;
use tokio::time::interval;
use lapin::{
    options::*, types::FieldTable, BasicProperties,
    ExchangeKind, message::Delivery,
};
use tokio::sync::Mutex as TokioMutex;

use crate::state::{Clients, RedisState, Client, RabbitMQState};
use crate::models::{WSMessage, WSError};
use crate::utils::logger::{
    log_connection, log_disconnection, log_rejected,
};

// Add this type alias for our error type
type BoxedError = Box<dyn std::error::Error + Send + Sync>;

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
    _topic: &str,
    message: &str,
    clients: &Clients,
) {
    if let Some(client) = clients.read().await.get(client_id) {
        let success_message = serde_json::json!({
            "action": action,
            "topic": _topic,
            "message": message,
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
                    let _ = handle_client_message(
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
) -> Result<(), BoxedError> {
    let msg_str = msg.to_str().map_err(|_| "Invalid message format")?;
    let ws_message: WSMessage = serde_json::from_str(msg_str)?;

    match ws_message.action.as_str() {
        "create_topic" => {
            if let Some(topic) = ws_message.message {
                handle_create_topic(client_id, &topic, clients, redis_state).await;
            }
        }
        "subscribe" => {
            let topic = &ws_message.topic;
            println!("Handling subscribe request for topic: {}", topic);
            handle_subscribe(client_id, topic, clients, redis_state).await;
        }
        "publish" | "message" => {
            let topic = &ws_message.topic;
            let message = ws_message.message
                .ok_or("Message content is required")?;

            // Check if topic exists
            let mut conn = redis_state.connection.clone();
            let topic_key = format!("topics:{}", topic);
            let exists: bool = conn.exists(&topic_key).await.unwrap_or(false);
            if !exists {
                send_error(
                    client_id,
                    ws_message.action.as_str(),
                    topic,
                    404,
                    "Topic does not exist",
                    clients
                ).await;
                return Ok(());
            }

            let payload = serde_json::json!({
                "action": "message",
                "message": message,
                "topic": topic,
                "sender": client_id,
                "timestamp": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            });

            let payload_str = serde_json::to_string(&payload)?;
            println!("Publishing message to RabbitMQ: {}", payload_str);

            if let Err(e) = publish_to_rabbitmq(&payload_str, topic, rabbitmq_state).await {
                eprintln!("Failed to publish to RabbitMQ: {}", e);
                send_error(
                    client_id,
                    ws_message.action.as_str(),
                    topic,
                    500,
                    "Failed to publish message",
                    clients
                ).await;
                return Ok(());
            }

            send_success(
                client_id,
                ws_message.action.as_str(),
                topic,
                "Message sent",
                clients
            ).await;
        }
        _ => {
            eprintln!("Unknown action: {}", ws_message.action);
        }
    }

    Ok(())
}

async fn publish_to_rabbitmq(
    message: &str,
    _topic: &str,
    rabbitmq_state: &RabbitMQState,
) -> Result<(), BoxedError> {
    rabbitmq_state.channel
        .basic_publish(
            "message_fanout",  // Use fanout exchange for broadcasting
            "",  // Routing key is ignored for fanout exchanges
            BasicPublishOptions::default(),
            message.as_bytes(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2), // persistent delivery
        )
        .await?
        .await?;

    Ok(())
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
    
    // Set up both exchanges
    rabbitmq_state.channel
        .exchange_declare(
            "message_fanout",  // For broadcasting messages to all servers
            ExchangeKind::Fanout,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .expect("Failed to declare fanout exchange");

    // Create a queue for this server
    let queue_name = format!("server_{}", redis_state.server_id);
    rabbitmq_state.channel
        .queue_declare(
            &queue_name,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .expect("Failed to declare queue");

    // Bind the queue to the fanout exchange
    rabbitmq_state.channel
        .queue_bind(
            &queue_name,
            "message_fanout",
            "",  // Routing key is ignored for fanout exchanges
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("Failed to bind queue");

    // Set QoS
    if let Err(e) = rabbitmq_state.channel
        .basic_qos(1000, BasicQosOptions::default())
        .await 
    {
        eprintln!("Failed to set QoS: {}", e);
        return;
    }

    // Create the consumer
    let mut consumer = rabbitmq_state.channel
        .basic_consume(
            &queue_name,
            &format!("consumer_{}", redis_state.server_id),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("Failed to create consumer");

    let conn = Arc::new(TokioMutex::new(redis_state.connection.clone()));
    let topic_subscribers = Arc::new(TokioMutex::new(HashMap::new()));
    let subscriber_servers = Arc::new(TokioMutex::new(HashMap::new()));
    
    println!("RabbitMQ consumer started, waiting for messages...");

    while let Some(delivery_result) = consumer.next().await {
        match delivery_result {
            Ok(delivery) => {
                let conn_clone = Arc::clone(&conn);
                let topic_subscribers_clone = Arc::clone(&topic_subscribers);
                let subscriber_servers_clone = Arc::clone(&subscriber_servers);
                let clients_clone = clients.clone();
                let redis_state_clone = redis_state.clone();

                match process_delivery(
                    &delivery,
                    &conn_clone,
                    &clients_clone,
                    &redis_state_clone,
                    &topic_subscribers_clone,
                    &subscriber_servers_clone
                ).await {
                    Ok(_) => {
                        if let Err(e) = delivery.ack(BasicAckOptions {
                            multiple: false,
                        }).await {
                            eprintln!("Error acknowledging message: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error processing message: {}", e);
                        if let Err(nack_err) = delivery.nack(BasicNackOptions {
                            requeue: false,
                            multiple: false,
                        }).await {
                            eprintln!("Error sending nack: {}", nack_err);
                        }
                    }
                }

                // Clear caches periodically
                let subs_len = topic_subscribers.lock().await.len();
                if subs_len > 1000 {
                    topic_subscribers.lock().await.clear();
                    subscriber_servers.lock().await.clear();
                }
            }
            Err(e) => {
                eprintln!("Error receiving message: {}", e);
            }
        }
    }
}

async fn process_delivery(
    delivery: &Delivery,
    _conn: &Arc<TokioMutex<ConnectionManager>>,
    clients: &Clients,
    redis_state: &RedisState,
    _topic_subscribers: &Arc<TokioMutex<HashMap<String, Vec<String>>>>,
    _subscriber_servers: &Arc<TokioMutex<HashMap<String, HashMap<String, String>>>>
) -> Result<(), BoxedError> {
    let payload = String::from_utf8(delivery.data.clone())?;
    handle_rabbitmq_message(&payload, clients, redis_state).await;
    Ok(())
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
    println!("Starting subscribe process for client {} to topic {}", client_id, topic_name);
    let mut conn = redis_state.connection.clone();
    let topic_key = format!("topics:{}", topic_name);

    // Check if topic exists
    let exists: bool = conn.exists(&topic_key).await.unwrap_or(false);
    if !exists {
        println!("Topic {} does not exist", topic_name);
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
    let pipe_result: Result<(), RedisError> = redis::pipe()
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

    match pipe_result {
        Ok(_) => {
            println!("Successfully subscribed client {} to topic {} on server {}", 
                client_id, topic_name, redis_state.server_id);
            
            send_success(
                client_id,
                "subscribe",
                topic_name,
                "Subscribed successfully",
                clients
            ).await;
        }
        Err(e) => {
            eprintln!("Failed to subscribe client {} to topic {}: {}", 
                client_id, topic_name, e);
            
            send_error(
                client_id,
                "subscribe",
                topic_name,
                500,
                "Failed to subscribe to topic",
                clients
            ).await;
        }
    }
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