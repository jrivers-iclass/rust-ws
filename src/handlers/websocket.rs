use futures_util::{StreamExt, SinkExt};
use tokio::sync::mpsc;
use warp::ws::{Message, WebSocket};
use uuid::Uuid;
use crate::models::{WSMessage, Client, Topic};
use crate::models::message::{WSError, WSSuccess};
use crate::state::{Clients, Topics};
use crate::utils::logger::{
    log_connection, 
    log_disconnection, 
    log_message,
    log_rejected,
    log_error_msg
};
use chrono;

async fn send_error(
    client_id: &str, 
    action: &str, 
    topic: &str, 
    code: u16, 
    message: &str,
    clients: &Clients
) {
    let error = WSError::new(action, topic, code, message);
    let error_str = serde_json::to_string(&error).unwrap_or_default();
    
    if let Some(client) = clients.read().await.get(client_id) {
        let _ = client.sender.send(Message::text(error_str));
    }
}

async fn send_success(
    client_id: &str,
    action: &str,
    topic: &str,
    message: &str,
    clients: &Clients
) {
    let success = WSSuccess::new(action, topic, message);
    let success_str = serde_json::to_string(&success).unwrap_or_default();
    
    if let Some(client) = clients.read().await.get(client_id) {
        let _ = client.sender.send(Message::text(success_str));
    }
}

async fn handle_client_message(client_id: &str, msg: Message, clients: &Clients, topics: &Topics) {
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

    log_message(client_id, &ws_message.action, &ws_message.topic);

    match ws_message.action.as_str() {
        "create" => {
            let mut topics = topics.write().await;
            if topics.contains_key(&ws_message.topic) {
                log_rejected(client_id, "create", &ws_message.topic, "topic already exists");
                send_error(client_id, "create", &ws_message.topic, 409, "Topic already exists", clients).await;
                return;
            }
            topics.insert(ws_message.topic.clone(), Topic {
                password: ws_message.password,
                subscribers: Vec::new(),
            });
            send_success(client_id, "create", &ws_message.topic, "Topic created successfully", clients).await;
        }
        "subscribe" => {
            let mut topics = topics.write().await;
            if let Some(topic) = topics.get_mut(&ws_message.topic) {
                if let Some(ref topic_password) = topic.password {
                    if ws_message.password.as_ref() != Some(topic_password) {
                        log_rejected(client_id, "subscribe", &ws_message.topic, "incorrect password");
                        send_error(client_id, "subscribe", &ws_message.topic, 401, "Incorrect password", clients).await;
                        return;
                    }
                }
                if !topic.subscribers.contains(&client_id.to_string()) {
                    topic.subscribers.push(client_id.to_string());
                    send_success(client_id, "subscribe", &ws_message.topic, "Subscribed successfully", clients).await;
                }
            } else {
                log_rejected(client_id, "subscribe", &ws_message.topic, "topic does not exist");
                send_error(client_id, "subscribe", &ws_message.topic, 404, "Topic does not exist", clients).await;
            }
        }
        "publish" => {
            let topics_guard = topics.read().await;
            let topic = match topics_guard.get(&ws_message.topic) {
                Some(t) => t,
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

            let mut sent_count = 0;
            {
                let clients_guard = clients.read().await;
                for subscriber_id in &topic.subscribers {
                    if subscriber_id != client_id {
                        if let Some(client) = clients_guard.get(subscriber_id) {
                            let publish_message = serde_json::json!({
                                "action": "publish",
                                "topic": ws_message.topic,
                                "message": message_content,
                                "sender": client_id,
                                "timestamp": chrono::Utc::now().to_rfc3339()
                            });
                            if client.sender.send(Message::text(publish_message.to_string())).is_ok() {
                                sent_count += 1;
                            }
                        }
                    }
                }
            }
            send_success(
                client_id,
                "publish",
                &ws_message.topic,
                &format!("Message sent to {} subscribers", sent_count),
                clients
            ).await;
        }
        _ => {
            log_rejected(client_id, &ws_message.action, &ws_message.topic, "unknown action");
            send_error(client_id, &ws_message.action, &ws_message.topic, 400, "Unknown action", clients).await;
        }
    }
}

pub async fn client_connection(ws: WebSocket, clients: Clients, topics: Topics) {
    let (mut client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, mut client_rcv) = mpsc::unbounded_channel::<Message>();

    let client_id = Uuid::new_v4().to_string();
    log_connection(&client_id);

    // Add client to clients map
    {
        let mut clients = clients.write().await;
        clients.insert(client_id.clone(), Client {
            sender: client_sender,
        });
    }

    // Create and/or subscribe to general topic
    {
        let mut topics = topics.write().await;
        if !topics.contains_key("general") {
            // Create general topic if it doesn't exist
            topics.insert("general".to_string(), Topic {
                password: None,
                subscribers: Vec::new(),
            });
            log_message(&client_id, "create", "general");
        }
        
        // Subscribe to general topic
        if let Some(topic) = topics.get_mut("general") {
            if !topic.subscribers.contains(&client_id) {
                topic.subscribers.push(client_id.clone());
                log_message(&client_id, "subscribe", "general");
                
                // Send success message to client
                if let Some(client) = clients.read().await.get(&client_id) {
                    let success = WSSuccess::new("subscribe", "general", "Subscribed to general topic");
                    if let Ok(success_str) = serde_json::to_string(&success) {
                        let _ = client.sender.send(Message::text(success_str));
                    }
                }
            }
        }
    }

    // Subscribe to system topic (if you want to keep this)
    {
        let mut topics = topics.write().await;
        if let Some(topic) = topics.get_mut("system") {
            topic.subscribers.push(client_id.clone());
        }
    }

    // Clone client_id for the send task
    let send_task_client_id = client_id.clone();

    // Forward messages from client_rcv to client_ws_sender
    let ws_send_task = tokio::spawn(async move {
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
            handle_client_message(&client_id, msg, &clients, &topics).await;
        } else if msg.is_binary() {
            log_error_msg(&client_id, "message", "binary messages not supported");
        } else if msg.is_close() {
            println!("Received close from client {}", client_id);
            break;
        }
    }

    // Cleanup on disconnect
    log_disconnection(&client_id);
    clients.write().await.remove(&client_id);
    let mut topics = topics.write().await;
    for topic in topics.values_mut() {
        topic.subscribers.retain(|id| id != &client_id);
    }

    // Cancel the send task
    ws_send_task.abort();
} 