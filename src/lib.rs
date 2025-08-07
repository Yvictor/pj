use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::select;
use tracing::{debug, warn};

use pingora_core::apps::ServerApp;
use pingora_core::connectors::TransportConnector;
use pingora_core::listeners::Listeners;
use pingora_core::protocols::Stream;
use pingora_core::server::ShutdownWatch;
use pingora_core::services::listening::Service;
use pingora_core::upstreams::peer::BasicPeer;

pub mod error;
pub mod connection;
pub mod id_manager;
pub use error::{ProxyError, Result};
use connection::{ConnectionInfo, ConnectionStats};
use id_manager::ConnectionIdManager;

pub struct ProxyApp {
    client_connector: TransportConnector,
    proxy_to: BasicPeer,
    listen_addr: String,
    active_connections: Arc<AtomicU64>,
    id_manager: Arc<ConnectionIdManager>,
}

enum DuplexEvent {
    DownstreamRead(usize),
    UpstreamRead(usize),
}

impl ProxyApp {
    pub fn new(proxy_to: BasicPeer, listen_addr: String, id_manager: Arc<ConnectionIdManager>) -> Self {
        ProxyApp {
            client_connector: TransportConnector::new(None),
            proxy_to,
            listen_addr,
            active_connections: Arc::new(AtomicU64::new(0)),
            id_manager,
        }
    }

    pub async fn duplex(&self, mut server_session: Stream, mut client_session: Stream, conn_info: ConnectionInfo, active_connections: Arc<AtomicU64>) {
        let mut upstream_buf = [0; 1024];
        let mut downstream_buf = [0; 1024];
        let mut stats = ConnectionStats::new();
        
        conn_info.log_start();
        
        loop {
            let downstream_read = server_session.read(&mut upstream_buf);
            let upstream_read = client_session.read(&mut downstream_buf);
            let event: DuplexEvent;
            select! {
                n = downstream_read => {
                    match n {
                        Ok(n) => event = DuplexEvent::DownstreamRead(n),
                        Err(e) => {
                            warn!("Downstream read error: {}", e);
                            let remaining = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                            conn_info.log_end(stats.bytes_sent, stats.bytes_received, Some(&e.to_string()), remaining);
                            return;
                        }
                    }
                }
                n = upstream_read => {
                    match n {
                        Ok(n) => event = DuplexEvent::UpstreamRead(n),
                        Err(e) => {
                            warn!("Upstream read error: {}", e);
                            let remaining = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                            conn_info.log_end(stats.bytes_sent, stats.bytes_received, Some(&e.to_string()), remaining);
                            return;
                        }
                    }
                }
            }
            match event {
                DuplexEvent::DownstreamRead(0) => {
                    debug!("Downstream session closing");
                    let remaining = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                    conn_info.log_end(stats.bytes_sent, stats.bytes_received, None, remaining);
                    return;
                }
                DuplexEvent::UpstreamRead(0) => {
                    debug!("Upstream session closing");
                    let remaining = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                    conn_info.log_end(stats.bytes_sent, stats.bytes_received, None, remaining);
                    return;
                }
                DuplexEvent::DownstreamRead(n) => {
                    stats.add_received(n);
                    if let Err(e) = client_session.write_all(&upstream_buf[0..n]).await {
                        warn!("Failed to write to client session: {}", e);
                        let remaining = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                        conn_info.log_end(stats.bytes_sent, stats.bytes_received, Some(&e.to_string()), remaining);
                        return;
                    }
                    if let Err(e) = client_session.flush().await {
                        warn!("Failed to flush client session: {}", e);
                        let remaining = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                        conn_info.log_end(stats.bytes_sent, stats.bytes_received, Some(&e.to_string()), remaining);
                        return;
                    }
                }
                DuplexEvent::UpstreamRead(n) => {
                    stats.add_sent(n);
                    if let Err(e) = server_session.write_all(&downstream_buf[0..n]).await {
                        warn!("Failed to write to server session: {}", e);
                        let remaining = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                        conn_info.log_end(stats.bytes_sent, stats.bytes_received, Some(&e.to_string()), remaining);
                        return;
                    }
                    if let Err(e) = server_session.flush().await {
                        warn!("Failed to flush server session: {}", e);
                        let remaining = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                        conn_info.log_end(stats.bytes_sent, stats.bytes_received, Some(&e.to_string()), remaining);
                        return;
                    }
                }
            }
        }
    }
}

#[async_trait]
impl ServerApp for ProxyApp {
    async fn process_new(
        self: &Arc<Self>,
        io: Stream,
        _shutdown: &ShutdownWatch,
    ) -> Option<Stream> {
        // Try to get client address from the stream's socket digest
        let client_socket_addr = {
            use std::net::{IpAddr, Ipv4Addr, SocketAddr};
            
            io.get_socket_digest()
                .and_then(|digest| digest.peer_addr.get().cloned())
                .and_then(|opt_addr| opt_addr)
                .and_then(|addr| {
                    addr.as_inet().map(|inet| {
                        let ip = IpAddr::from(inet.ip().to_canonical());
                        SocketAddr::new(ip, inet.port())
                    })
                })
                .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0))
        };
        
        let client_session = self.client_connector.new_stream(&self.proxy_to).await;

        match client_session {
            Ok(client_session) => {
                // Increment active connections counter
                let current_connections = self.active_connections.fetch_add(1, Ordering::Relaxed) + 1;
                
                let conn_info = ConnectionInfo::new(
                    client_socket_addr,
                    &self.listen_addr,
                    &self.proxy_to._address.to_string(),
                    current_connections,
                    &self.id_manager
                );
                
                self.duplex(io, client_session, conn_info, self.active_connections.clone()).await;
                None
            }
            Err(e) => {
                warn!("Failed to create client session to {}: {}", self.proxy_to._address, e);
                None
            }
        }
    }
}

