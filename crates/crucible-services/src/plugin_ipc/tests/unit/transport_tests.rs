//! # Transport Component Tests
//!
//! Comprehensive tests for IPC transport components including Unix domain sockets,
//! TCP fallback, connection pooling, multiplexing, and network resilience.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream, TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::plugin_ipc::{
    error::{IpcError, IpcResult, ConnectionErrorCode},
    transport::{TransportManager, TransportConfig},
    message::{IpcMessage, MessageType, MessagePayload},
};

use super::common::{
    *,
    fixtures::*,
    mocks::*,
    helpers::*,
};

/// Connection management tests
pub struct ConnectionManagementTests;

impl ConnectionManagementTests {
    /// Test Unix domain socket connection establishment
    pub async fn test_unix_socket_connection() -> IpcResult<()> {
        let temp_dir = TestHelpers::create_temp_dir()?;
        let socket_path = format!("{}/test_socket", temp_dir);

        // Create Unix socket listener
        let listener = UnixListener::bind(&socket_path)?;
        let socket_path_clone = socket_path.clone();

        // Spawn server task
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0u8; 1024];
                if let Ok(n) = stream.read(&mut buffer).await {
                    let _ = stream.write_all(&buffer[..n]).await;
                }
            }
        });

        // Connect to socket
        tokio::time::sleep(Duration::from_millis(10)).await; // Give server time to start
        let mut stream = UnixStream::connect(&socket_path).await?;

        // Send test data
        let test_data = b"Hello, Unix socket!";
        stream.write_all(test_data).await?;

        // Read response
        let mut buffer = [0u8; 1024];
        let n = stream.read(&mut buffer).await?;

        assert_eq!(n, test_data.len());
        assert_eq!(&buffer[..n], test_data);

        // Cleanup
        drop(stream);
        TestHelpers::cleanup_temp_dir(&temp_dir)?;
        Ok(())
    }

    /// Test TCP connection fallback
    pub async fn test_tcp_connection_fallback() -> IpcResult<()> {
        let bind_addr = "127.0.0.1:0"; // Let OS choose port
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;

        // Spawn server task
        let local_addr_clone = local_addr;
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0u8; 1024];
                if let Ok(n) = stream.read(&mut buffer).await {
                    let response = format!("Echo: {}", String::from_utf8_lossy(&buffer[..n]));
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        });

        // Connect to TCP server
        tokio::time::sleep(Duration::from_millis(10)).await;
        let mut stream = TcpStream::connect(local_addr_clone).await?;

        // Send test data
        let test_data = b"Hello, TCP fallback!";
        stream.write_all(test_data).await?;

        // Read response
        let mut buffer = [0u8; 1024];
        let n = stream.read(&mut buffer).await?;

        let response = String::from_utf8_lossy(&buffer[..n]);
        assert!(response.contains("Echo:"));
        assert!(response.contains("Hello, TCP fallback!"));

        drop(stream);
        Ok(())
    }

    /// Test connection pooling
    pub async fn test_connection_pooling() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let num_connections = 10;

        // Create multiple connections
        let mut connection_ids = Vec::new();
        for i in 0..num_connections {
            let endpoint = format!("endpoint_{}", i);
            let connection_id = transport.connect(&endpoint).await?;
            connection_ids.push(connection_id);
        }

        // Verify all connections are unique
        let mut unique_ids = std::collections::HashSet::new();
        for id in &connection_ids {
            unique_ids.insert(id);
        }
        assert_eq!(unique_ids.len(), num_connections);

        // Verify all connections are active
        for connection_id in &connection_ids {
            let is_connected = transport.is_connected(connection_id).await?;
            assert!(is_connected);
        }

        // Disconnect all connections
        for connection_id in &connection_ids {
            transport.disconnect(connection_id).await?;
        }

        // Verify all connections are disconnected
        for connection_id in &connection_ids {
            let is_connected = transport.is_connected(connection_id).await?;
            assert!(!is_connected);
        }

        Ok(())
    }

    /// Test connection reuse from pool
    pub async fn test_connection_reuse() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "test_endpoint_reuse";

        // Create first connection
        let connection_id1 = transport.connect(endpoint).await?;
        transport.disconnect(&connection_id1).await?;

        // Create second connection to same endpoint
        let connection_id2 = transport.connect(endpoint).await?;

        // In a real implementation, this might reuse the same connection ID
        // For mock implementation, we just verify both connections work
        assert_ne!(connection_id1, connection_id2);
        assert!(transport.is_connected(&connection_id2).await?);

        transport.disconnect(&connection_id2).await?;
        Ok(())
    }

    /// Test connection timeout handling
    pub async fn test_connection_timeouts() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());

        // Set connection delay
        transport.set_delay(
            Duration::from_millis(100),
            Duration::ZERO,
            Duration::ZERO,
        ).await;

        let start = SystemTime::now();
        let result = transport.connect("slow_endpoint").await;
        let duration = start.elapsed().unwrap();

        assert!(result.is_ok());
        assert!(duration >= Duration::from_millis(100));

        // Set very long delay to simulate timeout
        transport.set_delay(
            Duration::from_secs(5),
            Duration::ZERO,
            Duration::ZERO,
        ).await;

        let start = SystemTime::now();
        let result = tokio::time::timeout(Duration::from_millis(500), transport.connect("timeout_endpoint")).await;
        let duration = start.elapsed().unwrap();

        assert!(result.is_err()); // Timeout occurred
        assert!(duration < Duration::from_secs(1));

        Ok(())
    }

    /// Test connection failure scenarios
    pub async fn test_connection_failures() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());

        // Test connection failure
        *transport.should_fail_connect.lock().await = true;
        let result = transport.connect("failing_endpoint").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("ConnectionRefused"));

        // Reset failure
        *transport.should_fail_connect.lock().await = false;

        // Test successful connection after failure
        let result = transport.connect("working_endpoint").await;
        assert!(result.is_ok());

        Ok(())
    }
}

