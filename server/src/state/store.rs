use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::models::{Client, Topic};

pub type Clients = Arc<RwLock<HashMap<String, Client>>>;
pub type Topics = Arc<RwLock<HashMap<String, Topic>>>; 