pub fn proxy_service(addr: &str, proxy_addr: &str, id_manager: Arc<ConnectionIdManager>) -> Service<ProxyApp> {
    let proxy_to = BasicPeer::new(proxy_addr);

    Service::with_listeners(
        "Proxy Service".to_string(),
        Listeners::tcp(addr),
        ProxyApp::new(proxy_to, addr.to_string(), id_manager),
    )
}

#[derive(Debug, Clone)]
pub struct ProxyMapping {
    pub listen_addr: String,
    pub proxy_addr: String,
}

pub fn parse_proxy_mapping(s: &str) -> std::result::Result<ProxyMapping, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 4 {
        return Err("Invalid proxy mapping format. Expected format: listen_ip:listen_port:proxy_ip:proxy_port".to_string());
    }

    let listen_addr = format!("{}:{}", parts[0], parts[1]);
    let proxy_addr = format!("{}:{}", parts[2], parts[3]);

    Ok(ProxyMapping {
        listen_addr,
        proxy_addr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pingora_core::services::Service as ServiceTrait;

    #[test]
    fn test_parse_proxy_mapping_valid() {
        let input = "127.0.0.1:8080:192.168.1.1:9090";
        let result = parse_proxy_mapping(input);
        
        assert!(result.is_ok());
        let mapping = result.expect("Failed to parse valid mapping");
        assert_eq!(mapping.listen_addr, "127.0.0.1:8080");
        assert_eq!(mapping.proxy_addr, "192.168.1.1:9090");
    }

    #[test]
    fn test_parse_proxy_mapping_with_localhost() {
        let input = "localhost:8080:localhost:9090";
        let result = parse_proxy_mapping(input);
        
        assert!(result.is_ok());
        let mapping = result.expect("Failed to parse localhost mapping");
        assert_eq!(mapping.listen_addr, "localhost:8080");
        assert_eq!(mapping.proxy_addr, "localhost:9090");
    }

    #[test]
    fn test_parse_proxy_mapping_with_zeros() {
        let input = "0.0.0.0:80:10.0.0.1:8080";
        let result = parse_proxy_mapping(input);
        
        assert!(result.is_ok());
        let mapping = result.expect("Failed to parse zeros mapping");
        assert_eq!(mapping.listen_addr, "0.0.0.0:80");
        assert_eq!(mapping.proxy_addr, "10.0.0.1:8080");
    }

    #[test]
    fn test_parse_proxy_mapping_invalid_format() {
        let test_cases = vec![
            "127.0.0.1:8080",
            "127.0.0.1:8080:192.168.1.1",
            "127.0.0.1",
            "",
            "127.0.0.1:8080:192.168.1.1:9090:extra",
        ];

        for input in test_cases {
            let result = parse_proxy_mapping(input);
            assert!(result.is_err(), "Expected error for input: {}", input);
        }
    }

    #[test]
    fn test_proxy_app_creation() {
        let peer = BasicPeer::new("127.0.0.1:8080");
        let listen_addr = "0.0.0.0:8787".to_string();
        let id_manager = Arc::new(ConnectionIdManager::new(None, None));
        let proxy_app = ProxyApp::new(peer.clone(), listen_addr.clone(), id_manager);
        
        assert_eq!(proxy_app.proxy_to._address, peer._address);
        assert_eq!(proxy_app.listen_addr, listen_addr);
    }

    #[test]
    fn test_proxy_service_creation() {
        let listen_addr = "127.0.0.1:8000";
        let proxy_addr = "127.0.0.1:9000";
        let id_manager = Arc::new(ConnectionIdManager::new(None, None));
        
        let service = proxy_service(listen_addr, proxy_addr, id_manager);
        
        assert_eq!(service.name(), "Proxy Service");
    }

    #[test]
    fn test_proxy_mapping_clone() {
        let mapping = ProxyMapping {
            listen_addr: "127.0.0.1:8080".to_string(),
            proxy_addr: "192.168.1.1:9090".to_string(),
        };
        
        let cloned = mapping.clone();
        assert_eq!(cloned.listen_addr, mapping.listen_addr);
        assert_eq!(cloned.proxy_addr, mapping.proxy_addr);
    }

    #[test]
    fn test_proxy_mapping_debug() {
        let mapping = ProxyMapping {
            listen_addr: "127.0.0.1:8080".to_string(),
            proxy_addr: "192.168.1.1:9090".to_string(),
        };
        
        let debug_str = format!("{:?}", mapping);
        assert!(debug_str.contains("127.0.0.1:8080"));
        assert!(debug_str.contains("192.168.1.1:9090"));
    }

    #[test]
    fn test_duplex_event_downstream_read() {
        let event = DuplexEvent::DownstreamRead(100);
        match event {
            DuplexEvent::DownstreamRead(n) => assert_eq!(n, 100),
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_duplex_event_upstream_read() {
        let event = DuplexEvent::UpstreamRead(200);
        match event {
            DuplexEvent::UpstreamRead(n) => assert_eq!(n, 200),
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_duplex_event_sizes() {
        assert_eq!(std::mem::size_of::<DuplexEvent>(), 16);
    }
}