use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

async fn start_echo_server(addr: &str) -> Result<tokio::task::JoinHandle<()>, std::io::Error> {
    let listener = TcpListener::bind(addr).await?;
    println!("Echo server listening on {}", addr);
    
    Ok(tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            
            tokio::spawn(async move {
                let mut buf = [0; 1024];
                
                loop {
                    match socket.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(e) = socket.write_all(&buf[0..n]).await {
                                eprintln!("Failed to write to socket: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to read from socket: {}", e);
                            break;
                        }
                    }
                }
            });
        }
    }))
}

#[tokio::test]
async fn test_basic_proxy_functionality() {
    let echo_server_addr = "127.0.0.1:19001";
    let proxy_listen_addr = "127.0.0.1:19002";
    
    let _echo_handle = start_echo_server(echo_server_addr).await.expect("Failed to start echo server");
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, echo_server_addr)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    let mut client = TcpStream::connect(proxy_listen_addr).await.unwrap();
    
    let test_message = b"Hello, proxy!";
    client.write_all(test_message).await.unwrap();
    
    let mut buffer = vec![0u8; test_message.len()];
    let n = timeout(Duration::from_secs(5), client.read_exact(&mut buffer))
        .await
        .expect("Timeout waiting for response")
        .expect("Failed to read response");
    
    assert_eq!(n, test_message.len());
    assert_eq!(&buffer[..], test_message);
    
    proxy_process.kill().expect("Failed to kill proxy process");
    let _ = proxy_process.wait();
}

#[tokio::test]
async fn test_multiple_concurrent_connections() {
    let echo_server_addr = "127.0.0.1:19003";
    let proxy_listen_addr = "127.0.0.1:19004";
    
    let _echo_handle = start_echo_server(echo_server_addr).await.expect("Failed to start echo server");
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, echo_server_addr)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    let mut handles = vec![];
    for i in 0..5 {
        let handle = tokio::spawn(async move {
            let mut client = TcpStream::connect(proxy_listen_addr).await.unwrap();
            
            let test_message = format!("Message {}", i);
            client.write_all(test_message.as_bytes()).await.unwrap();
            
            let mut buffer = vec![0u8; test_message.len()];
            let n = timeout(Duration::from_secs(5), client.read_exact(&mut buffer))
                .await
                .expect("Timeout waiting for response")
                .expect("Failed to read response");
            
            assert_eq!(n, test_message.len());
            assert_eq!(buffer, test_message.as_bytes());
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    proxy_process.kill().expect("Failed to kill proxy process");
    let _ = proxy_process.wait();
}

#[tokio::test]
async fn test_multiple_proxy_mappings() {
    let echo_server_addr1 = "127.0.0.1:19005";
    let echo_server_addr2 = "127.0.0.1:19006";
    let proxy_listen_addr1 = "127.0.0.1:19007";
    let proxy_listen_addr2 = "127.0.0.1:19008";
    
    let _echo_handle1 = start_echo_server(echo_server_addr1).await.expect("Failed to start echo server 1");
    let _echo_handle2 = start_echo_server(echo_server_addr2).await.expect("Failed to start echo server 2");
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    let mut proxy_process = Command::new("cargo")
        .args(&[
            "run", "--",
            "--proxy", &format!("{}:{}", proxy_listen_addr1, echo_server_addr1),
            "--proxy", &format!("{}:{}", proxy_listen_addr2, echo_server_addr2),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    let mut client1 = TcpStream::connect(proxy_listen_addr1).await.unwrap();
    let test_message1 = b"Hello from proxy 1!";
    client1.write_all(test_message1).await.unwrap();
    let mut buffer1 = vec![0u8; test_message1.len()];
    let n1 = timeout(Duration::from_secs(5), client1.read_exact(&mut buffer1))
        .await
        .expect("Timeout waiting for response from proxy 1")
        .expect("Failed to read response from proxy 1");
    assert_eq!(n1, test_message1.len());
    assert_eq!(&buffer1[..], test_message1);
    
    let mut client2 = TcpStream::connect(proxy_listen_addr2).await.unwrap();
    let test_message2 = b"Hello from proxy 2!";
    client2.write_all(test_message2).await.unwrap();
    let mut buffer2 = vec![0u8; test_message2.len()];
    let n2 = timeout(Duration::from_secs(5), client2.read_exact(&mut buffer2))
        .await
        .expect("Timeout waiting for response from proxy 2")
        .expect("Failed to read response from proxy 2");
    assert_eq!(n2, test_message2.len());
    assert_eq!(&buffer2[..], test_message2);
    
    proxy_process.kill().expect("Failed to kill proxy process");
    let _ = proxy_process.wait();
}

#[tokio::test]
async fn test_large_data_transfer() {
    let echo_server_addr = "127.0.0.1:19009";
    let proxy_listen_addr = "127.0.0.1:19010";
    
    let _echo_handle = start_echo_server(echo_server_addr).await.expect("Failed to start echo server");
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, echo_server_addr)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    let mut client = TcpStream::connect(proxy_listen_addr).await.unwrap();
    
    let large_data = vec![0x42u8; 10000];
    client.write_all(&large_data).await.unwrap();
    
    let mut received_data = vec![0u8; large_data.len()];
    let mut total_read = 0;
    
    while total_read < large_data.len() {
        let n = timeout(
            Duration::from_secs(5),
            client.read(&mut received_data[total_read..])
        )
        .await
        .expect("Timeout waiting for large data")
        .expect("Failed to read large data");
        
        if n == 0 {
            panic!("Connection closed before all data was received");
        }
        
        total_read += n;
    }
    
    assert_eq!(received_data, large_data);
    
    proxy_process.kill().expect("Failed to kill proxy process");
    let _ = proxy_process.wait();
}

#[tokio::test]
async fn test_connection_to_unreachable_upstream() {
    let unreachable_addr = "127.0.0.1:19999";
    let proxy_listen_addr = "127.0.0.1:19011";
    
    let mut proxy_process = Command::new("cargo")
        .args(&["run", "--", "--proxy", &format!("{}:{}", proxy_listen_addr, unreachable_addr)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start proxy");
    
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    match TcpStream::connect(proxy_listen_addr).await {
        Ok(mut stream) => {
            let test_message = b"Test message";
            match stream.write_all(test_message).await {
                Ok(_) => {
                    let mut buffer = vec![0u8; 1024];
                    match timeout(Duration::from_secs(2), stream.read(&mut buffer)).await {
                        Ok(Ok(0)) => {
                            println!("Connection closed as expected");
                        }
                        Ok(Ok(_)) => {
                            panic!("Should not receive data from unreachable upstream");
                        }
                        Ok(Err(_)) | Err(_) => {
                            println!("Read failed or timed out as expected");
                        }
                    }
                }
                Err(_) => {
                    println!("Write failed as expected");
                }
            }
        }
        Err(_) => {
            println!("Connection refused as expected");
        }
    }
    
    proxy_process.kill().expect("Failed to kill proxy process");
    let _ = proxy_process.wait();
}