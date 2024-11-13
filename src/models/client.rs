use tokio::sync::mpsc::UnboundedSender;
use warp::ws::Message;

#[derive(Debug, Clone)]
pub struct Client {
    pub sender: UnboundedSender<Message>,
} 