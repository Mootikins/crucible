//! # Test Helpers
//!
//! Utility functions and helpers for testing IPC components.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use uuid::Uuid;

use crate::plugin_ipc::{
    error::{IpcError, IpcResult},
    message::{IpcMessage, MessageType, MessagePayload, MessageHeader},
    protocol::ProtocolHandler,
    security::SecurityManager,
    transport::TransportManager,
    metrics::MetricsCollector,
};

use super::{mocks::*, fixtures::*, performance::PerformanceMetrics};

/// Helper utilities for testing
pub struct TestHelpers;

impl TestHelpers {
    /// Create a test message with random ID
    pub fn random_message(message_type: MessageType, payload: MessagePayload) -> IpcMessage {
        let mut header = MessageHeader::default();
        header.message_id = Uuid::new_v4().to_string();
        header.session_id = Uuid::new_v4().to_string();
        header.message_type = message_type;
        IpcMessage { header, payload }
    }

    /// Compare two messages ignoring timestamp and message ID
    pub fn messages_equal_ignoring_metadata(msg1: &IpcMessage, msg2: &IpcMessage) -> bool {
        msg1.header.message_type == msg2.header.message_type
            && msg1.header.priority == msg2.header.priority
            && msg1.payload == msg2.payload
    }

    /// Wait for a condition to be true with timeout
    pub async fn wait_for_condition<F, Fut>(
        condition: F,
        timeout_duration: Duration,
        poll_interval: Duration,
    ) -> Result<(), &'static str>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let start = SystemTime::now();

        while start.elapsed().unwrap() < timeout_duration {
            if condition().await {
                return Ok(());
            }
            tokio::time::sleep(poll_interval).await;
        }

        Err("Condition not met within timeout")
    }

    /// Measure execution time of an async operation
    pub async fn measure_time<F, T, E>(operation: F) -> Result<(T, Duration), E>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        let start = SystemTime::now();
        let result = operation.await;
        let duration = start.elapsed().unwrap();
        result.map(|r| (r, duration))
    }

    /// Generate test data of specified size
    pub fn generate_test_data(size_bytes: usize) -> Vec<u8> {
        let mut data = Vec::with_capacity(size_bytes);
        for i in 0..size_bytes {
            data.push((i % 256) as u8);
        }
        data
    }

    /// Generate test string of specified length
    pub fn generate_test_string(length: usize) -> String {
        let base = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut result = String::with_capacity(length);
        for i in 0..length {
            result.push(base.chars().nth(i % base.len()).unwrap());
        }
        result
    }

    /// Create a temporary directory for testing
    pub fn create_temp_dir() -> Result<String, std::io::Error> {
        let temp_path = format!("/tmp/crucible_test_{}", Uuid::new_v4());
        std::fs::create_dir_all(&temp_path)?;
        Ok(temp_path)
    }

    /// Clean up temporary directory
    pub fn cleanup_temp_dir(path: &str) -> Result<(), std::io::Error> {
        std::fs::remove_dir_all(path)
    }

    /// Verify error matches expected type and message pattern
    pub fn verify_error(error: &IpcError, expected_message_pattern: &str) -> bool {
        let error_message = error.to_string();
        error_message.contains(expected_message_pattern)
    }

    /// Count messages by type in a collection
    pub fn count_messages_by_type(messages: &[IpcMessage], message_type: MessageType) -> usize {
        messages.iter()
            .filter(|msg| msg.header.message_type == message_type)
            .count()
    }

    /// Find message by correlation ID
    pub fn find_message_by_correlation_id(
        messages: &[IpcMessage],
        correlation_id: &str,
    ) -> Option<&IpcMessage> {
        messages.iter()
            .find(|msg| msg.header.correlation_id.as_ref() == Some(&correlation_id.to_string()))
    }

    /// Create a simple in-memory transport for testing
    pub fn create_memory_transport() -> MemoryTransport {
        MemoryTransport::new()
    }

    /// Generate a sequence of messages for testing
    pub fn generate_message_sequence(count: usize) -> Vec<IpcMessage> {
        let mut messages = Vec::with_capacity(count);
        for i in 0..count {
            let message = MessageFixtures::request(
                &format!("operation_{}", i),
                serde_json::json!({"index": i, "data": format!("test_data_{}", i)}),
            );
            messages.push(message);
        }
        messages
    }

    /// Simulate network latency
    pub async fn simulate_latency(latency_ms: u64) {
        tokio::time::sleep(Duration::from_millis(latency_ms)).await;
    }

    /// Create a mock failure scenario
    pub fn create_failure_scenario(
        failure_rate: f64,
        failure_type: FailureType,
    ) -> FailureScenario {
        FailureScenario::new(failure_rate, failure_type)
    }
}

