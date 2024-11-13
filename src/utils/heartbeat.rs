use std::time::{Duration, Instant};
use tracing::info;

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct HeartbeatManager {
    pub last_heartbeat: Instant,
}

impl HeartbeatManager {
    pub fn new() -> Self {
        let manager = Self {
            last_heartbeat: Instant::now(),
        };
        info!("HeartbeatManager initialized");
        manager
    }

    pub fn update(&mut self) {
        self.last_heartbeat = Instant::now();
        info!("Heartbeat updated, timeout in {} seconds", HEARTBEAT_TIMEOUT.as_secs());
    }

    pub fn is_alive(&self) -> bool {
        let elapsed = self.last_heartbeat.elapsed();
        let alive = elapsed < HEARTBEAT_TIMEOUT;
        info!(
            "Heartbeat check: elapsed={:?}, timeout={:?}, alive={}",
            elapsed,
            HEARTBEAT_TIMEOUT,
            alive
        );
        alive
    }
} 