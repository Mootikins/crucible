//! # IPC Protocol Implementation
//!
//! Core protocol implementation handling message framing, serialization, and protocol
//! negotiation for the plugin IPC system.

use crate::plugin_ipc::{
    error::{IpcError, IpcResult},
    message::{IpcMessage, MessageHeader, MessageFlags, MessageType},
    security::SecurityManager,
};
use bytes::{Buf, BufMut, BytesMut};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Protocol handler for managing IPC communication
pub struct ProtocolHandler {
    /// Protocol version
    version: u8,
    /// Security manager
    security_manager: Arc<SecurityManager>,
    /// Supported compression algorithms
    compression_algos: Vec<String>,
    /// Supported encryption algorithms
    encryption_algos: Vec<String>,
    /// Protocol state
    state: Arc<RwLock<ProtocolState>>,
}

impl ProtocolHandler {
    /// Create a new protocol handler
    pub fn new(security_manager: Arc<SecurityManager>) -> Self {
        Self {
            version: crate::plugin_ipc::PROTOCOL_VERSION,
            security_manager,
            compression_algos: vec![
                "lz4".to_string(),
                "zstd".to_string(),
                "gzip".to_string(),
            ],
            encryption_algos: vec![
                "aes256gcm".to_string(),
                "chacha20poly1305".to_string(),
            ],
            state: Arc::new(RwLock::new(ProtocolState::new())),
        }
    }

    /// Negotiate protocol capabilities during handshake
    pub async fn negotiate_capabilities(
        &self,
        client_capabilities: &crate::plugin_ipc::message::ClientCapabilities,
    ) -> IpcResult<ProtocolCapabilities> {
        let mut state = self.state.write().await;

        // Determine supported compression
        let compression_algo = if client_capabilities.supports_compression {
            self.compression_algos.first().cloned()
        } else {
            None
        };

        // Determine supported encryption
        let encryption_algo = if client_capabilities.supports_encryption {
            self.encryption_algos.first().cloned()
        } else {
            None
        };

        let capabilities = ProtocolCapabilities {
            version: self.version,
            compression_enabled: compression_algo.is_some(),
            compression_algorithm: compression_algo,
            encryption_enabled: encryption_algo.is_some(),
            encryption_algorithm: encryption_algo,
            max_message_size: crate::plugin_ipc::MAX_MESSAGE_SIZE,
            supported_features: vec![
                "heartbeat".to_string(),
                "batching".to_string(),
                "streaming".to_string(),
                "cancellation".to_string(),
            ],
        };

        state.negotiated_capabilities = Some(capabilities.clone());
        Ok(capabilities)
    }

    /// Frame a message for transmission
    pub async fn frame_message(&self, message: &mut IpcMessage) -> IpcResult<Vec<u8>> {
        let state = self.state.read().await;
        let capabilities = state.negotiated_capabilities.as_ref()
            .ok_or_else(|| IpcError::Protocol {
                message: "Protocol capabilities not negotiated".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::ProtocolViolation,
                source: None,
            })?;

        // Validate message
        message.validate()?;

        // Apply compression if enabled
        if capabilities.compression_enabled {
            self.compress_message(message).await?;
            message.header.flags.compressed = true;
        }

        // Apply encryption if enabled
        if capabilities.encryption_enabled {
            self.encrypt_message(message).await?;
            message.header.flags.encrypted = true;
        }

        // Serialize message
        let serialized = serde_json::to_vec(message)
            .map_err(|e| IpcError::Protocol {
                message: format!("Failed to serialize message: {}", e),
                code: crate::plugin_ipc::error::ProtocolErrorCode::SerializationFailed,
                source: None,
            })?;

        // Create frame with header and payload
        let mut frame = BytesMut::new();

        // Frame header
        frame.put_u32(self.version as u32);
        frame.put_u32(serialized.len() as u32);
        frame.put_u32(calculate_checksum(&serialized));

        // Message payload
        frame.put_slice(&serialized);

        Ok(frame.to_vec())
    }

