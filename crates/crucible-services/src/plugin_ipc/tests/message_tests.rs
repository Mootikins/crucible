//! # Message Type Tests
//!
//! Comprehensive tests for IPC message types including creation, validation,
//! serialization, routing, and handling of different message categories.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::plugin_ipc::{
    error::{IpcError, IpcResult, MessageErrorCode},
    message::{
        IpcMessage, MessageHeader, MessagePayload, MessageType, MessageFlags, MessagePriority,
        RequestPayload, ResponsePayload, EventPayload, HeartbeatPayload, ResourceUsage,
        HeartbeatStatus, ClientCapabilities, PluginCapabilities, StreamChunk, StreamEnd,
        ConfigurationUpdate, SecurityConfig, TransportConfig, MetricsConfig,
    },
};

use super::common::{
    *,
    fixtures::*,
    helpers::*,
};

/// Message creation tests
pub struct MessageCreationTests;

impl MessageCreationTests {
    /// Test basic message creation
    pub fn test_basic_message_creation() -> IpcResult<()> {
        let message = IpcMessage::new(
            MessageType::Heartbeat,
            MessagePayload::Heartbeat(HeartbeatPayload {
                status: HeartbeatStatus::Healthy,
                last_activity: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                resource_usage: ResourceUsage {
                    memory_bytes: 1024 * 1024,
                    cpu_percentage: 25.5,
                    disk_bytes: 2048 * 1024,
                    network_bytes: 512 * 1024,
                    open_files: 10,
                    active_threads: 2,
                },
                metrics: HashMap::new(),
                status_data: HashMap::new(),
            }),
        );

        assert_eq!(message.header.message_type, MessageType::Heartbeat);
        assert!(matches!(message.payload, MessagePayload::Heartbeat(_)));
        assert!(!message.header.message_id.is_empty());
        assert!(!message.header.session_id.is_empty());
        assert!(message.header.timestamp > 0);

        Ok(())
    }

    /// Test request message creation
    pub fn test_request_message_creation() -> IpcResult<()> {
        let destination = "test_plugin".to_string();
        let payload = RequestPayload {
            operation: "test_operation".to_string(),
            parameters: json!({"param1": "value1", "param2": 42}),
            timeout_ms: Some(30000),
            retry_policy: None,
            metadata: HashMap::new(),
            context: HashMap::new(),
        };

        let message = IpcMessage::request(destination.clone(), payload.clone());

        assert_eq!(message.header.message_type, MessageType::Request);
        assert_eq!(message.header.destination, Some(destination));
        assert!(message.header.correlation_id.is_some());
        assert!(matches!(message.payload, MessagePayload::Request(_)));

        if let MessagePayload::Request(req) = &message.payload {
            assert_eq!(req.operation, payload.operation);
            assert_eq!(req.parameters, payload.parameters);
        }

        Ok(())
    }

    /// Test response message creation
    pub fn test_response_message_creation() -> IpcResult<()> {
        let correlation_id = Uuid::new_v4().to_string();
        let payload = ResponsePayload {
            correlation_id: correlation_id.clone(),
            success: true,
            data: Some(json!({"result": "success"})),
            error: None,
            metadata: HashMap::new(),
            execution_time_ms: Some(150),
        };

        let message = IpcMessage::response(correlation_id.clone(), payload.clone());

        assert_eq!(message.header.message_type, MessageType::Response);
        assert_eq!(message.header.correlation_id, Some(correlation_id));
        assert!(matches!(message.payload, MessagePayload::Response(_)));

        if let MessagePayload::Response(resp) = &message.payload {
            assert_eq!(resp.correlation_id, payload.correlation_id);
            assert_eq!(resp.success, payload.success);
        }

        Ok(())
    }

