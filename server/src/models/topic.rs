#[derive(Debug, Clone)]
pub struct Topic {
    pub password: Option<String>,
    pub subscribers: Vec<String>,
} 