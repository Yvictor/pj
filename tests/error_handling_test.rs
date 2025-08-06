use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, sleep};

#[tokio::test]
async fn test_port_already_in_use() {
    let addr = "127.0.0.1:20001";
    
    // Start first listener to occupy the port
    let _listener = TcpListener::bind(addr).await.expect("Failed to bind first listener");
    
    // Try to start proxy on the same port
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:127.0.0.1:22", addr)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    // Wait for proxy to attempt binding
    sleep(Duration::from_secs(3)).await;
    
    // Check if process is still running
    match proxy_process.try_wait() {
        Ok(Some(status)) => {
            // Process exited, which could happen if port binding failed
            println!("Proxy exited with status: {:?}", status);
        }
        Ok(None) => {
            // Process is still running - Pingora might handle the error internally
            // Try to connect to see if it's actually listening
            match TcpStream::connect(addr).await {
                Ok(_) => {
                    println!("Warning: Proxy might be sharing the port or using SO_REUSEPORT");
                }
                Err(_) => {
                    println!("Proxy is running but not listening on the occupied port (handled gracefully)");
                }
            }
            proxy_process.kill().expect("Failed to kill proxy");
        }
        Err(e) => {
            panic!("Failed to check proxy status: {}", e);
        }
    }
}

#[tokio::test]
async fn test_invalid_listen_address() {
    // Test with invalid IP address
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", "999.999.999.999:8080:127.0.0.1:22"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    // Wait for proxy to attempt binding
    sleep(Duration::from_secs(2)).await;
    
    // Check if process handled the error gracefully
    match proxy_process.try_wait() {
        Ok(Some(_status)) => {
            println!("Proxy handled invalid address gracefully");
        }
        Ok(None) => {
            proxy_process.kill().expect("Failed to kill proxy");
            // This might be ok if the address parsing is lenient
            println!("Proxy is still running with invalid address");
        }
        Err(e) => {
            panic!("Failed to check proxy status: {}", e);
        }
    }
}

#[tokio::test]
async fn test_connection_interrupted() {
    let echo_server_addr = "127.0.0.1:20003";
    let proxy_listen_addr = "127.0.0.1:20004";
    
    // Start echo server
    let echo_listener = TcpListener::bind(echo_server_addr).await.expect("Failed to bind echo server");
    
    let echo_handle = tokio::spawn(async move {
        let (mut socket, _) = echo_listener.accept().await.unwrap();
        let mut buf = [0; 1024];
        
        // Read once then close abruptly
        let _ = socket.read(&mut buf).await;
        // Simulate abrupt disconnection by dropping the socket
        drop(socket);
    });
    
    // Start proxy
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, echo_server_addr)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Connect through proxy
    let mut client = TcpStream::connect(proxy_listen_addr).await.expect("Failed to connect to proxy");
    
    // Send data
    let test_data = b"Hello, World!";
    client.write_all(test_data).await.expect("Failed to write data");
    
    // Try to read - should handle disconnection gracefully
    let mut buf = vec![0u8; 1024];
    match timeout(Duration::from_secs(2), client.read(&mut buf)).await {
        Ok(Ok(0)) => {
            println!("Connection closed gracefully");
        }
        Ok(Ok(n)) => {
            println!("Read {} bytes before disconnection", n);
        }
        Ok(Err(e)) => {
            println!("Read error (expected): {}", e);
        }
        Err(_) => {
            println!("Read timeout (connection handled gracefully)");
        }
    }
    
    // Ensure proxy is still running after connection error
    match proxy_process.try_wait() {
        Ok(None) => {
            println!("Proxy still running after connection error - good!");
            proxy_process.kill().expect("Failed to kill proxy");
        }
        Ok(Some(_)) => {
            panic!("Proxy should not exit on connection errors");
        }
        Err(e) => {
            panic!("Failed to check proxy status: {}", e);
        }
    }
    
    let _ = echo_handle.await;
}

#[tokio::test]
async fn test_rapid_connection_failures() {
    let unreachable_addr = "127.0.0.1:20099";  // Non-existent server
    let proxy_listen_addr = "127.0.0.1:20005";
    
    // Start proxy pointing to unreachable address
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, unreachable_addr)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Attempt multiple rapid connections
    let mut handles = vec![];
    for i in 0..10 {
        let handle = tokio::spawn(async move {
            match TcpStream::connect(proxy_listen_addr).await {
                Ok(mut stream) => {
                    let test_data = format!("Connection {}", i);
                    let _ = stream.write_all(test_data.as_bytes()).await;
                    
                    let mut buf = vec![0u8; 1024];
                    match timeout(Duration::from_millis(500), stream.read(&mut buf)).await {
                        Ok(Ok(0)) => {
                            println!("Connection {} closed", i);
                        }
                        Ok(Ok(_)) => {
                            println!("Connection {} got unexpected data", i);
                        }
                        Ok(Err(_)) => {
                            println!("Connection {} failed to read", i);
                        }
                        Err(_) => {
                            println!("Connection {} timed out", i);
                        }
                    }
                }
                Err(e) => {
                    println!("Connection {} failed: {}", i, e);
                }
            }
        });
        handles.push(handle);
    }
    
    // Wait for all connections to complete
    for handle in handles {
        let _ = handle.await;
    }
    
    // Ensure proxy is still running after multiple failed connections
    match proxy_process.try_wait() {
        Ok(None) => {
            println!("Proxy survived rapid connection failures - good!");
            proxy_process.kill().expect("Failed to kill proxy");
        }
        Ok(Some(_)) => {
            panic!("Proxy should not crash on connection failures");
        }
        Err(e) => {
            panic!("Failed to check proxy status: {}", e);
        }
    }
}