/// In-memory transport for testing
#[derive(Debug)]
pub struct MemoryTransport {
    messages: Arc<RwLock<Vec<IpcMessage>>>,
    connected: Arc<Mutex<bool>>,
}

impl MemoryTransport {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(RwLock::new(Vec::new())),
            connected: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn connect(&self) -> IpcResult<()> {
        *self.connected.lock().await = true;
        Ok(())
    }

    pub async fn disconnect(&self) -> IpcResult<()> {
        *self.connected.lock().await = false;
        Ok(())
    }

    pub async fn send(&self, message: IpcMessage) -> IpcResult<()> {
        if !*self.connected.lock().await {
            return Err(IpcError::Connection {
                message: "Not connected".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionClosed,
                endpoint: "memory_transport".to_string(),
                retry_count: 0,
            });
        }
        self.messages.write().await.push(message);
        Ok(())
    }

    pub async fn receive(&self) -> IpcResult<Option<IpcMessage>> {
        if !*self.connected.lock().await {
            return Err(IpcError::Connection {
                message: "Not connected".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionClosed,
                endpoint: "memory_transport".to_string(),
                retry_count: 0,
            });
        }
        let mut messages = self.messages.write().await;
        Ok(messages.pop())
    }

    pub async fn clear(&self) {
        self.messages.write().await.clear();
    }

    pub async fn message_count(&self) -> usize {
        self.messages.read().await.len()
    }
}

/// Failure simulation for testing
#[derive(Debug, Clone)]
pub enum FailureType {
    Connection,
    Send,
    Receive,
    Authentication,
    Encryption,
    Timeout,
    Protocol,
}

#[derive(Debug)]
pub struct FailureScenario {
    failure_rate: f64,
    failure_type: FailureType,
    attempt_count: Arc<Mutex<u32>>,
}

impl FailureScenario {
    pub fn new(failure_rate: f64, failure_type: FailureType) -> Self {
        Self {
            failure_rate,
            failure_type,
            attempt_count: Arc::new(Mutex::new(0)),
        }
    }

    pub async fn should_fail(&self) -> bool {
        let mut count = self.attempt_count.lock().await;
        *count += 1;

        // Use deterministic pattern based on attempt count and failure rate
        (*count as f64 * self.failure_rate).floor() as u32 > ((*count - 1) as f64 * self.failure_rate).floor() as u32
    }

    pub async fn create_error(&self) -> IpcError {
        match self.failure_type {
            FailureType::Connection => IpcError::Connection {
                message: "Simulated connection failure".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::ConnectionRefused,
                endpoint: "test_endpoint".to_string(),
                retry_count: *self.attempt_count.lock().await,
            },
            FailureType::Send => IpcError::Connection {
                message: "Simulated send failure".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                endpoint: "test_endpoint".to_string(),
                retry_count: *self.attempt_count.lock().await,
            },
            FailureType::Receive => IpcError::Connection {
                message: "Simulated receive failure".to_string(),
                code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                endpoint: "test_endpoint".to_string(),
                retry_count: *self.attempt_count.lock().await,
            },
            FailureType::Authentication => IpcError::Authentication {
                message: "Simulated authentication failure".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: Some(Duration::from_secs(1)),
            },
            FailureType::Encryption => IpcError::Protocol {
                message: "Simulated encryption failure".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::EncryptionFailed,
                source: None,
            },
            FailureType::Timeout => IpcError::Protocol {
                message: "Simulated timeout".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::Timeout,
                source: None,
            },
            FailureType::Protocol => IpcError::Protocol {
                message: "Simulated protocol error".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::ProtocolViolation,
                source: None,
            },
        }
    }
}

