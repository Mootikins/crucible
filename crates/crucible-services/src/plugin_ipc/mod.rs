//! # Crucible Plugin IPC Protocol
//!
//! This module provides a comprehensive Inter-Process Communication (IPC) protocol
//! for process-based plugin communication in the Crucible knowledge management system.
//!
//! ## Features
//!
//! - **Process Isolation**: Each plugin runs in its own isolated process
//! - **High Performance**: Low-latency binary communication over Unix domain sockets
//! - **Type Safety**: Strong typing with protobuf serialization
//! - **Security**: JWT authentication, TLS encryption, capability-based authorization
//! - **Observability**: Built-in metrics, logging, and distributed tracing
//! - **Error Handling**: Comprehensive error recovery and circuit breaking
//! - **Scalability**: Connection pooling and multiplexing

pub mod protocol;
pub mod message;
pub mod transport;
pub mod security;
pub mod client;
pub mod server;
pub mod error;
pub mod metrics;
pub mod config;

// Re-export main types for convenience
pub use protocol::*;
pub use message::*;
pub use transport::*;
pub use security::*;
pub use client::*;
pub use server::*;
pub use error::*;
pub use metrics::*;
pub use config::*;

/// IPC Protocol version
pub const PROTOCOL_VERSION: u8 = 1;

/// Default transport configuration
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/crucible-plugins";
pub const DEFAULT_PORT_RANGE: std::ops::Range<u16> = 9000..10000;

/// Message size limits
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16MB
pub const MAX_PAYLOAD_SIZE: usize = MAX_MESSAGE_SIZE - 1024; // Reserve space for headers

/// Default timeouts
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5000;
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30000;
pub const DEFAULT_HEARTBEAT_INTERVAL_MS: u64 = 10000;
pub const DEFAULT_IDLE_TIMEOUT_MS: u64 = 60000;

/// Security constants
pub const DEFAULT_TOKEN_EXPIRY_MS: u64 = 3600000; // 1 hour
pub const MAX_TOKEN_EXPIRY_MS: u64 = 86400000; // 24 hours
pub const DEFAULT_CIPHER_SUITE: &str = "TLS_AES_256_GCM_SHA384";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_constants() {
        assert_eq!(PROTOCOL_VERSION, 1);
        assert!(MAX_MESSAGE_SIZE > MAX_PAYLOAD_SIZE);
        assert!(DEFAULT_PORT_RANGE.start < DEFAULT_PORT_RANGE.end);
    }

    #[test]
    fn test_timeout_values() {
        assert!(DEFAULT_CONNECT_TIMEOUT_MS < DEFAULT_REQUEST_TIMEOUT_MS);
        assert!(DEFAULT_REQUEST_TIMEOUT_MS > DEFAULT_HEARTBEAT_INTERVAL_MS);
        assert!(DEFAULT_IDLE_TIMEOUT_MS > DEFAULT_HEARTBEAT_INTERVAL_MS);
    }

    #[test]
    fn test_security_constants() {
        assert!(DEFAULT_TOKEN_EXPIRY_MS < MAX_TOKEN_EXPIRY_MS);
        assert!(!DEFAULT_CIPHER_SUITE.is_empty());
    }
}