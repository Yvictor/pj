#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

use async_trait::async_trait;
use tracing::{debug, info};

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::select;

use pingora_core::apps::ServerApp;
use pingora_core::connectors::TransportConnector;
use pingora_core::protocols::Stream;
use pingora_core::server::{Server, ShutdownWatch};
use pingora_core::upstreams::peer::BasicPeer;

use pingora_core::listeners::Listeners;
use pingora_core::services::listening::Service;

use pingora_core::server::configuration::Opt;

use clap::{Parser, CommandFactory};

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

    async fn duplex(&self, mut server_session: Stream, mut client_session: Stream) {
        let mut upstream_buf = [0; 1024];
        let mut downstream_buf = [0; 1024];
        // debug!("duplex, server_session: {:?}, client_session: {:?}", server_session, client_session);
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

#[derive(Parser, Debug)]
#[command(author, version, about = "A TCP reverse proxy built with pingora")]
struct Args {
    /// Proxy mappings in format "listen_addr:proxy_addr" (e.g., "0.0.0.0:8787:127.0.0.1:22")
    /// Can be specified multiple times for multiple mappings
    #[arg(short, long, value_parser = parse_proxy_mapping)]
    proxy: Vec<ProxyMapping>,
}

#[derive(Debug, Clone)]
struct ProxyMapping {
    listen_addr: String,
    proxy_addr: String,
}

// Parser for the proxy mapping argument
fn parse_proxy_mapping(s: &str) -> Result<ProxyMapping, String> {
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

fn main() {
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    // Check if at least one proxy mapping is provided
    if args.proxy.is_empty() {
        let mut cmd = Args::command();
        cmd.print_help().unwrap();
        std::process::exit(1);
    }
    let proxy_count = args.proxy.len();
    
    let opt = Some(Opt::default());
    let mut server = Server::new(opt).unwrap();
    server.bootstrap();
    
    // Add a service for each proxy mapping
    for mapping in args.proxy {
        let proxy = proxy_service(&mapping.listen_addr, &mapping.proxy_addr);
        server.add_service(proxy);
        
        info!("Adding proxy mapping - listening on {}, proxying to {}", 
              mapping.listen_addr, mapping.proxy_addr);
    }
    
    info!("Starting proxy server with {} mappings", proxy_count);
    server.run_forever();
}