/// Test utilities for concurrent operations
pub struct ConcurrencyTestUtils;

impl ConcurrencyTestUtils {
    /// Run multiple operations concurrently and collect results
    pub async fn run_concurrent_operations<F, Fut, T>(
        num_operations: usize,
        operation_factory: F,
    ) -> Vec<Result<T, IpcError>>
    where
        F: Fn(usize) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<T, IpcError>> + Send + 'static,
        T: Send + 'static,
    {
        let mut handles = Vec::new();

        for i in 0..num_operations {
            let handle = tokio::spawn(operation_factory(i));
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    // Convert task panic to IpcError
                    results.push(Err(IpcError::Protocol {
                        message: format!("Task panicked: {}", e),
                        code: crate::plugin_ipc::error::ProtocolErrorCode::InternalError,
                        source: None,
                    }));
                }
            }
        }

        results
    }

    /// Test race conditions in message processing
    pub async fn test_message_race_condition(
        transport: Arc<MemoryTransport>,
        num_senders: usize,
        messages_per_sender: usize,
    ) -> (usize, usize, Duration) {
        let start = SystemTime::now();

        // Connect transport
        transport.connect().await.unwrap();

        // Create senders
        let mut handles = Vec::new();
        for sender_id in 0..num_senders {
            let transport_clone = Arc::clone(&transport);
            let handle = tokio::spawn(async move {
                for msg_id in 0..messages_per_sender {
                    let message = MessageFixtures::request(
                        &format!("sender_{}_msg_{}", sender_id, msg_id),
                        serde_json::json!({"sender": sender_id, "msg": msg_id}),
                    );
                    let _ = transport_clone.send(message).await;
                }
            });
            handles.push(handle);
        }

        // Wait for all senders to complete
        for handle in handles {
            let _ = handle.await;
        }

        let sent_count = num_senders * messages_per_sender;
        let received_count = transport.message_count().await;
        let duration = start.elapsed().unwrap();

        transport.disconnect().await.unwrap();

        (sent_count, received_count, duration)
    }
}

/// Performance testing utilities
pub struct PerformanceTestUtils;

impl PerformanceTestUtils {
    /// Benchmark message serialization/deserialization
    pub async fn benchmark_message_serde(
        message: &IpcMessage,
        iterations: usize,
    ) -> (Duration, Duration, f64) {
        // Benchmark serialization
        let serialize_start = SystemTime::now();
        for _ in 0..iterations {
            let _ = serde_json::to_vec(message).unwrap();
        }
        let serialize_duration = serialize_start.elapsed().unwrap();

        // Benchmark deserialization
        let serialized = serde_json::to_vec(message).unwrap();
        let deserialize_start = SystemTime::now();
        for _ in 0..iterations {
            let _: IpcMessage = serde_json::from_slice(&serialized).unwrap();
        }
        let deserialize_duration = deserialize_start.elapsed().unwrap();

        // Calculate throughput (messages per second)
        let total_time = serialize_duration + deserialize_duration;
        let throughput = iterations as f64 / total_time.as_secs_f64();

        (serialize_duration, deserialize_duration, throughput)
    }

    /// Benchmark transport operations
    pub async fn benchmark_transport(
        transport: Arc<MemoryTransport>,
        message_count: usize,
    ) -> (Duration, f64, f64) {
        let messages = TestHelpers::generate_message_sequence(message_count);

        let start = SystemTime::now();
        transport.connect().await.unwrap();

        // Benchmark sending
        for message in messages {
            let _ = transport.send(message).await;
        }

        let send_duration = start.elapsed().unwrap();
        let send_throughput = message_count as f64 / send_duration.as_secs_f64();

        // Benchmark receiving
        let receive_start = SystemTime::now();
        let mut received_count = 0;
        while let Some(_) = transport.receive().await.unwrap() {
            received_count += 1;
        }

        let receive_duration = receive_start.elapsed().unwrap();
        let receive_throughput = received_count as f64 / receive_duration.as_secs_f64();

        transport.disconnect().await.unwrap();

        (send_duration, send_throughput, receive_throughput)
    }