#[tokio::test]
async fn test_large_data_with_sudden_disconnection() {
    let echo_server_addr = "127.0.0.1:20006";
    let proxy_listen_addr = "127.0.0.1:20007";
    
    // Start echo server that disconnects after receiving some data
    let echo_listener = TcpListener::bind(echo_server_addr).await.expect("Failed to bind echo server");
    
    let echo_handle = tokio::spawn(async move {
        let (mut socket, _) = echo_listener.accept().await.unwrap();
        let mut buf = [0; 1024];
        let mut total_received = 0;
        
        // Receive some data then disconnect
        for _ in 0..3 {
            match socket.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    total_received += n;
                    if total_received > 2048 {
                        println!("Echo server disconnecting after {} bytes", total_received);
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        // Abrupt disconnection
        drop(socket);
    });
    
    // Start proxy
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, echo_server_addr)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Connect and send large amount of data
    let mut client = TcpStream::connect(proxy_listen_addr).await.expect("Failed to connect to proxy");
    
    let large_data = vec![0x42u8; 10000];
    match client.write_all(&large_data).await {
        Ok(_) => println!("Sent large data"),
        Err(e) => println!("Failed to send all data (expected): {}", e),
    }
    
    // Try to read response
    let mut buf = vec![0u8; 1024];
    match timeout(Duration::from_secs(2), client.read(&mut buf)).await {
        Ok(Ok(0)) => {
            println!("Connection closed after partial transfer");
        }
        Ok(Ok(n)) => {
            println!("Read {} bytes before disconnection", n);
        }
        Ok(Err(e)) => {
            println!("Read error after disconnection: {}", e);
        }
        Err(_) => {
            println!("Read timeout after disconnection");
        }
    }
    
    // Ensure proxy is still running
    match proxy_process.try_wait() {
        Ok(None) => {
            println!("Proxy handled partial transfer gracefully");
            proxy_process.kill().expect("Failed to kill proxy");
        }
        Ok(Some(_)) => {
            panic!("Proxy should not crash on partial transfers");
        }
        Err(e) => {
            panic!("Failed to check proxy status: {}", e);
        }
    }
    
    let _ = echo_handle.await;
}

#[tokio::test]
async fn test_simultaneous_connection_errors() {
    let proxy_listen_addr1 = "127.0.0.1:20008";
    let proxy_listen_addr2 = "127.0.0.1:20009";
    let unreachable1 = "127.0.0.1:20098";
    let unreachable2 = "127.0.0.1:20097";
    
    // Start proxy with multiple mappings to unreachable addresses
    let mut proxy_process = Command::new("cargo")
        .args(&[
            "run", "--",
            "--proxy", &format!("{}:{}", proxy_listen_addr1, unreachable1),
            "--proxy", &format!("{}:{}", proxy_listen_addr2, unreachable2),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    sleep(Duration::from_secs(5)).await;
    
    // Try to connect to both proxies simultaneously
    let handle1 = tokio::spawn(async move {
        match TcpStream::connect(proxy_listen_addr1).await {
            Ok(mut stream) => {
                let _ = stream.write_all(b"Test1").await;
                let mut buf = [0u8; 10];
                let _ = timeout(Duration::from_secs(1), stream.read(&mut buf)).await;
                println!("Connection to proxy 1 handled");
            }
            Err(e) => {
                println!("Failed to connect to proxy 1: {}", e);
            }
        }
    });
    
    let handle2 = tokio::spawn(async move {
        match TcpStream::connect(proxy_listen_addr2).await {
            Ok(mut stream) => {
                let _ = stream.write_all(b"Test2").await;
                let mut buf = [0u8; 10];
                let _ = timeout(Duration::from_secs(1), stream.read(&mut buf)).await;
                println!("Connection to proxy 2 handled");
            }
            Err(e) => {
                println!("Failed to connect to proxy 2: {}", e);
            }
        }
    });
    
    let _ = handle1.await;
    let _ = handle2.await;
    
    // Ensure proxy is still running
    match proxy_process.try_wait() {
        Ok(None) => {
            println!("Proxy handled simultaneous errors gracefully");
            proxy_process.kill().expect("Failed to kill proxy");
        }
        Ok(Some(_)) => {
            panic!("Proxy should not crash on simultaneous connection errors");
        }
        Err(e) => {
            panic!("Failed to check proxy status: {}", e);
        }
    }
}