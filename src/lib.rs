use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::select;
use tracing::debug;

use pingora_core::apps::ServerApp;
use pingora_core::connectors::TransportConnector;
use pingora_core::listeners::Listeners;
use pingora_core::protocols::Stream;
use pingora_core::server::ShutdownWatch;
use pingora_core::services::listening::Service;
use pingora_core::upstreams::peer::BasicPeer;

pub struct ProxyApp {
    client_connector: TransportConnector,
    proxy_to: BasicPeer,
}

enum DuplexEvent {
    DownstreamRead(usize),
    UpstreamRead(usize),
}

impl ProxyApp {
    pub fn new(proxy_to: BasicPeer) -> Self {
        ProxyApp {
            client_connector: TransportConnector::new(None),
            proxy_to,
        }
    }

    pub async fn duplex(&self, mut server_session: Stream, mut client_session: Stream) {
        let mut upstream_buf = [0; 1024];
        let mut downstream_buf = [0; 1024];
        
        loop {
            let downstream_read = server_session.read(&mut upstream_buf);
            let upstream_read = client_session.read(&mut downstream_buf);
            let event: DuplexEvent;
            select! {
                n = downstream_read => event
                    = DuplexEvent::DownstreamRead(n.unwrap()),
                n = upstream_read => event
                    = DuplexEvent::UpstreamRead(n.unwrap()),
            }
            match event {
                DuplexEvent::DownstreamRead(0) => {
                    debug!("downstream session closing");
                    return;
                }
                DuplexEvent::UpstreamRead(0) => {
                    debug!("upstream session closing");
                    return;
                }
                DuplexEvent::DownstreamRead(n) => {
                    client_session.write_all(&upstream_buf[0..n]).await.unwrap();
                    client_session.flush().await.unwrap();
                }
                DuplexEvent::UpstreamRead(n) => {
                    server_session
                        .write_all(&downstream_buf[0..n])
                        .await
                        .unwrap();
                    server_session.flush().await.unwrap();
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
        let client_session = self.client_connector.new_stream(&self.proxy_to).await;

        match client_session {
            Ok(client_session) => {
                self.duplex(io, client_session).await;
                None
            }
            Err(e) => {
                debug!("Failed to create client session: {}", e);
                None
            }
        }
    }
}

pub fn proxy_service(addr: &str, proxy_addr: &str) -> Service<ProxyApp> {
    let proxy_to = BasicPeer::new(proxy_addr);

    Service::with_listeners(
        "Proxy Service".to_string(),
        Listeners::tcp(addr),
        ProxyApp::new(proxy_to),
    )
}

#[derive(Debug, Clone)]
pub struct ProxyMapping {
    pub listen_addr: String,
    pub proxy_addr: String,
}

pub fn parse_proxy_mapping(s: &str) -> Result<ProxyMapping, String> {
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
        let mapping = result.unwrap();
        assert_eq!(mapping.listen_addr, "127.0.0.1:8080");
        assert_eq!(mapping.proxy_addr, "192.168.1.1:9090");
    }

    #[test]
    fn test_parse_proxy_mapping_with_localhost() {
        let input = "localhost:8080:localhost:9090";
        let result = parse_proxy_mapping(input);
        
        assert!(result.is_ok());
        let mapping = result.unwrap();
        assert_eq!(mapping.listen_addr, "localhost:8080");
        assert_eq!(mapping.proxy_addr, "localhost:9090");
    }

    #[test]
    fn test_parse_proxy_mapping_with_zeros() {
        let input = "0.0.0.0:80:10.0.0.1:8080";
        let result = parse_proxy_mapping(input);
        
        assert!(result.is_ok());
        let mapping = result.unwrap();
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
        let proxy_app = ProxyApp::new(peer.clone());
        
        assert_eq!(proxy_app.proxy_to._address, peer._address);
    }

    #[test]
    fn test_proxy_service_creation() {
        let listen_addr = "127.0.0.1:8000";
        let proxy_addr = "127.0.0.1:9000";
        
        let service = proxy_service(listen_addr, proxy_addr);
        
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