    /// Unframe a received message
    pub async fn unframe_message(&self, frame: &[u8]) -> IpcResult<IpcMessage> {
        if frame.len() < 12 {
            return Err(IpcError::Protocol {
                message: "Frame too short".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::InvalidHeader,
                source: None,
            });
        }

        let mut cursor = std::io::Cursor::new(frame);

        // Read frame header
        let version = cursor.get_u32();
        if version != self.version as u32 {
            return Err(IpcError::Protocol {
                message: format!("Version mismatch: expected {}, got {}", self.version, version),
                code: crate::plugin_ipc::error::ProtocolErrorCode::VersionMismatch,
                source: None,
            });
        }

        let payload_length = cursor.get_u32() as usize;
        let expected_checksum = cursor.get_u32();

        if frame.len() < 12 + payload_length {
            return Err(IpcError::Protocol {
                message: "Incomplete frame".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::InvalidMessageFormat,
                source: None,
            });
        }

        // Extract payload
        let start_pos = 12;
        let end_pos = start_pos + payload_length;
        let payload = &frame[start_pos..end_pos];

        // Verify checksum
        let actual_checksum = calculate_checksum(payload);
        if actual_checksum != expected_checksum {
            return Err(IpcError::Protocol {
                message: "Checksum mismatch".to_string(),
                code: crate::plugin_ipc::error::ProtocolErrorCode::ChecksumMismatch,
                source: None,
            });
        }

        // Deserialize message
        let mut message: IpcMessage = serde_json::from_slice(payload)
            .map_err(|e| IpcError::Protocol {
                message: format!("Failed to deserialize message: {}", e),
                code: crate::plugin_ipc::error::ProtocolErrorCode::DeserializationFailed,
                source: None,
            })?;

        // Apply decryption if needed
        if message.header.flags.encrypted {
            self.decrypt_message(&mut message).await?;
        }

        // Apply decompression if needed
        if message.header.flags.compressed {
            self.decompress_message(&mut message).await?;
        }

        // Validate message
        message.validate()?;

        Ok(message)
    }

    /// Send a message with proper framing
    pub async fn send_message<W>(&self, writer: &mut W, mut message: IpcMessage) -> IpcResult<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let frame = self.frame_message(&mut message).await?;

        writer.write_all(&frame).await
            .map_err(|e| IpcError::Connection {
                message: format!("Failed to write message frame: {}", e),
                code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                endpoint: "unknown".to_string(),
                retry_count: 0,
            })?;

        writer.flush().await
            .map_err(|e| IpcError::Connection {
                message: format!("Failed to flush message: {}", e),
                code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                endpoint: "unknown".to_string(),
                retry_count: 0,
            })?;

