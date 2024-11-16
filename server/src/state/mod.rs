use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use redis::aio::ConnectionManager;
use tokio::sync::mpsc::UnboundedSender;
use warp::ws::Message;
use crate::models::Topic;

pub type Clients = Arc<RwLock<HashMap<String, Client>>>;
pub type Topics = Arc<RwLock<HashMap<String, Topic>>>;

pub struct RedisState {
    pub connection: ConnectionManager,
    pub server_id: String,
    pub url: String,
}

impl RedisState {
    pub async fn new() -> Result<Self, redis::RedisError> {
        let url = format!("redis://{}", std::env::var("REDIS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()));
        let client = redis::Client::open(url.as_str())?;
        let connection = redis::aio::ConnectionManager::new(client).await?;
        
        let server_id = std::env::var("SERVER_ID").unwrap_or_else(|_| "ws1".to_string());
        
        Ok(RedisState {
            connection,
            server_id,
            url,
        })
    }
}

pub struct Client {
    pub sender: UnboundedSender<Message>,
}