    /// Test event message creation
    pub fn test_event_message_creation() -> IpcResult<()> {
        let event_type = "test_event".to_string();
        let data = json!({"event_data": "test_value"});

        let message = IpcMessage::new(
            MessageType::Event,
            MessagePayload::Event(EventPayload {
                event_type: event_type.clone(),
                data: data.clone(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                source: "test_source".to_string(),
                metadata: HashMap::new(),
            }),
        );

        assert_eq!(message.header.message_type, MessageType::Event);
        assert!(matches!(message.payload, MessagePayload::Event(_)));

        if let MessagePayload::Event(event) = &message.payload {
            assert_eq!(event.event_type, event_type);
            assert_eq!(event.data, data);
        }

        Ok(())
    }

    /// Test stream message creation
    pub fn test_stream_message_creation() -> IpcResult<()> {
        let stream_id = Uuid::new_v4().to_string();
        let chunk_number = 1;
        let data = b"test stream data".to_vec();

        // Test stream chunk
        let chunk_message = IpcMessage::new(
            MessageType::StreamChunk,
            MessagePayload::StreamChunk(StreamChunk {
                stream_id: stream_id.clone(),
                chunk_number,
                data: data.clone(),
                is_final: false,
                metadata: HashMap::new(),
            }),
        );

        assert_eq!(chunk_message.header.message_type, MessageType::StreamChunk);
        assert!(matches!(chunk_message.payload, MessagePayload::StreamChunk(_)));

        // Test stream end
        let end_message = IpcMessage::new(
            MessageType::StreamEnd,
            MessagePayload::StreamEnd(StreamEnd {
                stream_id: stream_id.clone(),
                success: true,
                error: None,
                metadata: HashMap::new(),
            }),
        );

        assert_eq!(end_message.header.message_type, MessageType::StreamEnd);
        assert!(matches!(end_message.payload, MessagePayload::StreamEnd(_)));

        Ok(())
    }

    /// Test configuration update message creation
    pub fn test_config_update_message_creation() -> IpcResult<()> {
        let config = json!({
            "timeout": 30000,
            "retries": 3,
            "enable_logging": true
        });

        let message = IpcMessage::new(
            MessageType::ConfigurationUpdate,
            MessagePayload::ConfigurationUpdate(ConfigurationUpdate {
                config: config.clone(),
                version: 1,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                source: "test_config".to_string(),
                metadata: HashMap::new(),
            }),
        );

        assert_eq!(message.header.message_type, MessageType::ConfigurationUpdate);
        assert!(matches!(message.payload, MessagePayload::ConfigurationUpdate(_)));

        if let MessagePayload::ConfigurationUpdate(update) = &message.payload {
            assert_eq!(update.config, config);
            assert_eq!(update.version, 1);
        }

        Ok(())
    }
}

/// Message validation tests
pub struct MessageValidationTests;

impl MessageValidationTests {
    /// Test message validation for required fields
    pub fn test_message_validation() -> IpcResult<()> {
        let mut message = MessageFixtures::heartbeat();

        // Test valid message
        assert!(message.validate().is_ok());

        // Test invalid message ID
        message.header.message_id = "".to_string();
        assert!(message.validate().is_err());

        // Test invalid session ID
        message.header.message_id = Uuid::new_v4().to_string();
        message.header.session_id = "".to_string();
        assert!(message.validate().is_err());

        // Test invalid timestamp (future)
        message.header.session_id = Uuid::new_v4().to_string();
        message.header.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64 + 3600_000_000_000; // 1 hour in future
        assert!(message.validate().is_err());

        Ok(())
    }

    /// Test request message validation
    pub fn test_request_message_validation() -> IpcResult<()> {
        let mut payload = RequestPayload {
            operation: "test_operation".to_string(),
            parameters: json!({}),
            timeout_ms: Some(30000),
            retry_policy: None,
            metadata: HashMap::new(),
            context: HashMap::new(),
        };

        // Test valid request
        let message = IpcMessage::new(MessageType::Request, MessagePayload::Request(payload.clone()));
        assert!(message.validate().is_ok());

        // Test empty operation
        payload.operation = "".to_string();
        let message = IpcMessage::new(MessageType::Request, MessagePayload::Request(payload));
        assert!(message.validate().is_err());

        Ok(())
    }

