//! Transport trait definitions
//!
//! This module defines the transport interface that all
//! synchronization transport mechanisms must implement.

use async_trait::async_trait;
use std::fmt::Debug;

/// Error type for transport operations
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Not connected")]
    NotConnected,
}

/// Transport trait for sending and receiving CRDT updates
#[async_trait]
pub trait Transport: Send + Sync + Debug {
    /// Send a CRDT update
    async fn send_update(&self, update: Vec<u8>) -> Result<(), TransportError>;

    /// Receive all pending CRDT updates
    async fn receive_updates(&self) -> Result<Vec<Vec<u8>>, TransportError>;

    /// Connect to a remote endpoint
    async fn connect(&mut self, endpoint: &str) -> Result<(), TransportError>;

    /// Check if the transport is connected
    fn is_connected(&self) -> bool;

    /// Get the transport endpoint/address
    fn endpoint(&self) -> Option<&str>;
}