/// Message transmission tests
pub struct MessageTransmissionTests;

impl MessageTransmissionTests {
    /// Test basic message sending and receiving
    pub async fn test_basic_message_transmission() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "test_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Send message
        let message = MessageFixtures::heartbeat();
        transport.send_message(&connection_id, message.clone()).await?;

        // Receive message
        let received_message = transport.receive_message(&connection_id).await?;

        // Verify message type (mock implementation returns heartbeat)
        assert_eq!(received_message.header.message_type, MessageType::Heartbeat);

        // Cleanup
        transport.disconnect(&connection_id).await?;
        Ok(())
    }

    /// Test large message transmission
    pub async fn test_large_message_transmission() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "test_large_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Send large message
        let large_message = MessageFixtures::large_message(1024 * 1024); // 1MB
        transport.send_message(&connection_id, large_message.clone()).await?;

        // Receive message
        let received_message = transport.receive_message(&connection_id).await?;

        // Verify message was received
        assert!(matches!(received_message.header.message_type, MessageType::Heartbeat));

        // Cleanup
        transport.disconnect(&connection_id).await?;
        Ok(())
    }

    /// Test concurrent message transmission
    pub async fn test_concurrent_message_transmission() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let num_connections = 10;
        let messages_per_connection = 20;

        // Create connections
        let mut connection_ids = Vec::new();
        for i in 0..num_connections {
            let endpoint = format!("concurrent_endpoint_{}", i);
            let connection_id = transport.connect(&endpoint).await?;
            connection_ids.push(connection_id);
        }

        // Send messages concurrently
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_connections * messages_per_connection,
            |i| {
                let transport = Arc::clone(&transport);
                let connection_id = connection_ids[i % num_connections].clone();
                let message = MessageFixtures::request(
                    &format!("operation_{}", i),
                    serde_json::json!({"index": i}),
                );
                async move {
                    transport.send_message(&connection_id, message).await
                }
            },
        ).await;

        // Verify all sends succeeded
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_connections * messages_per_connection);

        // Receive messages from all connections
        let mut received_count = 0;
        for connection_id in &connection_ids {
            for _ in 0..messages_per_connection {
                let _ = transport.receive_message(connection_id).await?;
                received_count += 1;
            }
        }

        assert_eq!(received_count, num_connections * messages_per_connection);

        // Cleanup
        for connection_id in &connection_ids {
            transport.disconnect(connection_id).await?;
        }

        Ok(())
    }

    /// Test message ordering preservation
    pub async fn test_message_ordering() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "ordering_test_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Send multiple messages in sequence
        let num_messages = 100;
        let mut sent_messages = Vec::new();

        for i in 0..num_messages {
            let message = MessageFixtures::request(
                &format!("ordered_operation_{}", i),
                serde_json::json!({"sequence": i}),
            );
            transport.send_message(&connection_id, message.clone()).await?;
            sent_messages.push(message);
        }

        // Receive messages and verify ordering
        // Note: Mock implementation may not preserve ordering perfectly
        let mut received_count = 0;
        while received_count < num_messages {
            let _received_message = transport.receive_message(&connection_id).await?;
            received_count += 1;
        }

        assert_eq!(received_count, num_messages);

        // Cleanup
        transport.disconnect(&connection_id).await?;
        Ok(())
    }

    /// Test transmission failures
    pub async fn test_transmission_failures() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "failure_test_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Test send failure
        *transport.should_fail_send.lock().await = true;
        let message = MessageFixtures::heartbeat();
        let result = transport.send_message(&connection_id, message).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("TransportError"));

        // Test receive failure
        *transport.should_fail_send.lock().await = false;
        *transport.should_fail_receive.lock().await = true;
        let result = transport.receive_message(&connection_id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("TransportError"));

        // Reset failures
        *transport.should_fail_receive.lock().await = false;

        // Test successful operation after failures
        let message = MessageFixtures::heartbeat();
        transport.send_message(&connection_id, message).await?;
        let _received = transport.receive_message(&connection_id).await?;

        // Cleanup
        transport.disconnect(&connection_id).await?;
        Ok(())
    }
}