    /// Test response message validation
    pub fn test_response_message_validation() -> IpcResult<()> {
        let correlation_id = Uuid::new_v4().to_string();

        // Test successful response
        let payload = ResponsePayload {
            correlation_id: correlation_id.clone(),
            success: true,
            data: Some(json!({"result": "success"})),
            error: None,
            metadata: HashMap::new(),
            execution_time_ms: Some(150),
        };

        let message = IpcMessage::response(correlation_id.clone(), payload);
        assert!(message.validate().is_ok());

        // Test failed response with error
        let error_payload = ResponsePayload {
            correlation_id: correlation_id.clone(),
            success: false,
            data: None,
            error: Some(json!({
                "code": "TEST_ERROR",
                "message": "Test error message"
            })),
            metadata: HashMap::new(),
            execution_time_ms: Some(50),
        };

        let message = IpcMessage::response(correlation_id, error_payload);
        assert!(message.validate().is_ok());

        // Test response without correlation ID
        let invalid_payload = ResponsePayload {
            correlation_id: "".to_string(),
            success: true,
            data: Some(json!({})),
            error: None,
            metadata: HashMap::new(),
            execution_time_ms: None,
        };

        let message = IpcMessage::response("".to_string(), invalid_payload);
        assert!(message.validate().is_err());

        Ok(())
    }

    /// Test message size validation
    pub fn test_message_size_validation() -> IpcResult<()> {
        // Test small message
        let small_message = MessageFixtures::request("test", json!({"data": "small"}));
        assert!(small_message.validate().is_ok());

        // Test large message within limits
        let large_message = MessageFixtures::large_message(10 * 1024 * 1024); // 10MB
        assert!(large_message.validate().is_ok());

        // Test oversized message
        let oversized_message = MessageFixtures::large_message(20 * 1024 * 1024); // 20MB
        assert!(oversized_message.validate().is_err());

        Ok(())
    }

    /// Test message priority validation
    pub fn test_message_priority_validation() -> IpcResult<()> {
        let mut message = MessageFixtures::heartbeat();

        // Test all priority levels
        for priority in [
            MessagePriority::Low,
            MessagePriority::Normal,
            MessagePriority::High,
            MessagePriority::Critical,
        ] {
            message.header.priority = priority;
            assert!(message.validate().is_ok());
        }

        Ok(())
    }
}

/// Message serialization tests
pub struct MessageSerializationTests;

impl MessageSerializationTests {
    /// Test message serialization and deserialization
    pub fn test_message_serialization_roundtrip() -> IpcResult<()> {
        let original_message = MessageFixtures::full_message();

        // Serialize message
        let serialized = serde_json::to_vec(&original_message)?;
        assert!(!serialized.is_empty());

        // Deserialize message
        let deserialized: IpcMessage = serde_json::from_slice(&serialized)?;
        assert_eq!(deserialized.header.message_type, original_message.header.message_type);
        assert_eq!(deserialized.payload, original_message.payload);

        Ok(())
    }

    /// Test serialization of different message types
    pub fn test_all_message_types_serialization() -> IpcResult<()> {
        let messages = vec![
            MessageFixtures::heartbeat(),
            MessageFixtures::request("test", json!({"data": "test"})),
            MessageFixtures::success_response("corr_id", json!({"result": "success"})),
            MessageFixtures::error_response("corr_id", "TEST_ERROR", "Test error"),
            MessageFixtures::event("test_event", json!({"event_data": "test"})),
            MessageFixtures::stream_chunk("stream_1", 1, b"test data".to_vec()),
            MessageFixtures::stream_end("stream_1", true, None),
            MessageFixtures::config_update(json!({"key": "value"})),
        ];

        for message in messages {
            let serialized = serde_json::to_vec(&message)?;
            let deserialized: IpcMessage = serde_json::from_slice(&serialized)?;
            assert_eq!(deserialized.header.message_type, message.header.message_type);
            assert_eq!(deserialized.payload, message.payload);
        }

        Ok(())
    }

