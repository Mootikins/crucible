//! # Protocol Component Tests
//!
//! Comprehensive tests for the IPC protocol implementation including message framing,
//! serialization, compression, encryption, and protocol negotiation.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use bytes::{BytesMut, BufMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::plugin_ipc::{
    error::{IpcError, IpcResult, ProtocolErrorCode},
    message::{
        IpcMessage, MessageHeader, MessagePayload, MessageType, MessageFlags,
        ClientCapabilities, HeartbeatPayload, ResourceUsage, HeartbeatStatus,
    },
    protocol::{ProtocolHandler, ProtocolCapabilities, ProtocolStats, MessageFramer},
    security::SecurityManager,
};

use super::common::{
    *,
    fixtures::*,
    mocks::*,
    helpers::*,
};

/// Protocol test suite
pub struct ProtocolTests;

impl ProtocolTests {
    /// Test protocol handler creation and initialization
    pub async fn test_protocol_handler_creation() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        assert_eq!(handler.version, 1);
        assert!(!handler.compression_algos.is_empty());
        assert!(!handler.encryption_algos.is_empty());

        let stats = handler.get_stats().await;
        assert_eq!(stats.version, 1);
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);

        Ok(())
    }

    /// Test protocol capabilities negotiation
    pub async fn test_capabilities_negotiation() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        // Test with full capabilities
        let client_capabilities = CapabilityFixtures::full_client();
        let capabilities = handler.negotiate_capabilities(&client_capabilities).await?;

        assert_eq!(capabilities.version, 1);
        assert!(capabilities.compression_enabled);
        assert!(capabilities.encryption_enabled);
        assert!(capabilities.compression_algorithm.is_some());
        assert!(capabilities.encryption_algorithm.is_some());
        assert!(capabilities.supported_features.contains(&"heartbeat".to_string()));
        assert!(capabilities.supported_features.contains(&"batching".to_string()));
        assert!(capabilities.supported_features.contains(&"streaming".to_string()));
        assert!(capabilities.supported_features.contains(&"cancellation".to_string()));

        // Test with minimal capabilities
        let minimal_capabilities = CapabilityFixtures::minimal_client();
        let minimal_negotiated = handler.negotiate_capabilities(&minimal_capabilities).await?;

        assert_eq!(minimal_negotiated.version, 1);
        assert!(!minimal_negotiated.compression_enabled);
        assert!(!minimal_negotiated.encryption_enabled);
        assert!(minimal_negotiated.compression_algorithm.is_none());
        assert!(minimal_negotiated.encryption_algorithm.is_none());

        Ok(())
    }

    /// Test message framing and unframing
    pub async fn test_message_framing() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        // Test heartbeat message
        let original_message = MessageFixtures::heartbeat();
        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::basic_client()).await?;

        let framed = handler.frame_message(&mut original_message.clone()).await?;
        assert!(framed.len() >= 12); // At least header size

        let unframed = handler.unframe_message(&framed).await?;
        assert_eq!(unframed.header.message_type, original_message.header.message_type);
        assert_eq!(unframed.payload, original_message.payload);

        Ok(())
    }

    /// Test message framing with compression
    pub async fn test_message_compression() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        let large_message = MessageFixtures::large_message(10 * 1024); // 10KB message
        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::full_client()).await?;

        let framed = handler.frame_message(&mut large_message.clone()).await?;
        let unframed = handler.unframe_message(&framed).await?;

        assert_eq!(unframed.header.message_type, large_message.header.message_type);
        assert_eq!(unframed.payload, large_message.payload);

        Ok(())
    }

    /// Test message framing with encryption
    pub async fn test_message_encryption() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        let message = MessageFixtures::request("test_operation", serde_json::json!({"data": "sensitive"}));
        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::full_client()).await?;

        // Set up a session for encryption
        security_manager.create_session(message.header.session_id.clone()).await;

        let framed = handler.frame_message(&mut message.clone()).await?;
        let unframed = handler.unframe_message(&framed).await?;

        assert_eq!(unframed.header.message_type, message.header.message_type);
        assert_eq!(unframed.payload, message.payload);

        Ok(())
    }

    /// Test message framing with both compression and encryption
    pub async fn test_message_compression_and_encryption() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        let message = MessageFixtures::large_message(50 * 1024); // 50KB message
        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::full_client()).await?;

        // Set up a session for encryption
        security_manager.create_session(message.header.session_id.clone()).await;

        let framed = handler.frame_message(&mut message.clone()).await?;
        let unframed = handler.unframe_message(&framed).await?;

        assert_eq!(unframed.header.message_type, message.header.message_type);
        assert_eq!(unframed.payload, message.payload);

        Ok(())
    }

    /// Test protocol version compatibility
    pub async fn test_version_compatibility() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        // Test with matching version
        let message = MessageFixtures::heartbeat();
        let framed = handler.frame_message(&mut message.clone()).await?;
        let result = handler.unframe_message(&framed);
        assert!(result.is_ok());

        // Test with mismatched version
        let mut invalid_frame = framed.clone();
        // Change version in frame header
        invalid_frame[0] = 2; // Set version to 2
        let result = handler.unframe_message(&invalid_frame);
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("VersionMismatch"));

        Ok(())
    }

    /// Test message size limits
    pub async fn test_message_size_limits() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        // Test with message within limits
        let normal_message = MessageFixtures::large_message(1024 * 1024); // 1MB
        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::basic_client()).await?;
        let result = handler.frame_message(&mut normal_message).await;
        assert!(result.is_ok());

        // Test with oversized message
        let oversized_message = MessageFixtures::large_message(20 * 1024 * 1024); // 20MB
        let result = handler.frame_message(&mut oversized_message).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("MessageTooLarge"));

        Ok(())
    }

    /// Test message validation
    pub async fn test_message_validation() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        // Test valid message
        let valid_message = MessageFixtures::full_message();
        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::basic_client()).await?;
        let result = handler.frame_message(&mut valid_message).await;
        assert!(result.is_ok());

        // Test message with invalid timestamp (future)
        let mut invalid_message = MessageFixtures::heartbeat();
        invalid_message.header.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64 + 3600_000_000_000; // 1 hour in the future

        let result = handler.frame_message(&mut invalid_message).await;
        assert!(result.is_err());

        Ok(())
    }

    /// Test checksum verification
    pub async fn test_checksum_verification() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        let message = MessageFixtures::heartbeat();
        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::basic_client()).await?;
        let framed = handler.frame_message(&mut message.clone()).await?;

        // Test with valid checksum
        let result = handler.unframe_message(&framed).await;
        assert!(result.is_ok());

        // Test with corrupted checksum
        let mut corrupted_frame = framed.clone();
        // Corrupt the checksum (last 4 bytes of header)
        let checksum_pos = 8;
        corrupted_frame[checksum_pos] ^= 0xFF;
        let result = handler.unframe_message(&corrupted_frame);
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("ChecksumMismatch"));

        // Test with corrupted payload
        let mut corrupted_frame = framed.clone();
        // Corrupt a byte in the payload
        let payload_start = 12;
        if corrupted_frame.len() > payload_start {
            corrupted_frame[payload_start] ^= 0xFF;
        }
        let result = handler.unframe_message(&corrupted_frame);
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("ChecksumMismatch"));

        Ok(())
    }

    /// Test protocol statistics
    pub async fn test_protocol_statistics() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        // Check initial stats
        let stats = handler.get_stats().await;
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);

        // Reset stats
        handler.reset_stats().await;
        let stats = handler.get_stats().await;
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);

        Ok(())
    }

    /// Test concurrent message processing
    pub async fn test_concurrent_message_processing() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = Arc::new(ProtocolHandler::new(Arc::new(security_manager)));

        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::full_client()).await?;

        // Create multiple messages
        let messages: Vec<IpcMessage> = (0..100)
            .map(|i| MessageFixtures::request(
                &format!("operation_{}", i),
                serde_json::json!({"index": i})
            ))
            .collect();

        // Process messages concurrently
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            messages.len(),
            |i| {
                let handler = Arc::clone(&handler);
                let message = messages[i].clone();
                async move {
                    let framed = handler.frame_message(&mut message.clone()).await?;
                    let unframed = handler.unframe_message(&framed).await?;
                    Ok(unframed)
                }
            },
        ).await;

        // Verify all operations succeeded
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, messages.len());

        // Verify message integrity
        for (i, result) in results.iter().enumerate() {
            if let Ok(unframed) = result {
                assert_eq!(unframed.header.message_type, MessageType::Request);
                // Check that the operation parameter matches
                if let MessagePayload::Request(req) = &unframed.payload {
                    assert!(req.parameters["index"].as_u64().unwrap() == i as u64);
                }
            }
        }

        Ok(())
    }

    /// Test error handling in protocol operations
    pub async fn test_protocol_error_handling() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));

        // Test incomplete frame
        let incomplete_frame = vec![0u8; 10]; // Less than header size
        let result = handler.unframe_message(&incomplete_frame);
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("InvalidHeader"));

        // Test oversized frame
        let mut oversized_frame = Vec::new();
        oversized_frame.extend_from_slice(&[1u8; 8]); // Header
        oversized_frame.extend_from_slice(&((20 * 1024 * 1024u32).to_be_bytes())); // 20MB payload
        let result = handler.unframe_message(&oversized_frame);
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("MessageTooLarge"));

        // Test invalid JSON
        let mut invalid_json_frame = Vec::new();
        invalid_json_frame.extend_from_slice(&[1u8; 8]); // Header
        let invalid_payload = b"{ invalid json }";
        invalid_json_frame.extend_from_slice(&(invalid_payload.len() as u32).to_be_bytes());
        invalid_json_frame.extend_from_slice(&calculate_checksum(invalid_payload));
        invalid_json_frame.extend_from_slice(invalid_payload);

        let result = handler.unframe_message(&invalid_json_frame);
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("DeserializationFailed"));

        Ok(())
    }
}

