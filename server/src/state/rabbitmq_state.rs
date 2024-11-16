use lapin::{Connection, Channel};
use std::time::Duration;
use tokio::time::sleep;

pub struct RabbitMQState {
    pub connection: Connection,
    pub channel: Channel,
    pub server_id: String,
}

impl RabbitMQState {
    pub async fn new() -> Result<Self, lapin::Error> {
        let amqp_url = std::env::var("AMQP_URL")
            .unwrap_or_else(|_| "amqp://guest:guest@localhost:5672".to_string());
        
        let mut retry_count = 0;
        let max_retries = 5;
        
        loop {
            match Connection::connect(
                &amqp_url,
                lapin::ConnectionProperties::default()
            ).await {
                Ok(connection) => {
                    let channel = connection.create_channel().await?;
                    
                    // Use the full Docker container ID as the server ID
                    let server_id = std::env::var("HOSTNAME")
                        .unwrap_or_else(|_| "unknown".to_string());
                    
                    println!("Successfully connected to RabbitMQ with server_id: {}", server_id);
                    
                    return Ok(RabbitMQState {
                        connection,
                        channel,
                        server_id,
                    });
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= max_retries {
                        return Err(e);
                    }
                    println!("Failed to connect to RabbitMQ, retrying in 5 seconds... (attempt {}/{})", 
                        retry_count, max_retries);
                    sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
} 