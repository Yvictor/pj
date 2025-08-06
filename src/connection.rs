use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tracing::info;

static CONNECTION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub id: u64,
    pub client_addr: SocketAddr,
    pub proxy_addr: String,
    pub backend_addr: String,
    pub start_instant: Instant,
    pub active_connections: u64,
}

impl ConnectionInfo {
    pub fn new(client_addr: SocketAddr, proxy_addr: &str, backend_addr: &str, active_connections: u64) -> Self {
        let id = CONNECTION_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            client_addr,
            proxy_addr: proxy_addr.to_string(),
            backend_addr: backend_addr.to_string(),
            start_instant: Instant::now(),
            active_connections,
        }
    }

    pub fn log_start(&self) {
        info!(
            "Conn #{} estab [{}]: {} -> {} -> {}",
            self.id,
            self.active_connections,
            self.client_addr,
            self.proxy_addr,
            self.backend_addr
        );
    }

    pub fn log_end(&self, bytes_sent: u64, bytes_received: u64, error: Option<&str>, remaining_connections: u64) {
        let duration = self.start_instant.elapsed();
        let status = if error.is_some() { "fail " } else { "close" };
        
        info!(
            "Conn #{} {} [{}]: Duration: {:.2}s | Sent: {} | Received: {}{}",
            self.id,
            status,
            remaining_connections,
            duration.as_secs_f64(),
            format_bytes(bytes_sent),
            format_bytes(bytes_received),
            error.map(|e| format!(" | Error: {}", e)).unwrap_or_default()
        );
    }
}

#[derive(Debug, Default)]
pub struct ConnectionStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl ConnectionStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_sent(&mut self, bytes: usize) {
        self.bytes_sent += bytes as u64;
    }

    pub fn add_received(&mut self, bytes: usize) {
        self.bytes_received += bytes as u64;
    }
}