/// Message framer tests
pub struct MessageFramerTests;

impl MessageFramerTests {
    /// Test basic frame extraction
    pub fn test_basic_frame_extraction() -> IpcResult<()> {
        let mut framer = MessageFramer::new(1024);

        // Add incomplete data
        let frames = framer.add_data(b"Hello")?;
        assert_eq!(frames.len(), 0);
        assert_eq!(framer.buffer_size(), 5);

        // Add frame header and payload
        let mut frame_data = Vec::new();
        frame_data.extend_from_slice(&[0u8; 8]); // Version and other header fields
        frame_data.extend_from_slice(&(5u32).to_be_bytes()); // Payload length
        frame_data.extend_from_slice(&calculate_checksum(b"Hello")); // Checksum
        frame_data.extend_from_slice(b"Hello"); // Payload

        let frames = framer.add_data(&frame_data)?;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].len(), 8 + 4 + 4 + 5); // Header + length + checksum + payload

        Ok(())
    }

    /// Test multiple frames in buffer
    pub fn test_multiple_frame_extraction() -> IpcResult<()> {
        let mut framer = MessageFramer::new(1024);

        // Create two frames
        let mut frame_data = Vec::new();

        // First frame
        frame_data.extend_from_slice(&[0u8; 8]);
        frame_data.extend_from_slice(&(5u32).to_be_bytes());
        frame_data.extend_from_slice(&calculate_checksum(b"Hello"));
        frame_data.extend_from_slice(b"Hello");

        // Second frame
        frame_data.extend_from_slice(&[0u8; 8]);
        frame_data.extend_from_slice(&(6u32).to_be_bytes());
        frame_data.extend_from_slice(&calculate_checksum(b"World!"));
        frame_data.extend_from_slice(b"World!");

        let frames = framer.add_data(&frame_data)?;
        assert_eq!(frames.len(), 2);

        Ok(())
    }

    /// Test frame size limits
    pub fn test_frame_size_limits() -> IpcResult<()> {
        let mut framer = MessageFramer::new(100); // Small max size

        // Create frame that exceeds max size
        let mut frame_data = Vec::new();
        frame_data.extend_from_slice(&[0u8; 8]); // Header
        frame_data.extend_from_slice(&((200u32).to_be_bytes())); // Large payload

        let result = framer.add_data(&frame_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("MessageTooLarge"));

        Ok(())
    }

    /// Test buffer clearing
    pub fn test_buffer_clearing() -> IpcResult<()> {
        let mut framer = MessageFramer::new(1024);

        // Add some data
        framer.add_data(b"Some data")?;
        assert_eq!(framer.buffer_size(), 9);

        // Clear buffer
        framer.clear();
        assert_eq!(framer.buffer_size(), 0);

        Ok(())
    }
}