        debug!("Sent message: {} to {}", message.header.message_type, message.header.destination.as_deref().unwrap_or("unknown"));
        Ok(())
    }

    /// Receive a message with proper unframing
    pub async fn receive_message<R>(&self, reader: &mut R) -> IpcResult<IpcMessage>
    where
        R: AsyncReadExt + Unpin,
    {
        // Read frame header
        let mut header_bytes = [0u8; 12];
        reader.read_exact(&mut header_bytes).await
            .map_err(|e| IpcError::Connection {
                message: format!("Failed to read frame header: {}", e),
                code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                endpoint: "unknown".to_string(),
                retry_count: 0,
            })?;

        // Extract payload length
        let payload_length = u32::from_be_bytes([header_bytes[4], header_bytes[5], header_bytes[6], header_bytes[7]]) as usize;

        // Check message size limit
        if payload_length > crate::plugin_ipc::MAX_MESSAGE_SIZE {
            return Err(IpcError::Message {
                message: format!("Message too large: {} bytes", payload_length),
                code: crate::plugin_ipc::error::MessageErrorCode::MessageTooLarge,
                message_id: None,
            });
        }

        // Read payload
        let mut payload = vec![0u8; payload_length];
        reader.read_exact(&mut payload).await
            .map_err(|e| IpcError::Connection {
                message: format!("Failed to read message payload: {}", e),
                code: crate::plugin_ipc::error::ConnectionErrorCode::TransportError,
                endpoint: "unknown".to_string(),
                retry_count: 0,
            })?;

        // Combine header and payload for unframing
        let mut frame = Vec::with_capacity(12 + payload_length);
        frame.extend_from_slice(&header_bytes);
        frame.extend_from_slice(&payload);

        let message = self.unframe_message(&frame)?;
        debug!("Received message: {} from {}", message.header.message_type, message.header.source);

        Ok(message)
    }

    /// Compress message payload
    async fn compress_message(&self, message: &mut IpcMessage) -> IpcResult<()> {
        // This is a placeholder for compression implementation
        // In a real implementation, you would use a compression library
        match &message.payload {
            crate::plugin_ipc::message::MessagePayload::Request(_) |
            crate::plugin_ipc::message::MessagePayload::Response(_) |
            crate::plugin_ipc::message::MessagePayload::Event(_) => {
                // Compress the payload if it's compressible
                debug!("Compressing message payload");
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Decompress message payload
    async fn decompress_message(&self, message: &mut IpcMessage) -> IpcResult<()> {
        // This is a placeholder for decompression implementation
        debug!("Decompressing message payload");
        Ok(())
    }

    /// Encrypt message payload
    async fn encrypt_message(&self, message: &mut IpcMessage) -> IpcResult<()> {
        let session_id = &message.header.session_id;
        let serialized = serde_json::to_vec(&message.payload)
            .map_err(|e| IpcError::Protocol {
                message: format!("Failed to serialize payload for encryption: {}", e),
                code: crate::plugin_ipc::error::ProtocolErrorCode::SerializationFailed,
                source: None,
            })?;

        let encrypted = self.security_manager.encrypt_message(session_id, &serialized)
            .await?;

        // Replace payload with encrypted data
        message.payload = crate::plugin_ipc::message::MessagePayload::Unknown(serde_json::Value::String(
            base64::encode(&encrypted)
        ));

        Ok(())
    }

    /// Decrypt message payload
    async fn decrypt_message(&self, message: &mut IpcMessage) -> IpcResult<()> {
        let session_id = &message.header.session_id;

        if let crate::plugin_ipc::message::MessagePayload::Unknown(serde_json::Value::String(encoded_data)) = &message.payload {
            let encrypted = base64::decode(encoded_data)
                .map_err(|_| IpcError::Protocol {
                    message: "Invalid base64 encoding for encrypted payload".to_string(),
                    code: crate::plugin_ipc::error::ProtocolErrorCode::DecryptionFailed,
                    source: None,
                })?;

            let decrypted = self.security_manager.decrypt_message(session_id, &encrypted)
                .await?;

            // Restore original payload
            let original_payload: crate::plugin_ipc::message::MessagePayload = serde_json::from_slice(&decrypted)
                .map_err(|e| IpcError::Protocol {
                    message: format!("Failed to deserialize decrypted payload: {}", e),
                    code: crate::plugin_ipc::error::ProtocolErrorCode::DeserializationFailed,
                    source: None,
                })?;

            message.payload = original_payload;
        }

        Ok(())
    }

    /// Get protocol statistics
    pub async fn get_stats(&self) -> ProtocolStats {
        let state = self.state.read().await;
        ProtocolStats {
            version: self.version,
            messages_sent: state.messages_sent,
            messages_received: state.messages_received,
            bytes_sent: state.bytes_sent,
            bytes_received: state.bytes_received,
            compression_ratio: state.compression_ratio,
            encryption_enabled: state.negotiated_capabilities
                .as_ref()
                .map(|c| c.encryption_enabled)
                .unwrap_or(false),
            compression_enabled: state.negotiated_capabilities
                .as_ref()
                .map(|c| c.compression_enabled)
                .unwrap_or(false),
        }
    }

    /// Reset protocol statistics
    pub async fn reset_stats(&self) {
        let mut state = self.state.write().await;
        state.messages_sent = 0;
        state.messages_received = 0;
        state.bytes_sent = 0;
        state.bytes_received = 0;
        state.compression_ratio = 0.0;
    }
}

/// Protocol state
#[derive(Debug)]
struct ProtocolState {
    negotiated_capabilities: Option<ProtocolCapabilities>,
    messages_sent: u64,
    messages_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
    compression_ratio: f64,
}

impl ProtocolState {
    fn new() -> Self {
        Self {
            negotiated_capabilities: None,
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            compression_ratio: 0.0,
        }
    }
}

/// Negotiated protocol capabilities
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProtocolCapabilities {
    pub version: u8,
    pub compression_enabled: bool,
    pub compression_algorithm: Option<String>,
    pub encryption_enabled: bool,
    pub encryption_algorithm: Option<String>,
    pub max_message_size: usize,
    pub supported_features: Vec<String>,
}

/// Protocol statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProtocolStats {
    pub version: u8,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub compression_ratio: f64,
    pub encryption_enabled: bool,
    pub compression_enabled: bool,
}

/// Calculate simple checksum for message integrity
fn calculate_checksum(data: &[u8]) -> u32 {
    // Simple CRC32-like checksum
    // In a real implementation, you would use a proper hash function
    let mut checksum = 0u32;
    for &byte in data {
        checksum = checksum.wrapping_mul(31).wrapping_add(byte as u32);
    }
    checksum
}

/// Message framer for handling binary protocol framing
pub struct MessageFramer {
    buffer: BytesMut,
    max_frame_size: usize,
}

impl MessageFramer {
    pub fn new(max_frame_size: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(8192),
            max_frame_size,
        }
    }

    /// Add data to the buffer and try to extract complete frames
    pub fn add_data(&mut self, data: &[u8]) -> IpcResult<Vec<Vec<u8>>> {
        self.buffer.extend_from_slice(data);
        self.extract_frames()
    }

    /// Extract complete frames from the buffer
    fn extract_frames(&mut self) -> IpcResult<Vec<Vec<u8>>> {
        let mut frames = Vec::new();

        while self.buffer.len() >= 12 {
            // Read frame header to determine payload length
            let payload_length = u32::from_be_bytes([
                self.buffer[4],
                self.buffer[5],
                self.buffer[6],
                self.buffer[7],
            ]) as usize;

            let total_frame_size = 12 + payload_length;

            // Check if we have enough data
            if self.buffer.len() < total_frame_size {
                break;
            }

            // Check frame size limit
            if total_frame_size > self.max_frame_size {
                return Err(IpcError::Protocol {
                    message: format!("Frame too large: {} bytes", total_frame_size),
                    code: crate::plugin_ipc::error::ProtocolErrorCode::MessageTooLarge,
                    source: None,
                });
            }

            // Extract frame
            let frame = self.buffer.split_to(total_frame_size).freeze();
            frames.push(frame.to_vec());
        }

        Ok(frames)
    }

    /// Get the current buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_security_manager() -> Arc<SecurityManager> {
        Arc::new(SecurityManager::new(
            crate::plugin_ipc::security::AuthConfig::default(),
            crate::plugin_ipc::security::EncryptionConfig::default(),
            crate::plugin_ipc::security::AuthorizationConfig::default(),
        ))
    }

    #[tokio::test]
    async fn test_protocol_handler_creation() {
        let security_manager = create_test_security_manager();
        let handler = ProtocolHandler::new(security_manager);
        assert_eq!(handler.version, crate::plugin_ipc::PROTOCOL_VERSION);
    }

    #[tokio::test]
    async fn test_capabilities_negotiation() {
        let security_manager = create_test_security_manager();
        let handler = ProtocolHandler::new(security_manager);

        let client_capabilities = crate::plugin_ipc::message::ClientCapabilities {
            plugin_types: vec![],
            operations: vec![],
            data_formats: vec![],
            max_concurrent_requests: 10,
            supports_streaming: false,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
        };

        let capabilities = handler.negotiate_capabilities(&client_capabilities).await.unwrap();
        assert_eq!(capabilities.version, crate::plugin_ipc::PROTOCOL_VERSION);
        assert!(capabilities.compression_enabled);
        assert!(capabilities.encryption_enabled);
    }

    #[tokio::test]
    async fn test_message_framing() {
        let security_manager = create_test_security_manager();
        let handler = ProtocolHandler::new(security_manager);

        let message = crate::plugin_ipc::message::IpcMessage::new(
            crate::plugin_ipc::message::MessageType::Heartbeat,
            crate::plugin_ipc::message::MessagePayload::Heartbeat(
                crate::plugin_ipc::message::HeartbeatPayload {
                    status: crate::plugin_ipc::message::HeartbeatStatus::Healthy,
                    last_activity: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as u64,
                    resource_usage: crate::plugin_ipc::message::ResourceUsage {
                        memory_bytes: 1024,
                        cpu_percentage: 50.0,
                        disk_bytes: 2048,
                        network_bytes: 512,
                        open_files: 10,
                        active_threads: 2,
                    },
                    metrics: std::collections::HashMap::new(),
                    status_data: std::collections::HashMap::new(),
                }
            ),
        );

        let framed = handler.frame_message(&mut message.clone()).await.unwrap();
        assert!(framed.len() >= 12); // At least header size

        let unframed = handler.unframe_message(&framed).await.unwrap();
        assert_eq!(unframed.header.message_type, message.header.message_type);
    }

    #[test]
    fn test_checksum_calculation() {
        let data1 = b"Hello, World!";
        let data2 = b"Hello, World!";

        let checksum1 = calculate_checksum(data1);
        let checksum2 = calculate_checksum(data2);
        assert_eq!(checksum1, checksum2);

        let data3 = b"Hello, World";
        let checksum3 = calculate_checksum(data3);
        assert_ne!(checksum1, checksum3);
    }

    #[test]
    fn test_message_framer() {
        let mut framer = MessageFramer::new(1024);

        // Test incomplete data
        let frames = framer.add_data(b"Hello").unwrap();
        assert_eq!(frames.len(), 0);
        assert_eq!(framer.buffer_size(), 5);

        // Test with valid frame (header + payload)
        let mut frame_data = Vec::new();
        frame_data.extend_from_slice(&[0u8; 8]); // Version and other header fields
        frame_data.extend_from_slice(&(5u32).to_be_bytes()); // Payload length
        frame_data.extend_from_slice(b"Hello"); // Payload

        let frames = framer.add_data(&frame_data).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].len(), 13); // 8 + 4 + 5
    }

    #[test]
    fn test_message_framer_large_frame() {
        let mut framer = MessageFramer::new(100);

        // Create frame that exceeds max size
        let mut frame_data = Vec::new();
        frame_data.extend_from_slice(&[0u8; 8]); // Header
        frame_data.extend_from_slice(&((200u32).to_be_bytes())); // Large payload

        let result = framer.add_data(&frame_data);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_protocol_stats() {
        let security_manager = create_test_security_manager();
        let handler = ProtocolHandler::new(security_manager);

        let stats = handler.get_stats().await;
        assert_eq!(stats.version, crate::plugin_ipc::PROTOCOL_VERSION);
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);

        handler.reset_stats().await;
        let stats = handler.get_stats().await;
        assert_eq!(stats.messages_sent, 0);
    }

    #[test]
    fn test_protocol_capabilities_serialization() {
        let capabilities = ProtocolCapabilities {
            version: 1,
            compression_enabled: true,
            compression_algorithm: Some("lz4".to_string()),
            encryption_enabled: true,
            encryption_algorithm: Some("aes256gcm".to_string()),
            max_message_size: 1024 * 1024,
            supported_features: vec!["heartbeat".to_string(), "batching".to_string()],
        };

        let serialized = serde_json::to_string(&capabilities).unwrap();
        let deserialized: ProtocolCapabilities = serde_json::from_str(&serialized).unwrap();

        assert_eq!(capabilities.version, deserialized.version);
        assert_eq!(capabilities.compression_enabled, deserialized.compression_enabled);
        assert_eq!(capabilities.max_message_size, deserialized.max_message_size);
    }

    #[test]
    fn test_protocol_stats_serialization() {
        let stats = ProtocolStats {
            version: 1,
            messages_sent: 100,
            messages_received: 95,
            bytes_sent: 10240,
            bytes_received: 9728,
            compression_ratio: 0.75,
            encryption_enabled: true,
            compression_enabled: true,
        };

        let serialized = serde_json::to_string(&stats).unwrap();
        let deserialized: ProtocolStats = serde_json::from_str(&serialized).unwrap();

        assert_eq!(stats.version, deserialized.version);
        assert_eq!(stats.messages_sent, deserialized.messages_sent);
        assert_eq!(stats.encryption_enabled, deserialized.encryption_enabled);
    }
}