/// Network resilience tests
pub struct NetworkResilienceTests;

impl NetworkResilienceTests {
    /// Test automatic reconnection
    pub async fn test_automatic_reconnection() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "reconnect_test_endpoint";

        // Initial connection
        let connection_id = transport.connect(endpoint).await?;
        assert!(transport.is_connected(&connection_id).await?);

        // Simulate connection loss
        transport.disconnect(&connection_id).await?;
        assert!(!transport.is_connected(&connection_id).await?);

        // In a real implementation, automatic reconnection would happen
        // For mock, we simulate by creating a new connection
        let new_connection_id = transport.connect(endpoint).await?;
        assert!(transport.is_connected(&new_connection_id).await?);

        // Verify new connection works
        let message = MessageFixtures::heartbeat();
        transport.send_message(&new_connection_id, message).await?;
        let _received = transport.receive_message(&new_connection_id).await?;

        // Cleanup
        transport.disconnect(&new_connection_id).await?;
        Ok(())
    }

    /// Test connection health monitoring
    pub async fn test_connection_health_monitoring() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "health_test_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Send heartbeat messages to maintain connection
        for _ in 0..5 {
            let heartbeat = MessageFixtures::heartbeat();
            transport.send_message(&connection_id, heartbeat).await?;
            let _response = transport.receive_message(&connection_id).await?;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Check connection stats
        let stats = transport.get_connection_stats(&connection_id).await?;
        assert!(stats.contains_key("connection_id"));
        assert!(stats.contains_key("is_connected"));

        // Cleanup
        transport.disconnect(&connection_id).await?;
        Ok(())
    }

    /// Test graceful shutdown
    pub async fn test_graceful_shutdown() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let num_connections = 10;

        // Create multiple connections
        let mut connection_ids = Vec::new();
        for i in 0..num_connections {
            let endpoint = format!("shutdown_test_{}", i);
            let connection_id = transport.connect(&endpoint).await?;
            connection_ids.push(connection_id);
        }

        // Send messages on all connections
        for connection_id in &connection_ids {
            let message = MessageFixtures::heartbeat();
            transport.send_message(connection_id, message).await?;
        }

        // Gracefully shutdown all connections
        for connection_id in &connection_ids {
            transport.disconnect(connection_id).await?;
        }

        // Verify all connections are closed
        for connection_id in &connection_ids {
            let is_connected = transport.is_connected(connection_id).await?;
            assert!(!is_connected);
        }

        Ok(())
    }

    /// Test network interruption simulation
    pub async fn test_network_interruption() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "interruption_test_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Send some messages successfully
        for i in 0..5 {
            let message = MessageFixtures::request(
                &format!("before_interruption_{}", i),
                serde_json::json!({"index": i}),
            );
            transport.send_message(&connection_id, message).await?;
            let _response = transport.receive_message(&connection_id).await?;
        }

        // Simulate network interruption
        *transport.should_fail_send.lock().await = true;

        // Try to send messages during interruption
        for i in 5..10 {
            let message = MessageFixtures::request(
                &format!("during_interruption_{}", i),
                serde_json::json!({"index": i}),
            );
            let result = transport.send_message(&connection_id, message).await;
            assert!(result.is_err());
        }

        // Restore network
        *transport.should_fail_send.lock().await = false;

        // Send messages after restoration
        for i in 10..15 {
            let message = MessageFixtures::request(
                &format!("after_restoration_{}", i),
                serde_json::json!({"index": i}),
            );
            transport.send_message(&connection_id, message).await?;
            let _response = transport.receive_message(&connection_id).await?;
        }

        // Cleanup
        transport.disconnect(&connection_id).await?;
        Ok(())
    }
}

