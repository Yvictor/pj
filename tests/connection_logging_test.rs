use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

#[tokio::test]
async fn test_connection_logging_basic() {
    let echo_server_addr = "127.0.0.1:21001";
    let proxy_listen_addr = "127.0.0.1:21002";
    
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
    
    // Start proxy with stderr capture to get logs
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, echo_server_addr)])
        .env("PJ_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Connect and send data
    let mut client = TcpStream::connect(proxy_listen_addr).await.expect("Failed to connect to proxy");
    let test_data = b"Test connection logging";
    client.write_all(test_data).await.expect("Failed to write data");
    
    let mut buffer = vec![0u8; test_data.len()];
    client.read_exact(&mut buffer).await.expect("Failed to read response");
    assert_eq!(&buffer[..], test_data);
    
    // Close connection
    drop(client);
    sleep(Duration::from_millis(500)).await;
    
    // Kill proxy and collect output
    proxy_process.kill().expect("Failed to kill proxy");
    
    let output = proxy_process.wait_with_output().expect("Failed to get proxy output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined_output = format!("{}\n{}", stderr, stdout);
    
    // Check for connection logging
    println!("Proxy output:\n{}", combined_output);
    
    // Verify log contains connection information
    assert!(combined_output.contains("Conn #"), 
            "Should log connection info");
    assert!(combined_output.contains("estab") || combined_output.contains("Adding proxy mapping"), 
            "Should log connection establishment or proxy setup");
    assert!(combined_output.contains(&proxy_listen_addr), "Should log proxy address");
    assert!(combined_output.contains(&echo_server_addr), "Should log backend address");
    assert!(combined_output.contains("closed") || combined_output.contains("Duration:"), 
            "Should log connection close with stats");
}

#[tokio::test]
async fn test_connection_logging_with_stats() {
    let echo_server_addr = "127.0.0.1:21003";
    let proxy_listen_addr = "127.0.0.1:21004";
    
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
    
    // Start proxy
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, echo_server_addr)])
        .env("PJ_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Connect and send multiple data packets
    let mut client = TcpStream::connect(proxy_listen_addr).await.expect("Failed to connect to proxy");
    
    let data1 = b"First packet";
    let data2 = b"Second packet";
    let data3 = b"Third packet";
    
    client.write_all(data1).await.expect("Failed to write data1");
    let mut buf1 = vec![0u8; data1.len()];
    client.read_exact(&mut buf1).await.expect("Failed to read response1");
    
    client.write_all(data2).await.expect("Failed to write data2");
    let mut buf2 = vec![0u8; data2.len()];
    client.read_exact(&mut buf2).await.expect("Failed to read response2");
    
    client.write_all(data3).await.expect("Failed to write data3");
    let mut buf3 = vec![0u8; data3.len()];
    client.read_exact(&mut buf3).await.expect("Failed to read response3");
    
    // Close connection
    drop(client);
    sleep(Duration::from_millis(500)).await;
    
    // Kill proxy and collect output
    proxy_process.kill().expect("Failed to kill proxy");
    
    let output = proxy_process.wait_with_output().expect("Failed to get proxy output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined_output = format!("{}\n{}", stderr, stdout);
    
    println!("Proxy output:\n{}", combined_output);
    
    // Verify stats in logs
    assert!(combined_output.contains("Conn #"), "Should log connection info");
    assert!(combined_output.contains("estab"), "Should log connection establishment");
    assert!(combined_output.contains("close") || combined_output.contains("fail"), 
            "Should log connection close status");
    assert!(combined_output.contains("Duration:"), "Should log connection duration");
    assert!(combined_output.contains("Sent:") && combined_output.contains("Received:"), 
            "Should log data transfer stats");
    // Check for human-readable format (B, KB, MB, GB)
    assert!(combined_output.contains(" B") || combined_output.contains(" KB") || 
            combined_output.contains(" MB") || combined_output.contains(" GB"), 
            "Should use human-readable byte format");
}

#[tokio::test]
async fn test_connection_logging_with_error() {
    let unreachable_addr = "127.0.0.1:21099";
    let proxy_listen_addr = "127.0.0.1:21005";
    
    // Start proxy pointing to unreachable address
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, unreachable_addr)])
        .env("PJ_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Try to connect
    match TcpStream::connect(proxy_listen_addr).await {
        Ok(mut stream) => {
            let _ = stream.write_all(b"Test").await;
            sleep(Duration::from_millis(500)).await;
        }
        Err(_) => {
            println!("Connection refused (expected)");
        }
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // Kill proxy and collect output
    proxy_process.kill().expect("Failed to kill proxy");
    
    let output = proxy_process.wait_with_output().expect("Failed to get proxy output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined_output = format!("{}\n{}", stderr, stdout);
    
    println!("Proxy output:\n{}", combined_output);
    
    // Should log connection failure
    assert!(combined_output.contains("Failed to create client session") || 
            combined_output.contains("Connection refused") ||
            combined_output.contains("failed") ||
            combined_output.contains("Failed") ||
            combined_output.contains("21099"),  // The unreachable port should appear in logs
            "Should log connection failures or unreachable address");
}

#[tokio::test]
async fn test_multiple_connections_logging() {
    let echo_server_addr = "127.0.0.1:21006";
    let proxy_listen_addr = "127.0.0.1:21007";
    
    // Start echo server
    let echo_listener = TcpListener::bind(echo_server_addr).await.expect("Failed to bind echo server");
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = echo_listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = [0; 1024];
                loop {
                    match socket.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if socket.write_all(&buf[0..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });
    
    // Start proxy
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, echo_server_addr)])
        .env("PJ_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Create multiple concurrent connections
    let mut handles = vec![];
    for i in 0..3 {
        let handle = tokio::spawn(async move {
            let mut client = TcpStream::connect(proxy_listen_addr).await.unwrap();
            let message = format!("Connection {}", i);
            client.write_all(message.as_bytes()).await.unwrap();
            
            let mut buffer = vec![0u8; message.len()];
            client.read_exact(&mut buffer).await.unwrap();
            
            // Keep connection open for different durations
            sleep(Duration::from_millis(100 * (i as u64 + 1))).await;
        });
        handles.push(handle);
    }
    
    // Wait for all connections to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // Kill proxy and collect output
    proxy_process.kill().expect("Failed to kill proxy");
    
    let output = proxy_process.wait_with_output().expect("Failed to get proxy output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined_output = format!("{}\n{}", stderr, stdout);
    
    println!("Proxy output:\n{}", combined_output);
    
    // Count connection logs
    let connection_count = combined_output.matches("Conn #").count();
    assert!(connection_count >= 3, "Should log at least 3 connections, found {}", connection_count);
    
    // Should have unique connection IDs
    assert!(combined_output.contains("Conn #0"), "Should have connection 0");
    assert!(combined_output.contains("Conn #1"), "Should have connection 1");
    assert!(combined_output.contains("Conn #2"), "Should have connection 2");
}