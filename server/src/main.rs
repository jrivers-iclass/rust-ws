mod handlers;
mod models;
mod state;
mod utils;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

use crate::state::{Clients, RedisState, RabbitMQState};
use crate::handlers::websocket::{ws_handler, start_rabbitmq_consumer, start_cleanup_task};
use crate::utils::logger::setup_logger;

#[tokio::main]
async fn main() {
    setup_logger();

    let clients: Clients = Arc::new(RwLock::new(HashMap::new()));
    
    // Initialize states
    let redis_state = Arc::new(
        RedisState::new()
            .await
            .expect("Failed to connect to Redis")
    );

    let rabbitmq_state = Arc::new(
        RabbitMQState::new()
            .await
            .expect("Failed to connect to RabbitMQ")
    );

    // Start RabbitMQ consumer
    let consumer_clients = clients.clone();
    let consumer_rabbitmq = rabbitmq_state.clone();
    let consumer_redis = redis_state.clone();
    tokio::spawn(async move {
        start_rabbitmq_consumer(
            consumer_rabbitmq,
            consumer_redis,
            consumer_clients
        ).await;
    });

    // Start cleanup task
    let cleanup_clients = clients.clone();
    let cleanup_redis_state = redis_state.clone();
    tokio::spawn(async move {
        start_cleanup_task(cleanup_redis_state, cleanup_clients).await;
    });

    // Create filters
    let clients = warp::any().map(move || clients.clone());
    let redis = warp::any().map(move || redis_state.clone());
    let rabbitmq = warp::any().map(move || rabbitmq_state.clone());

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(clients)
        .and(redis)
        .and(rabbitmq)
        .and_then(ws_handler);

    println!("Server started at localhost:8000");
    warp::serve(ws_route)
        .run(([0, 0, 0, 0], 8000))
        .await;
}