    /// Test serialization with special characters
    pub fn test_serialization_with_special_characters() -> IpcResult<()> {
        let special_data = json!({
            "unicode": "Hello 世界 🌍",
            "quotes": "String with 'single' and \"double\" quotes",
            "newlines": "Line 1\nLine 2\r\nLine 3",
            "tabs": "Column1\tColumn2\tColumn3",
            "backslashes": "Path\\to\\file",
            "null_char": "Text with \0 null character",
            "emoji": "😀 😃 😄 😁 😆 😅 😂 🤣"
        });

        let message = MessageFixtures::request("special_chars", special_data);

        let serialized = serde_json::to_vec(&message)?;
        let deserialized: IpcMessage = serde_json::from_slice(&serialized)?;

        assert_eq!(deserialized.payload, message.payload);

        Ok(())
    }

    /// Test backward compatibility
    pub fn test_backward_compatibility() -> IpcResult<()> {
        // Create a message with minimal fields (like older version)
        let minimal_message = json!({
            "header": {
                "version": 1,
                "message_type": "Heartbeat",
                "flags": {
                    "compressed": false,
                    "encrypted": false,
                    "requires_ack": false,
                    "no_retry": false,
                    "high_priority": false,
                    "stream": false,
                    "final_message": false
                },
                "message_id": Uuid::new_v4().to_string(),
                "session_id": Uuid::new_v4().to_string(),
                "timestamp": SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64,
                "source": "test",
                "priority": "Normal"
            },
            "payload": {
                "Heartbeat": {
                    "status": "Healthy",
                    "last_activity": SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64,
                    "resource_usage": {
                        "memory_bytes": 1024,
                        "cpu_percentage": 25.0,
                        "disk_bytes": 2048,
                        "network_bytes": 512,
                        "open_files": 5,
                        "active_threads": 1
                    },
                    "metrics": {},
                    "status_data": {}
                }
            }
        });

        let serialized = serde_json::to_vec(&minimal_message)?;
        let deserialized: IpcMessage = serde_json::from_slice(&serialized)?;
        assert!(deserialized.validate().is_ok());

        Ok(())
    }
}

/// Message routing tests
pub struct MessageRoutingTests;

impl MessageRoutingTests {
    /// Test message routing by destination
    pub fn test_message_routing_by_destination() -> IpcResult<()> {
        let messages = vec![
            MessageFixtures::request_to("plugin1", "op1", json!({})),
            MessageFixtures::request_to("plugin2", "op2", json!({})),
            MessageFixtures::request_to("plugin1", "op3", json!({})),
            MessageFixtures::heartbeat(), // No destination (broadcast)
        ];

        // Route messages by destination
        let mut plugin1_messages = Vec::new();
        let mut plugin2_messages = Vec::new();
        let mut broadcast_messages = Vec::new();

        for message in messages {
            match &message.header.destination {
                Some(dest) if dest == "plugin1" => plugin1_messages.push(message),
                Some(dest) if dest == "plugin2" => plugin2_messages.push(message),
                Some(_) => {}, // Other destinations
                None => broadcast_messages.push(message),
            }
        }

        assert_eq!(plugin1_messages.len(), 2);
        assert_eq!(plugin2_messages.len(), 1);
        assert_eq!(broadcast_messages.len(), 1);

        Ok(())
    }

    /// Test request-response correlation
    pub fn test_request_response_correlation() -> IpcResult<()> {
        let correlation_id = Uuid::new_v4().to_string();

        let request = MessageFixtures::request_to("plugin1", "test_op", json!({}));
        let request_with_corr = IpcMessage::with_header(
            request.header.message_type,
            request.payload,
            request.header,
        );
        // Manually set correlation ID for testing
        let mut req = request_with_corr;
        req.header.correlation_id = Some(correlation_id.clone());

        let response = MessageFixtures::success_response(&correlation_id, json!({"result": "success"}));

        // Test correlation matching
        assert_eq!(req.header.correlation_id, response.header.correlation_id);
        assert_eq!(response.header.message_type, MessageType::Response);

        Ok(())
    }