/// Performance tests for protocol operations
pub struct ProtocolPerformanceTests;

impl ProtocolPerformanceTests {
    /// Benchmark message framing performance
    pub async fn benchmark_framing_performance() -> IpcResult<(f64, f64)> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));
        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::full_client()).await?;

        let message = MessageFixtures::large_message(10 * 1024); // 10KB
        let iterations = 1000;

        // Benchmark framing
        let (frame_duration, _, _) = PerformanceTestUtils::measure_memory_usage(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                for _ in 0..iterations {
                    let _ = handler.frame_message(&mut message.clone()).await.unwrap();
                }
            });
        });

        let framing_ops_per_sec = iterations as f64 / frame_duration.as_secs_f64();

        // Benchmark unframing
        let framed = handler.frame_message(&mut message.clone()).await.unwrap();
        let (unframe_duration, _, _) = PerformanceTestUtils::measure_memory_usage(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                for _ in 0..iterations {
                    let _ = handler.unframe_message(&framed).await.unwrap();
                }
            });
        });

        let unframing_ops_per_sec = iterations as f64 / unframe_duration.as_secs_f64();

        Ok((framing_ops_per_sec, unframing_ops_per_sec))
    }

    /// Benchmark throughput with different message sizes
    pub async fn benchmark_throughput_by_size() -> IpcResult<Vec<(usize, f64)>> {
        let security_manager = MockSecurityManager::new();
        let handler = ProtocolHandler::new(Arc::new(security_manager));
        let capabilities = handler.negotiate_capabilities(&CapabilityFixtures::full_client()).await?;

        let message_sizes = vec![1024, 4096, 16384, 65536, 262144]; // 1KB to 256KB
        let mut results = Vec::new();

        for size in message_sizes {
            let message = MessageFixtures::large_message(size);
            let iterations = std::cmp::max(1, 100_000 / size); // Adjust iterations based on size

            let start = SystemTime::now();
            for _ in 0..iterations {
                let framed = handler.frame_message(&mut message.clone()).await.unwrap();
                let _ = handler.unframe_message(&framed).await.unwrap();
            }
            let duration = start.elapsed().unwrap();

            let total_bytes = size * iterations;
            let throughput_mbps = (total_bytes as f64) / (1024.0 * 1024.0) / duration.as_secs_f64();

            results.push((size, throughput_mbps));
        }

        Ok(results)
    }
}

