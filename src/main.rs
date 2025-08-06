#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

use clap::{CommandFactory, Parser};
use pingora_core::server::{configuration::Opt, Server};
use std::process;
use tracing::{error, info};

use pj::{parse_proxy_mapping, proxy_service, ProxyMapping};

#[derive(Parser, Debug)]
#[command(author, version, about = "A TCP reverse proxy built with pingora")]
struct Args {
    /// Proxy mappings in format "listen_addr:proxy_addr" (e.g., "0.0.0.0:8787:127.0.0.1:22")
    /// Can be specified multiple times for multiple mappings
    #[arg(short, long, value_parser = parse_proxy_mapping)]
    proxy: Vec<ProxyMapping>,
}

fn main() {
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    if args.proxy.is_empty() {
        let mut cmd = Args::command();
        if let Err(e) = cmd.print_help() {
            eprintln!("Failed to print help: {}", e);
        }
        process::exit(1);
    }
    let proxy_count = args.proxy.len();
    
    let opt = Some(Opt::default());
    let mut server = match Server::new(opt) {
        Ok(server) => server,
        Err(e) => {
            error!("Failed to create server: {}", e);
            process::exit(1);
        }
    };
    
    server.bootstrap();
    
    for mapping in args.proxy {
        let proxy = proxy_service(&mapping.listen_addr, &mapping.proxy_addr);
        server.add_service(proxy);
        
        info!("Adding proxy mapping - listening on {}, proxying to {}", 
              mapping.listen_addr, mapping.proxy_addr);
    }
    
    info!("Starting proxy server with {} mappings", proxy_count);
    server.run_forever();
}