    /// Test message priority routing
    pub fn test_priority_routing() -> IpcResult<()> {
        let mut messages = vec![
            MessageFixtures::heartbeat(),
            MessageFixtures::request("test", json!({})),
            MessageFixtures::event("test", json!({})),
        ];

        // Set different priorities
        messages[0].header.priority = MessagePriority::Low;
        messages[1].header.priority = MessagePriority::High;
        messages[2].header.priority = MessagePriority::Critical;

        // Sort by priority
        messages.sort_by(|a, b| b.header.priority.cmp(&a.header.priority));

        assert_eq!(messages[0].header.priority, MessagePriority::Critical);
        assert_eq!(messages[1].header.priority, MessagePriority::High);
        assert_eq!(messages[2].header.priority, MessagePriority::Low);

        Ok(())
    }

    /// Test message filtering
    pub fn test_message_filtering() -> IpcResult<()> {
        let messages = vec![
            MessageFixtures::heartbeat(),
            MessageFixtures::request("test1", json!({})),
            MessageFixtures::request("test2", json!({})),
            MessageFixtures::event("test_event", json!({})),
            MessageFixtures::success_response("corr1", json!({})),
        ];

        // Filter by message type
        let requests: Vec<_> = messages.iter()
            .filter(|msg| msg.header.message_type == MessageType::Request)
            .collect();
        assert_eq!(requests.len(), 2);

        // Filter by priority
        let high_priority: Vec<_> = messages.iter()
            .filter(|msg| msg.header.priority == MessagePriority::Normal || msg.header.priority == MessagePriority::High)
            .collect();
        assert_eq!(high_priority.len(), 5); // All messages are at least Normal priority

        Ok(())
    }
}

/// Stream message tests
pub struct StreamMessageTests;

impl StreamMessageTests {
    /// Test stream chunk sequence
    pub fn test_stream_chunk_sequence() -> IpcResult<()> {
        let stream_id = Uuid::new_v4().to_string();
        let chunks = vec![
            ("Hello, ".to_string().into_bytes(), 1),
            ("world! ".to_string().into_bytes(), 2),
            ("This ".to_string().into_bytes(), 3),
            ("is ".to_string().into_bytes(), 4),
            ("a test.".to_string().into_bytes(), 5),
        ];

        let mut stream_messages = Vec::new();
        for (data, chunk_number) in &chunks {
            let message = MessageFixtures::stream_chunk(&stream_id, *chunk_number, data.clone());
            stream_messages.push(message);
        }

        // Add final message
        let final_message = MessageFixtures::stream_end(&stream_id, true, None);
        stream_messages.push(final_message);

        // Verify sequence
        assert_eq!(stream_messages.len(), 6);
        for (i, message) in stream_messages.iter().enumerate().take(5) {
            if let MessagePayload::StreamChunk(chunk) = &message.payload {
                assert_eq!(chunk.stream_id, stream_id);
                assert_eq!(chunk.chunk_number, i as u32 + 1);
            }
        }

        // Verify final message
        if let MessagePayload::StreamEnd(end) = &stream_messages[5].payload {
            assert_eq!(end.stream_id, stream_id);
            assert!(end.success);
        }

        Ok(())
    }

    /// Test stream error handling
    pub fn test_stream_error_handling() -> IpcResult<()> {
        let stream_id = Uuid::new_v4().to_string();
        let error_details = ErrorFixtures::error_details("STREAM_ERROR", "Stream processing failed");

        // Stream with error
        let error_message = MessageFixtures::stream_end(&stream_id, false, Some(error_details.clone()));

        if let MessagePayload::StreamEnd(end) = &error_message.payload {
            assert!(!end.success);
            assert!(end.error.is_some());
            assert_eq!(end.error, Some(error_details));
        }

        Ok(())
    }