/// Test helper function to calculate checksum
fn calculate_checksum(data: &[u8]) -> [u8; 4] {
    let mut checksum = 0u32;
    for &byte in data {
        checksum = checksum.wrapping_mul(31).wrapping_add(byte as u32);
    }
    checksum.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    async_test!(test_protocol_handler_creation, {
        ProtocolTests::test_protocol_handler_creation().await.unwrap();
        "success"
    });

    async_test!(test_capabilities_negotiation, {
        ProtocolTests::test_capabilities_negotiation().await.unwrap();
        "success"
    });

    async_test!(test_message_framing, {
        ProtocolTests::test_message_framing().await.unwrap();
        "success"
    });

    async_test!(test_message_compression, {
        ProtocolTests::test_message_compression().await.unwrap();
        "success"
    });

    async_test!(test_message_encryption, {
        ProtocolTests::test_message_encryption().await.unwrap();
        "success"
    });

    async_test!(test_version_compatibility, {
        ProtocolTests::test_version_compatibility().await.unwrap();
        "success"
    });

    async_test!(test_message_size_limits, {
        ProtocolTests::test_message_size_limits().await.unwrap();
        "success"
    });

    async_test!(test_checksum_verification, {
        ProtocolTests::test_checksum_verification().await.unwrap();
        "success"
    });

    async_test!(test_concurrent_message_processing, {
        ProtocolTests::test_concurrent_message_processing().await.unwrap();
        "success"
    });

    async_test!(test_protocol_error_handling, {
        ProtocolTests::test_protocol_error_handling().await.unwrap();
        "success"
    });

    #[test]
    fn test_basic_frame_extraction() {
        MessageFramerTests::test_basic_frame_extraction().unwrap();
    }

    #[test]
    fn test_multiple_frame_extraction() {
        MessageFramerTests::test_multiple_frame_extraction().unwrap();
    }

    #[test]
    fn test_frame_size_limits() {
        MessageFramerTests::test_frame_size_limits().unwrap();
    }

    #[test]
    fn test_buffer_clearing() {
        MessageFramerTests::test_buffer_clearing().unwrap();
    }

    async_test!(test_framing_performance, {
        let (framing_ops, unframing_ops) = ProtocolPerformanceTests::benchmark_framing_performance().await.unwrap();
        assert!(framing_ops > 1000.0); // Should handle at least 1000 ops/sec
        assert!(unframing_ops > 1000.0); // Should handle at least 1000 ops/sec
        (framing_ops, unframing_ops)
    });

    async_test!(test_throughput_by_size, {
        let results = ProtocolPerformanceTests::benchmark_throughput_by_size().await.unwrap();
        assert!(!results.is_empty());
        // All sizes should achieve reasonable throughput
        for (_, throughput) in &results {
            assert!(*throughput > 1.0); // At least 1 MB/s
        }
        results.len()
    });
}