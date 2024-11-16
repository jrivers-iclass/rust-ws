use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use redis::{AsyncCommands, RedisError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topic {
    pub name: String,
    pub creator: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    pub subscribers: Vec<String>,
}

impl Topic {
    pub async fn from_redis(
        conn: &mut redis::aio::ConnectionManager,
        topic_name: &str
    ) -> Result<Option<Self>, RedisError> {
        let topic_key = format!("topics:{}", topic_name);
        
        // Check if topic exists
        let exists: bool = conn.exists(&topic_key).await?;
        if !exists {
            return Ok(None);
        }

        // Get topic data
        let (name, creator, created_at): (String, String, String) = redis::pipe()
            .hget(&topic_key, "name")
            .hget(&topic_key, "creator")
            .hget(&topic_key, "created_at")
            .query_async(conn)
            .await?;

        // Get subscribers
        let subscribers: Vec<String> = conn.smembers(format!("{}:subscribers", topic_key)).await?;

        Ok(Some(Topic {
            name,
            creator,
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            subscribers,
        }))
    }
} 