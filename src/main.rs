#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

use clap::{CommandFactory, Parser};
use pingora_core::server::{configuration::Opt, Server};
use std::env;
use std::process;
use std::sync::Arc;
use tracing::{error, info};

use pj::{parse_proxy_mapping, proxy_service, ProxyMapping};
use pj::id_manager::{ConnectionIdManager, parse_duration, parse_count};

#[derive(Parser, Debug)]
#[command(
    author, 
    version, 
    about = "A TCP reverse proxy built with pingora",
    after_help = "ENVIRONMENT VARIABLES:
  PJ_PROXY    Single proxy mapping (same format as --proxy)
  PJ_PROXIES  Multiple proxy mappings, comma or semicolon separated
  PJ_LOG      Set logging level (error, warn, info, debug, trace)
              Default: info
              Examples: 
                PJ_LOG=debug - Enable debug logging for all modules
                PJ_LOG=pj=trace - Trace logging for pj module only
                PJ_LOG=warn,pj=info - Warn globally, info for pj
              Note: Falls back to RUST_LOG if PJ_LOG is not set
  
  PJ_CONN_ID_RESET_INTERVAL  Time interval for connection ID reset
              Format: [number][unit] (d=days, h=hours, m=minutes, s=seconds)
              Default: None (no reset by time)
              Examples: 6h, 30m, 1d, 1d12h
  
  PJ_CONN_ID_RESET_COUNT     Count threshold for connection ID reset
              Format: number or with units (k=thousand, m=million, g=billion)
              Default: None (no reset by count)
              Examples: 100k, 10m, 1g, 500000

EXAMPLES:
  # Using command line arguments
  pj --proxy 0.0.0.0:8787:127.0.0.1:22
  
  # Using environment variables
  PJ_PROXY=\"0.0.0.0:8787:127.0.0.1:22\" pj
  PJ_PROXIES=\"0.0.0.0:8787:127.0.0.1:22,0.0.0.0:8080:127.0.0.1:80\" pj
  
  # With custom logging level
  PJ_LOG=debug pj --proxy 0.0.0.0:8787:127.0.0.1:22
  
  # With connection ID reset settings
  PJ_CONN_ID_RESET_INTERVAL=6h PJ_CONN_ID_RESET_COUNT=100k pj --proxy 0.0.0.0:8787:127.0.0.1:22"
)]
struct Args {
    /// Proxy mapping in format "listen_ip:listen_port:proxy_ip:proxy_port"
    /// Can be specified multiple times for multiple mappings
    #[arg(short, long, value_parser = parse_proxy_mapping)]
    proxy: Vec<ProxyMapping>,
}

fn main() {
    // Initialize tracing with PJ_LOG (fallback to RUST_LOG) environment variable support
    // Default to "info" if neither is set
    let filter = env::var("PJ_LOG")
        .or_else(|_| env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info".to_string());
    
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::new(filter)
        )
        .init();
    
    let args = Args::parse();
    
    // Collect proxy mappings from command line or environment variables
    let mut proxy_mappings = Vec::new();
    
    // Priority 1: Command line arguments
    if !args.proxy.is_empty() {
        proxy_mappings = args.proxy;
        info!("Using proxy mappings from command line arguments");
    } 
    // Priority 2: PJ_PROXIES environment variable (multiple mappings)
    else if let Ok(env_mappings) = env::var("PJ_PROXIES") {
        for mapping_str in env_mappings.split(|c| c == ',' || c == ';') {
            let trimmed = mapping_str.trim();
            if !trimmed.is_empty() {
                match parse_proxy_mapping(trimmed) {
                    Ok(mapping) => {
                        proxy_mappings.push(mapping);
                    },
                    Err(e) => {
                        error!("Failed to parse proxy mapping '{}': {}", trimmed, e);
                    }
                }
            }
        }
        if !proxy_mappings.is_empty() {
            info!("Using {} proxy mappings from PJ_PROXIES environment variable", proxy_mappings.len());
        }
    }
    // Priority 3: PJ_PROXY environment variable (single mapping)
    else if let Ok(env_proxy) = env::var("PJ_PROXY") {
        match parse_proxy_mapping(&env_proxy) {
            Ok(mapping) => {
                proxy_mappings.push(mapping);
                info!("Using proxy mapping from PJ_PROXY environment variable");
            },
            Err(e) => {
                error!("Failed to parse PJ_PROXY environment variable '{}': {}", env_proxy, e);
            }
        }
    }
    
    // If no proxy mappings found, show help
    if proxy_mappings.is_empty() {
        eprintln!("No proxy mappings specified.");
        eprintln!("Use --proxy flag, PJ_PROXY, or PJ_PROXIES environment variable.\n");
        let mut cmd = Args::command();
        if let Err(e) = cmd.print_help() {
            eprintln!("Failed to print help: {}", e);
        }
        process::exit(1);
    }
    
    let proxy_count = proxy_mappings.len();
    
    // Parse connection ID reset settings from environment variables
    let reset_interval = env::var("PJ_CONN_ID_RESET_INTERVAL")
        .ok()
        .and_then(|s| {
            match parse_duration(&s) {
                Ok(duration) => {
                    info!("Connection ID reset interval: {}", s);
                    Some(duration)
                }
                Err(e) => {
                    error!("Invalid PJ_CONN_ID_RESET_INTERVAL '{}': {}", s, e);
                    None
                }
            }
        });
    
    let reset_count = env::var("PJ_CONN_ID_RESET_COUNT")
        .ok()
        .and_then(|s| {
            match parse_count(&s) {
                Ok(count) => {
                    info!("Connection ID reset count threshold: {}", s);
                    Some(count)
                }
                Err(e) => {
                    error!("Invalid PJ_CONN_ID_RESET_COUNT '{}': {}", s, e);
                    None
                }
            }
        });
    
    // Log reset configuration
    match (reset_interval, reset_count) {
        (None, None) => info!("Connection ID reset disabled (no reset interval or count threshold set)"),
        (Some(_), None) => info!("Connection ID reset by time interval only"),
        (None, Some(_)) => info!("Connection ID reset by count threshold only"),
        (Some(_), Some(_)) => info!("Connection ID reset by time interval or count threshold"),
    }
    
    // Create shared ID manager
    let id_manager = Arc::new(ConnectionIdManager::new(reset_interval, reset_count));
    
    let opt = Some(Opt::default());
    let mut server = match Server::new(opt) {
        Ok(server) => server,
        Err(e) => {
            error!("Failed to create server: {}", e);
            process::exit(1);
        }
    };
    
    server.bootstrap();
    
    for mapping in proxy_mappings {
        let proxy = proxy_service(&mapping.listen_addr, &mapping.proxy_addr, id_manager.clone());
        server.add_service(proxy);
        
        info!("Adding proxy mapping - listening on {}, proxying to {}", 
              mapping.listen_addr, mapping.proxy_addr);
    }
    
    info!("Starting proxy server with {} mappings", proxy_count);
    server.run_forever();
}