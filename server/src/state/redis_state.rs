use redis::aio::ConnectionManager;

pub struct RedisState {
    pub connection: ConnectionManager,
    pub url: String,
    pub server_id: String,
}

impl RedisState {
    pub async fn new() -> Result<Self, redis::RedisError> {
        let url = format!("redis://{}", std::env::var("REDIS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()));
        let client = redis::Client::open(url.as_str())?;
        let connection = redis::aio::ConnectionManager::new(client).await?;
        
        // Use the full Docker container ID as the server ID
        let server_id = std::env::var("HOSTNAME")
            .unwrap_or_else(|_| "unknown".to_string());
        
        println!("Initializing Redis state with server_id: {}", server_id);
        
        Ok(RedisState {
            connection,
            url,
            server_id,
        })
    }
} 