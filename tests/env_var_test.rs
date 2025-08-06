use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

#[tokio::test]
async fn test_env_var_single_proxy() {
    let echo_server_addr = "127.0.0.1:22001";
    let proxy_listen_addr = "127.0.0.1:22002";
    
    // Start echo server
    let echo_listener = TcpListener::bind(echo_server_addr).await.expect("Failed to bind echo server");
    tokio::spawn(async move {
        let (mut socket, _) = echo_listener.accept().await.unwrap();
        let mut buf = [0; 1024];
        
        loop {
            match socket.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    socket.write_all(&buf[0..n]).await.unwrap();
                }
                Err(_) => break,
            }
        }
    });
    
    // Start proxy with PJ_PROXY environment variable
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--"])
        .env("PJ_PROXY", format!("{}:{}", proxy_listen_addr, echo_server_addr))
        .env("PJ_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Connect and test
    let mut client = TcpStream::connect(proxy_listen_addr).await.expect("Failed to connect to proxy");
    let test_data = b"Test with env var";
    client.write_all(test_data).await.expect("Failed to write data");
    
    let mut buffer = vec![0u8; test_data.len()];
    client.read_exact(&mut buffer).await.expect("Failed to read response");
    assert_eq!(&buffer[..], test_data);
    
    drop(client);
    
    // Kill proxy
    proxy_process.kill().expect("Failed to kill proxy");
    
    let output = proxy_process.wait_with_output().expect("Failed to get proxy output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}\n{}", stderr, stdout);
    
    println!("Proxy output:\n{}", combined);
    
    // Verify it used environment variable
    assert!(combined.contains("PJ_PROXY environment variable") || 
            combined.contains("proxy mapping from PJ_PROXY"),
            "Should log that it's using PJ_PROXY environment variable");
}

#[tokio::test]
async fn test_env_var_multiple_proxies() {
    let echo_server1_addr = "127.0.0.1:22003";
    let echo_server2_addr = "127.0.0.1:22004";
    let proxy_listen1_addr = "127.0.0.1:22005";
    let proxy_listen2_addr = "127.0.0.1:22006";
    
    // Start echo servers
    let echo_listener1 = TcpListener::bind(echo_server1_addr).await.expect("Failed to bind echo server 1");
    tokio::spawn(async move {
        let (mut socket, _) = echo_listener1.accept().await.unwrap();
        let mut buf = [0; 1024];
        loop {
            match socket.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    socket.write_all(&buf[0..n]).await.unwrap();
                }
                Err(_) => break,
            }
        }
    });
    
    let echo_listener2 = TcpListener::bind(echo_server2_addr).await.expect("Failed to bind echo server 2");
    tokio::spawn(async move {
        let (mut socket, _) = echo_listener2.accept().await.unwrap();
        let mut buf = [0; 1024];
        loop {
            match socket.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    socket.write_all(&buf[0..n]).await.unwrap();
                }
                Err(_) => break,
            }
        }
    });
    
    // Start proxy with PJ_PROXIES environment variable
    let mappings = format!("{}:{},{}:{}", 
        proxy_listen1_addr, echo_server1_addr,
        proxy_listen2_addr, echo_server2_addr);
    
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--"])
        .env("PJ_PROXIES", mappings)
        .env("PJ_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Test first proxy
    let mut client1 = TcpStream::connect(proxy_listen1_addr).await.expect("Failed to connect to proxy 1");
    let test_data1 = b"Test proxy 1";
    client1.write_all(test_data1).await.expect("Failed to write data to proxy 1");
    
    let mut buffer1 = vec![0u8; test_data1.len()];
    client1.read_exact(&mut buffer1).await.expect("Failed to read response from proxy 1");
    assert_eq!(&buffer1[..], test_data1);
    
    // Test second proxy
    let mut client2 = TcpStream::connect(proxy_listen2_addr).await.expect("Failed to connect to proxy 2");
    let test_data2 = b"Test proxy 2";
    client2.write_all(test_data2).await.expect("Failed to write data to proxy 2");
    
    let mut buffer2 = vec![0u8; test_data2.len()];
    client2.read_exact(&mut buffer2).await.expect("Failed to read response from proxy 2");
    assert_eq!(&buffer2[..], test_data2);
    
    drop(client1);
    drop(client2);
    
    // Kill proxy
    proxy_process.kill().expect("Failed to kill proxy");
    
    let output = proxy_process.wait_with_output().expect("Failed to get proxy output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{};\n{}", stderr, stdout);
    
    // Verify it used environment variable
    assert!(combined.contains("PJ_PROXIES environment variable") || 
            combined.contains("2 proxy mappings from PJ_PROXIES"),
            "Should log that it's using PJ_PROXIES environment variable");
}

