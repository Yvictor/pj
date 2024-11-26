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
    /// Address to listen on (e.g., "0.0.0.0:8787")
    #[arg(short, long)]
    listen_addr: Option<String>,

    /// Address to proxy to (e.g., "127.0.0.1:22")
    #[arg(short, long)]
    proxy_addr: Option<String>,
}

fn main() {
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    // Check if both addresses are provided, if not show usage and exit
    let (listen_addr, proxy_addr) = match (args.listen_addr, args.proxy_addr) {
        (Some(l), Some(p)) => (l, p),
        _ => {
            let mut cmd = Args::command();
            cmd.print_help().unwrap();
            std::process::exit(1);
        }
    };
    
    // let opt = Some(Opt::parse_args());
    let opt = Some(Opt::default());
    let mut server = Server::new(opt).unwrap();
    server.bootstrap();
    
    let proxy = proxy_service(&listen_addr, &proxy_addr);
    server.add_service(proxy);
    
    info!("Starting proxy server - listening on {}, proxying to {}", 
          listen_addr, proxy_addr);
    
    server.run_forever();
}