    /// Test concurrent stream processing
    pub async fn test_concurrent_stream_processing() -> IpcResult<()> {
        let num_streams = 10;
        let chunks_per_stream = 5;

        let mut stream_ids = Vec::new();
        let mut all_messages = Vec::new();

        // Create multiple streams
        for _ in 0..num_streams {
            let stream_id = Uuid::new_v4().to_string();
            stream_ids.push(stream_id.clone());

            // Create chunks for this stream
            for chunk_num in 1..=chunks_per_stream {
                let data = format!("Stream {} chunk {}", stream_id, chunk_num).into_bytes();
                let message = MessageFixtures::stream_chunk(&stream_id, chunk_num, data);
                all_messages.push(message);
            }

            // Add end message
            let end_message = MessageFixtures::stream_end(&stream_id, true, None);
            all_messages.push(end_message);
        }

        // Verify total messages
        assert_eq!(all_messages.len(), num_streams * (chunks_per_stream + 1));

        // Group messages by stream
        let mut streams: HashMap<String, Vec<IpcMessage>> = HashMap::new();
        for message in all_messages {
            let stream_id = match &message.payload {
                MessagePayload::StreamChunk(chunk) => chunk.stream_id.clone(),
                MessagePayload::StreamEnd(end) => end.stream_id.clone(),
                _ => continue,
            };
            streams.entry(stream_id).or_insert_with(Vec::new).push(message);
        }

        // Verify each stream
        assert_eq!(streams.len(), num_streams);
        for (stream_id, messages) in streams {
            assert_eq!(messages.len(), chunks_per_stream + 1);

            // Verify chunk sequence
            let mut chunk_numbers = Vec::new();
            for message in &messages {
                if let MessagePayload::StreamChunk(chunk) = &message.payload {
                    chunk_numbers.push(chunk.chunk_number);
                }
            }
            chunk_numbers.sort();
            assert_eq!(chunk_numbers, (1..=chunks_per_stream).collect::<Vec<_>>());
        }

        Ok(())
    }
}

/// Performance tests for message operations
pub struct MessagePerformanceTests;

impl MessagePerformanceTests {
    /// Benchmark message creation performance
    pub fn benchmark_message_creation() -> IpcResult<(f64, f64)> {
        let iterations = 100_000;

        // Benchmark heartbeat creation
        let heartbeat_start = SystemTime::now();
        for _ in 0..iterations {
            let _ = MessageFixtures::heartbeat();
        }
        let heartbeat_duration = heartbeat_start.elapsed().unwrap();
        let heartbeat_ops_per_sec = iterations as f64 / heartbeat_duration.as_secs_f64();

        // Benchmark request creation
        let request_start = SystemTime::now();
        for i in 0..iterations {
            let _ = MessageFixtures::request(
                &format!("operation_{}", i),
                json!({"index": i}),
            );
        }
        let request_duration = request_start.elapsed().unwrap();
        let request_ops_per_sec = iterations as f64 / request_duration.as_secs_f64();

        Ok((heartbeat_ops_per_sec, request_ops_per_sec))
    }

    /// Benchmark message serialization performance
    pub fn benchmark_message_serialization() -> IpcResult<f64> {
        let message = MessageFixtures::full_message();
        let iterations = 10_000;

        let start = SystemTime::now();
        for _ in 0..iterations {
            let _ = serde_json::to_vec(&message).unwrap();
        }
        let duration = start.elapsed().unwrap();
        let ops_per_sec = iterations as f64 / duration.as_secs_f64();

        Ok(ops_per_sec)
    }

    /// Benchmark message deserialization performance
    pub fn benchmark_message_deserialization() -> IpcResult<f64> {
        let message = MessageFixtures::full_message();
        let serialized = serde_json::to_vec(&message)?;
        let iterations = 10_000;

        let start = SystemTime::now();
        for _ in 0..iterations {
            let _: IpcMessage = serde_json::from_slice(&serialized).unwrap();
        }
        let duration = start.elapsed().unwrap();
        let ops_per_sec = iterations as f64 / duration.as_secs_f64();

        Ok(ops_per_sec)
    }

