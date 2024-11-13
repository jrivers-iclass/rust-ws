mod handlers;
mod models;
mod state;
mod utils;

use std::collections::HashMap;
use warp::Filter;

use crate::state::{Clients, Topics};
use crate::models::Topic;
use crate::handlers::client_connection;
use crate::utils::logger::setup_logger;

// Dependency injection for clients state
fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || clients.clone())
}

// Dependency injection for topics state
fn with_topics(topics: Topics) -> impl Filter<Extract = (Topics,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || topics.clone())
}

async fn ws_handler(ws: warp::ws::Ws, clients: Clients, topics: Topics) -> Result<impl warp::Reply, warp::Rejection> {  
    Ok(ws.on_upgrade(move |socket| client_connection(socket, clients, topics)))
}

#[tokio::main]
async fn main() {
    setup_logger();

    let clients: Clients = std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new()));
    let topics: Topics = std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new()));

    // Initialize system topic
    topics.write().await.insert("system".to_string(), Topic {
        password: None,
        subscribers: Vec::new(),
    });

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_clients(clients.clone()))
        .and(with_topics(topics.clone()))
        .and_then(ws_handler);

    let health_route = warp::path!("health").map(|| "OK");
    let routes = health_route.or(ws_route);
    
    println!("Server started at ws://127.0.0.1:8000");
    warp::serve(routes).run(([127, 0, 0, 1], 8000)).await;
}