#[tokio::test]
async fn test_cli_overrides_env_var() {
    let echo_server_addr = "127.0.0.1:22007";
    let proxy_listen_addr = "127.0.0.1:22008";
    let env_server_addr = "127.0.0.1:22009"; // Different, unused address
    
    // Start echo server
    let echo_listener = TcpListener::bind(echo_server_addr).await.expect("Failed to bind echo server");
    tokio::spawn(async move {
        let (mut socket, _) = echo_listener.accept().await.unwrap();
        let mut buf = [0; 1024];
        
        loop {
            match socket.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    socket.write_all(&buf[0..n]).await.unwrap();
                }
                Err(_) => break,
            }
        }
    });
    
    // Start proxy with both environment variable and CLI argument
    // CLI argument should take precedence
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, echo_server_addr)])
        .env("PJ_PROXY", format!("{}:{}", proxy_listen_addr, env_server_addr))
        .env("PJ_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Connect and test - should work because CLI arg points to running server
    let mut client = TcpStream::connect(proxy_listen_addr).await.expect("Failed to connect to proxy");
    let test_data = b"Test CLI override";
    client.write_all(test_data).await.expect("Failed to write data");
    
    let mut buffer = vec![0u8; test_data.len()];
    client.read_exact(&mut buffer).await.expect("Failed to read response");
    assert_eq!(&buffer[..], test_data);
    
    drop(client);
    
    // Kill proxy
    proxy_process.kill().expect("Failed to kill proxy");
    
    let output = proxy_process.wait_with_output().expect("Failed to get proxy output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{};\n{}", stderr, stdout);
    
    // Verify it used command line arguments, not environment variable
    assert!(combined.contains("command line arguments") || 
            combined.contains("Using proxy mappings from command line"),
            "Should log that it's using command line arguments");
    assert!(!combined.contains("PJ_PROXY environment variable"),
            "Should not mention using PJ_PROXY when CLI args are provided");
}

#[tokio::test]
async fn test_env_var_with_semicolon_separator() {
    let echo_server1_addr = "127.0.0.1:22010";
    let echo_server2_addr = "127.0.0.1:22011";
    let proxy_listen1_addr = "127.0.0.1:22012";
    let proxy_listen2_addr = "127.0.0.1:22013";
    
    // Start echo servers
    let echo_listener1 = TcpListener::bind(echo_server1_addr).await.expect("Failed to bind echo server 1");
    tokio::spawn(async move {
        let (mut socket, _) = echo_listener1.accept().await.unwrap();
        let mut buf = [0; 1024];
        loop {
            match socket.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    socket.write_all(&buf[0..n]).await.unwrap();
                }
                Err(_) => break,
            }
        }
    });
    
    let echo_listener2 = TcpListener::bind(echo_server2_addr).await.expect("Failed to bind echo server 2");
    tokio::spawn(async move {
        let (mut socket, _) = echo_listener2.accept().await.unwrap();
        let mut buf = [0; 1024];
        loop {
            match socket.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    socket.write_all(&buf[0..n]).await.unwrap();
                }
                Err(_) => break,
            }
        }
    });
    
    // Start proxy with PJ_PROXIES using semicolon separator
    let mappings = format!("{}:{};{}:{}", 
        proxy_listen1_addr, echo_server1_addr,
        proxy_listen2_addr, echo_server2_addr);
    
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--"])
        .env("PJ_PROXIES", mappings)
        .env("PJ_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Test both proxies
    let mut client1 = TcpStream::connect(proxy_listen1_addr).await.expect("Failed to connect to proxy 1");
    let test_data1 = b"Test semicolon 1";
    client1.write_all(test_data1).await.expect("Failed to write data to proxy 1");
    
    let mut buffer1 = vec![0u8; test_data1.len()];
    client1.read_exact(&mut buffer1).await.expect("Failed to read response from proxy 1");
    assert_eq!(&buffer1[..], test_data1);
    
    let mut client2 = TcpStream::connect(proxy_listen2_addr).await.expect("Failed to connect to proxy 2");
    let test_data2 = b"Test semicolon 2";
    client2.write_all(test_data2).await.expect("Failed to write data to proxy 2");
    
    let mut buffer2 = vec![0u8; test_data2.len()];
    client2.read_exact(&mut buffer2).await.expect("Failed to read response from proxy 2");
    assert_eq!(&buffer2[..], test_data2);
    
    drop(client1);
    drop(client2);
    
    // Kill proxy
    proxy_process.kill().expect("Failed to kill proxy");
}