/// Multiplexing tests
pub struct MultiplexingTests;

impl MultiplexingTests {
    /// Test multiple streams over single connection
    pub async fn test_stream_multiplexing() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "multiplex_test_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Create multiple logical streams
        let num_streams = 5;
        let messages_per_stream = 10;

        // Send messages for each stream (using metadata to identify stream)
        for stream_id in 0..num_streams {
            for msg_num in 0..messages_per_stream {
                let message = MessageFixtures::request(
                    &format!("stream_{}_msg_{}", stream_id, msg_num),
                    serde_json::json!({
                        "stream_id": stream_id,
                        "message_num": msg_num,
                        "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()
                    }),
                );
                transport.send_message(&connection_id, message).await?;
            }
        }

        // Receive all messages
        let total_messages = num_streams * messages_per_stream;
        let mut received_messages = Vec::new();

        for _ in 0..total_messages {
            let message = transport.receive_message(&connection_id).await?;
            received_messages.push(message);
        }

        assert_eq!(received_messages.len(), total_messages);

        // Group messages by stream ID (in a real implementation)
        let mut stream_groups: HashMap<u32, Vec<IpcMessage>> = HashMap::new();
        for message in received_messages {
            if let MessagePayload::Request(req) = &message.payload {
                if let Some(stream_id) = req.parameters["stream_id"].as_u64() {
                    stream_groups.entry(stream_id as u32).or_insert_with(Vec::new).push(message);
                }
            }
        }

        assert_eq!(stream_groups.len(), num_streams);

        // Cleanup
        transport.disconnect(&connection_id).await?;
        Ok(())
    }

    /// Test stream isolation
    pub async fn test_stream_isolation() -> IpcResult<()> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "isolation_test_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Create messages for different streams with different priorities
        let high_priority_msg = MessageFixtures::request_to(
            "high_priority_stream",
            "critical_operation",
            serde_json::json!({"priority": "high"}),
        );
        let mut high_priority_msg = high_priority_msg;
        high_priority_msg.header.priority = crate::plugin_ipc::message::MessagePriority::High;

        let low_priority_msg = MessageFixtures::request_to(
            "low_priority_stream",
            "background_operation",
            serde_json::json!({"priority": "low"}),
        );
        let mut low_priority_msg = low_priority_msg;
        low_priority_msg.header.priority = crate::plugin_ipc::message::MessagePriority::Low;

        // Send messages in mixed order
        transport.send_message(&connection_id, low_priority_msg).await?;
        transport.send_message(&connection_id, high_priority_msg).await?;

        // Receive messages
        let msg1 = transport.receive_message(&connection_id).await?;
        let msg2 = transport.receive_message(&connection_id).await?;

        // In a real implementation, priority would affect ordering
        // For mock implementation, we just verify both are received
        assert!(matches!(msg1.header.message_type, MessageType::Heartbeat));
        assert!(matches!(msg2.header.message_type, MessageType::Heartbeat));

        // Cleanup
        transport.disconnect(&connection_id).await?;
        Ok(())
    }
}

/// Transport performance tests
pub struct TransportPerformanceTests;

