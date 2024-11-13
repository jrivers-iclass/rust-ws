use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, error, warn};
use tracing_subscriber::EnvFilter;
use once_cell::sync::Lazy;

static VERBOSE_LOGGING: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

pub fn setup_logger() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .with_line_number(true)
        .with_file(true)
        .with_env_filter(env_filter)
        .init();

    info!("Logger initialized");
}

#[allow(dead_code)]
pub fn toggle_verbose_logging() -> bool {
    let new_state = !VERBOSE_LOGGING.load(Ordering::Relaxed);
    VERBOSE_LOGGING.store(new_state, Ordering::Relaxed);
    
    info!("Verbose logging {}", if new_state { "enabled" } else { "disabled" });
    new_state
}

pub fn log_connection(client_id: &str) {
    info!("Client connected: {}", client_id);
}

pub fn log_disconnection(client_id: &str) {
    info!("Client disconnected: {}", client_id);
}

pub fn log_message(client_id: &str, action: &str, topic: &str) {
    if VERBOSE_LOGGING.load(Ordering::Relaxed) {
        info!("Client {} {} topic {}", client_id, action, topic);
    }
}

pub fn log_rejected(client_id: &str, action: &str, topic: &str, reason: &str) {
    warn!("Rejected {} from client {} to topic {}: {}", action, client_id, topic, reason);
}

pub fn log_error_msg(client_id: &str, action: &str, error: &str) {
    error!("Client {} error during {}: {}", client_id, action, error);
} 