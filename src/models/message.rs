use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct WSMessage {
    pub action: String,
    pub topic: String,
    pub message: Option<String>,
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WSError {
    pub action: String,
    pub topic: String,
    pub code: u16,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WSSuccess {
    pub action: String,
    pub topic: String,
    pub code: u16,
    pub message: String,
}

impl WSError {
    pub fn new(action: &str, topic: &str, code: u16, message: &str) -> Self {
        Self {
            action: action.to_string(),
            topic: topic.to_string(),
            code,
            message: message.to_string(),
        }
    }
}

impl WSSuccess {
    pub fn new(action: &str, topic: &str, message: &str) -> Self {
        Self {
            action: action.to_string(),
            topic: topic.to_string(),
            code: 200,
            message: message.to_string(),
        }
    }
} 