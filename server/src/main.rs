mod handlers;
mod models;
mod state;
mod utils;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;
use redis::AsyncCommands;

use crate::state::{Clients, Topics, RedisState};
use crate::handlers::websocket::{ws_handler, start_redis_subscriber, start_cleanup_task};
use crate::utils::logger::setup_logger;
use crate::models::topic::Topic;

async fn load_topics_from_redis(
    redis_state: &RedisState,
    topics: &Topics
) -> Result<(), redis::RedisError> {
    let mut conn = redis_state.connection.clone();
    
    // Get all topic keys
    let topic_keys: Vec<String> = conn.keys("topics:*").await?;
    
    let mut topics_guard = topics.write().await;
    for topic_key in topic_keys {
        let topic_name = topic_key.strip_prefix("topics:").unwrap_or(&topic_key);
        if let Ok(Some(topic)) = Topic::from_redis(&mut conn, topic_name).await {
            topics_guard.insert(topic_name.to_string(), topic);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    setup_logger();

    let clients: Clients = Arc::new(RwLock::new(HashMap::new()));
    let topics: Topics = Arc::new(RwLock::new(HashMap::new()));
    
    // Initialize Redis state
    let redis_state = Arc::new(
        RedisState::new()
            .await
            .expect("Failed to connect to Redis")
    );

    // Load existing topics from Redis
    if let Err(e) = load_topics_from_redis(&redis_state, &topics).await {
        eprintln!("Failed to load topics from Redis: {}", e);
    }

    // Start Redis subscriber
    let redis_clients = clients.clone();
    let redis_state_clone = redis_state.clone();
    tokio::spawn(async move {
        start_redis_subscriber(redis_state_clone, redis_clients).await;
    });

    // Start cleanup task
    let cleanup_clients = clients.clone();
    let cleanup_redis_state = redis_state.clone();
    tokio::spawn(async move {
        start_cleanup_task(cleanup_redis_state, cleanup_clients).await;
    });

    // Create filters
    let clients = warp::any().map(move || clients.clone());
    let topics = warp::any().map(move || topics.clone());
    let redis = warp::any().map(move || redis_state.clone());

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(clients)
        .and(topics)
        .and(redis)
        .and_then(ws_handler);

    println!("Server started at localhost:8000");
    warp::serve(ws_route).run(([0, 0, 0, 0], 8000)).await;
}
