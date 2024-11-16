use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedSender;
use warp::ws::Message;

mod redis_state;
mod rabbitmq_state;

pub use redis_state::RedisState;
pub use rabbitmq_state::RabbitMQState;

pub type Clients = Arc<RwLock<HashMap<String, Client>>>;

pub struct Client {
    pub sender: UnboundedSender<Message>,
}