    /// Measure memory usage of operations
    pub fn measure_memory_usage<F, R>(operation: F) -> (R, usize)
    where
        F: FnOnce() -> R,
    {
        // This is a simplified memory measurement
        // In a real implementation, you would use proper memory profiling tools
        let memory_before = Self::estimate_memory_usage();
        let result = operation();
        let memory_after = Self::estimate_memory_usage();
        let memory_used = memory_after.saturating_sub(memory_before);

        (result, memory_used)
    }

    fn estimate_memory_usage() -> usize {
        // This is a placeholder for actual memory measurement
        // In a real implementation, you would use system calls or profiling libraries
        0
    }
}

/// Property-based testing utilities
pub struct PropertyTestUtils;

impl PropertyTestUtils {
    /// Generate random message for property testing
    pub fn random_message() -> IpcMessage {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let message_types = vec![
            MessageType::Request,
            MessageType::Response,
            MessageType::Event,
            MessageType::Heartbeat,
            MessageType::StreamChunk,
            MessageType::StreamEnd,
            MessageType::ConfigurationUpdate,
        ];

        let message_type = message_types[rng.gen_range(0..message_types.len())];
        let payload = Self::random_payload_for_type(&message_type);

        TestHelpers::random_message(message_type, payload)
    }

    fn random_payload_for_type(message_type: &MessageType) -> MessagePayload {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        match message_type {
            MessageType::Request => MessagePayload::Request(crate::plugin_ipc::message::RequestPayload {
                operation: format!("operation_{}", rng.gen_range(0..100)),
                parameters: serde_json::json!({"random": rng.gen_range(0..1000)}),
                timeout_ms: Some(rng.gen_range(1000..60000)),
                retry_policy: None,
                metadata: HashMap::new(),
                context: HashMap::new(),
            }),
            MessageType::Response => MessagePayload::Response(crate::plugin_ipc::message::ResponsePayload {
                correlation_id: Uuid::new_v4().to_string(),
                success: rng.gen(),
                data: Some(serde_json::json!({"result": rng.gen_range(0..100)})),
                error: None,
                metadata: HashMap::new(),
                execution_time_ms: Some(rng.gen_range(10..1000)),
            }),
            MessageType::Event => MessagePayload::Event(crate::plugin_ipc::message::EventPayload {
                event_type: format!("event_{}", rng.gen_range(0..50)),
                data: serde_json::json!({"value": rng.gen_range(0..1000)}),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                source: "test".to_string(),
                metadata: HashMap::new(),
            }),
            MessageType::Heartbeat => MessageFixtures::heartbeat().payload,
            MessageType::StreamChunk => MessagePayload::StreamChunk(crate::plugin_ipc::message::StreamChunk {
                stream_id: Uuid::new_v4().to_string(),
                chunk_number: rng.gen_range(0..100),
                data: vec![rng.gen_range(0..255); rng.gen_range(10..1000)],
                is_final: rng.gen(),
                metadata: HashMap::new(),
            }),
            MessageType::StreamEnd => MessagePayload::StreamEnd(crate::plugin_ipc::message::StreamEnd {
                stream_id: Uuid::new_v4().to_string(),
                success: rng.gen(),
                error: None,
                metadata: HashMap::new(),
            }),
            MessageType::ConfigurationUpdate => MessagePayload::ConfigurationUpdate(
                crate::plugin_ipc::message::ConfigurationUpdate {
                    config: serde_json::json!({"random": rng.gen_range(0..1000)}),
                    version: rng.gen_range(1..10),
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                    source: "test".to_string(),
                    metadata: HashMap::new(),
                }
            ),
            _ => MessagePayload::Unknown(serde_json::Value::Null),
        }
    }

    /// Test property: serialization round-trip preserves message
    pub fn test_serialization_roundtrip(message: &IpcMessage) -> bool {
        let serialized = serde_json::to_vec(message);
        if serialized.is_err() {
            return false;
        }

        let deserialized: Result<IpcMessage, _> = serde_json::from_slice(&serialized.unwrap());
        if deserialized.is_err() {
            return false;
        }

        TestHelpers::messages_equal_ignoring_metadata(message, &deserialized.unwrap())
    }
}