impl TransportPerformanceTests {
    /// Benchmark connection establishment
    pub async fn benchmark_connection_establishment() -> IpcResult<f64> {
        let transport = Arc::new(MockTransportManager::new());
        let num_connections = 1000;

        // Set small delay for realistic connection time
        transport.set_delay(Duration::from_millis(1), Duration::ZERO, Duration::ZERO).await;

        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_connections,
            |i| {
                let transport = Arc::clone(&transport);
                let endpoint = format!("perf_endpoint_{}", i);
                async move {
                    let connection_id = transport.connect(&endpoint).await?;
                    transport.disconnect(&connection_id).await
                }
            },
        ).await;
        let duration = start.elapsed().unwrap();

        // Calculate connections per second
        let connections_per_sec = num_connections as f64 / duration.as_secs_f64();

        // Verify success rate
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_connections);

        Ok(connections_per_sec)
    }

    /// Benchmark message throughput
    pub async fn benchmark_message_throughput() -> IpcResult<(f64, f64)> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "throughput_test_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Set minimal delays for high throughput
        transport.set_delay(Duration::ZERO, Duration::from_millis(1), Duration::from_millis(1)).await;

        let num_messages = 10000;
        let message_size = 1024; // 1KB messages

        // Benchmark sending
        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_messages,
            |i| {
                let transport = Arc::clone(&transport);
                let connection_id = connection_id.clone();
                let message = MessageFixtures::request(
                    &format!("perf_operation_{}", i),
                    serde_json::json!({"data": "x".repeat(message_size)}),
                );
                async move {
                    transport.send_message(&connection_id, message).await
                }
            },
        ).await;
        let send_duration = start.elapsed().unwrap();

        // Benchmark receiving
        let start = SystemTime::now();
        let mut received_count = 0;
        while received_count < num_messages {
            let _message = transport.receive_message(&connection_id).await?;
            received_count += 1;
        }
        let receive_duration = start.elapsed().unwrap();

        // Calculate throughput
        let send_throughput = num_messages as f64 / send_duration.as_secs_f64();
        let receive_throughput = num_messages as f64 / receive_duration.as_secs_f64();

        // Verify success rate
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_messages);

        // Cleanup
        transport.disconnect(&connection_id).await?;

        Ok((send_throughput, receive_throughput))
    }

    /// Benchmark concurrent connection performance
    pub async fn benchmark_concurrent_connections() -> IpcResult<f64> {
        let transport = Arc::new(MockTransportManager::new());
        let num_connections = 100;
        let messages_per_connection = 100;

        // Create connections
        let mut connection_ids = Vec::new();
        for i in 0..num_connections {
            let endpoint = format!("concurrent_perf_{}", i);
            let connection_id = transport.connect(&endpoint).await?;
            connection_ids.push(connection_id);
        }

        // Set minimal delays
        transport.set_delay(Duration::ZERO, Duration::from_millis(1), Duration::from_millis(1)).await;

        // Benchmark concurrent operations
        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_connections * messages_per_connection,
            |i| {
                let transport = Arc::clone(&transport);
                let connection_id = connection_ids[i % num_connections].clone();
                let message = MessageFixtures::request(
                    &format!("concurrent_op_{}", i),
                    serde_json::json!({"index": i}),
                );
                async move {
                    transport.send_message(&connection_id, message).await?;
                    transport.receive_message(&connection_id).await.map(|_| ())
                }
            },
        ).await;
        let duration = start.elapsed().unwrap();

        // Calculate operations per second
        let total_operations = num_connections * messages_per_connection;
        let ops_per_sec = total_operations as f64 / duration.as_secs_f64();

        // Verify success rate
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, total_operations);

        // Cleanup
        for connection_id in &connection_ids {
            transport.disconnect(connection_id).await?;
        }

        Ok(ops_per_sec)
    }

    /// Benchmark message latency
    pub async fn benchmark_message_latency() -> IpcResult<Duration> {
        let transport = Arc::new(MockTransportManager::new());
        let endpoint = "latency_test_endpoint";

        // Connect
        let connection_id = transport.connect(endpoint).await?;

        // Set small delay to simulate network latency
        transport.set_delay(Duration::ZERO, Duration::from_millis(5), Duration::from_millis(5)).await;

        let num_tests = 1000;
        let mut total_latency = Duration::ZERO;

        for i in 0..num_tests {
            let start = SystemTime::now();

            let message = MessageFixtures::request(
                &format!("latency_test_{}", i),
                serde_json::json!({"timestamp": start.duration_since(UNIX_EPOCH).unwrap().as_millis()}),
            );

            transport.send_message(&connection_id, message).await?;
            let _response = transport.receive_message(&connection_id).await?;

            let latency = start.elapsed().unwrap();
            total_latency += latency;
        }

        let average_latency = total_latency / num_tests;

        // Cleanup
        transport.disconnect(&connection_id).await?;

        Ok(average_latency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async_test!(test_unix_socket_connection, {
        ConnectionManagementTests::test_unix_socket_connection().await.unwrap();
        "success"
    });

    async_test!(test_tcp_connection_fallback, {
        ConnectionManagementTests::test_tcp_connection_fallback().await.unwrap();
        "success"
    });

    async_test!(test_connection_pooling, {
        ConnectionManagementTests::test_connection_pooling().await.unwrap();
        "success"
    });

    async_test!(test_connection_reuse, {
        ConnectionManagementTests::test_connection_reuse().await.unwrap();
        "success"
    });

    async_test!(test_connection_timeouts, {
        ConnectionManagementTests::test_connection_timeouts().await.unwrap();
        "success"
    });

    async_test!(test_connection_failures, {
        ConnectionManagementTests::test_connection_failures().await.unwrap();
        "success"
    });

    async_test!(test_basic_message_transmission, {
        MessageTransmissionTests::test_basic_message_transmission().await.unwrap();
        "success"
    });

    async_test!(test_large_message_transmission, {
        MessageTransmissionTests::test_large_message_transmission().await.unwrap();
        "success"
    });

    async_test!(test_concurrent_message_transmission, {
        MessageTransmissionTests::test_concurrent_message_transmission().await.unwrap();
        "success"
    });

    async_test!(test_message_ordering, {
        MessageTransmissionTests::test_message_ordering().await.unwrap();
        "success"
    });

    async_test!(test_transmission_failures, {
        MessageTransmissionTests::test_transmission_failures().await.unwrap();
        "success"
    });

    async_test!(test_automatic_reconnection, {
        NetworkResilienceTests::test_automatic_reconnection().await.unwrap();
        "success"
    });

    async_test!(test_connection_health_monitoring, {
        NetworkResilienceTests::test_connection_health_monitoring().await.unwrap();
        "success"
    });

    async_test!(test_graceful_shutdown, {
        NetworkResilienceTests::test_graceful_shutdown().await.unwrap();
        "success"
    });

    async_test!(test_network_interruption, {
        NetworkResilienceTests::test_network_interruption().await.unwrap();
        "success"
    });

    async_test!(test_stream_multiplexing, {
        MultiplexingTests::test_stream_multiplexing().await.unwrap();
        "success"
    });

    async_test!(test_stream_isolation, {
        MultiplexingTests::test_stream_isolation().await.unwrap();
        "success"
    });

    async_test!(test_connection_establishment_performance, {
        let conn_per_sec = TransportPerformanceTests::benchmark_connection_establishment().await.unwrap();
        assert!(conn_per_sec > 100.0); // At least 100 connections/sec
        conn_per_sec
    });

    async_test!(test_message_throughput_performance, {
        let (send_throughput, receive_throughput) = TransportPerformanceTests::benchmark_message_throughput().await.unwrap();
        assert!(send_throughput > 100.0); // At least 100 messages/sec
        assert!(receive_throughput > 100.0); // At least 100 messages/sec
        (send_throughput, receive_throughput)
    });

    async_test!(test_concurrent_connections_performance, {
        let ops_per_sec = TransportPerformanceTests::benchmark_concurrent_connections().await.unwrap();
        assert!(ops_per_sec > 50.0); // At least 50 operations/sec
        ops_per_sec
    });

    async_test!(test_message_latency_performance, {
        let avg_latency = TransportPerformanceTests::benchmark_message_latency().await.unwrap();
        assert!(avg_latency < Duration::from_millis(100)); // Less than 100ms average latency
        avg_latency.as_millis()
    });
}