    /// Benchmark message filtering performance
    pub fn benchmark_message_filtering() -> IpcResult<f64> {
        let messages: Vec<IpcMessage> = (0..10_000)
            .map(|i| MessageFixtures::request(
                &format!("operation_{}", i % 100),
                json!({"index": i}),
            ))
            .collect();

        let iterations = 1_000;

        let start = SystemTime::now();
        for _ in 0..iterations {
            let _filtered: Vec<_> = messages.iter()
                .filter(|msg| msg.header.message_type == MessageType::Request)
                .collect();
        }
        let duration = start.elapsed().unwrap();
        let ops_per_sec = iterations as f64 / duration.as_secs_f64();

        Ok(ops_per_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_message_creation() {
        MessageCreationTests::test_basic_message_creation().unwrap();
    }

    #[test]
    fn test_request_message_creation() {
        MessageCreationTests::test_request_message_creation().unwrap();
    }

    #[test]
    fn test_response_message_creation() {
        MessageCreationTests::test_response_message_creation().unwrap();
    }

    #[test]
    fn test_event_message_creation() {
        MessageCreationTests::test_event_message_creation().unwrap();
    }

    #[test]
    fn test_stream_message_creation() {
        MessageCreationTests::test_stream_message_creation().unwrap();
    }

    #[test]
    fn test_config_update_message_creation() {
        MessageCreationTests::test_config_update_message_creation().unwrap();
    }

    #[test]
    fn test_message_validation() {
        MessageValidationTests::test_message_validation().unwrap();
    }

    #[test]
    fn test_request_message_validation() {
        MessageValidationTests::test_request_message_validation().unwrap();
    }

    #[test]
    fn test_response_message_validation() {
        MessageValidationTests::test_response_message_validation().unwrap();
    }

    #[test]
    fn test_message_size_validation() {
        MessageValidationTests::test_message_size_validation().unwrap();
    }

    #[test]
    fn test_message_priority_validation() {
        MessageValidationTests::test_message_priority_validation().unwrap();
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        MessageSerializationTests::test_message_serialization_roundtrip().unwrap();
    }

    #[test]
    fn test_all_message_types_serialization() {
        MessageSerializationTests::test_all_message_types_serialization().unwrap();
    }

    #[test]
    fn test_serialization_with_special_characters() {
        MessageSerializationTests::test_serialization_with_special_characters().unwrap();
    }

    #[test]
    fn test_backward_compatibility() {
        MessageSerializationTests::test_backward_compatibility().unwrap();
    }

    #[test]
    fn test_message_routing_by_destination() {
        MessageRoutingTests::test_message_routing_by_destination().unwrap();
    }

    #[test]
    fn test_request_response_correlation() {
        MessageRoutingTests::test_request_response_correlation().unwrap();
    }

    #[test]
    fn test_priority_routing() {
        MessageRoutingTests::test_priority_routing().unwrap();
    }

    #[test]
    fn test_message_filtering() {
        MessageRoutingTests::test_message_filtering().unwrap();
    }

    #[test]
    fn test_stream_chunk_sequence() {
        StreamMessageTests::test_stream_chunk_sequence().unwrap();
    }

    #[test]
    fn test_stream_error_handling() {
        StreamMessageTests::test_stream_error_handling().unwrap();
    }

    async_test!(test_concurrent_stream_processing, {
        StreamMessageTests::test_concurrent_stream_processing().await.unwrap();
        "success"
    });

    #[test]
    fn test_message_creation_performance() {
        let (heartbeat_ops, request_ops) = MessagePerformanceTests::benchmark_message_creation().unwrap();
        assert!(heartbeat_ops > 10_000.0); // At least 10K ops/sec
        assert!(request_ops > 10_000.0); // At least 10K ops/sec
    }

    #[test]
    fn test_message_serialization_performance() {
        let ops_per_sec = MessagePerformanceTests::benchmark_message_serialization().unwrap();
        assert!(ops_per_sec > 1_000.0); // At least 1K ops/sec
    }

    #[test]
    fn test_message_deserialization_performance() {
        let ops_per_sec = MessagePerformanceTests::benchmark_message_deserialization().unwrap();
        assert!(ops_per_sec > 1_000.0); // At least 1K ops/sec
    }

    #[test]
    fn test_message_filtering_performance() {
        let ops_per_sec = MessagePerformanceTests::benchmark_message_filtering().unwrap();
        assert!(ops_per_sec > 100.0); // At least 100 ops/sec
    }
}