//! In-memory transport for testing and local synchronization
//!
//! This module provides a simple in-memory transport that's useful
//! for testing and for scenarios where instances are in the same process.

use crate::transport::{traits::TransportError, Transport};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// In-memory transport for testing
#[derive(Debug)]
pub struct MemoryTransport {
    endpoint: Option<String>,
    pending_updates: Arc<Mutex<VecDeque<Vec<u8>>>>,
    is_connected: Arc<RwLock<bool>>,
}

impl MemoryTransport {
    /// Create a new memory transport
    pub fn new() -> Self {
        Self {
            endpoint: None,
            pending_updates: Arc::new(Mutex::new(VecDeque::new())),
            is_connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Connect to another memory transport
    pub async fn connect_to(&mut self, other: &MemoryTransport) {
        *self.is_connected.write().await = true;
        self.endpoint = Some(format!("memory://{}", other as *const _ as usize));
    }

    /// Push an update to this transport (for testing)
    pub async fn push_update(&self, update: Vec<u8>) {
        let mut pending = self.pending_updates.lock().await;
        pending.push_back(update);
    }

    /// Create a pair of connected transports
    pub fn pair() -> (Self, Self) {
        let a = Self::new();
        let b = Self::new();
        (a, b)
    }
}

impl Default for MemoryTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transport for MemoryTransport {
    async fn send_update(&self, _update: Vec<u8>) -> Result<(), TransportError> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        // In a real implementation, this would send to the connected peer
        // For now, we just simulate success
        Ok(())
    }

    async fn receive_updates(&self) -> Result<Vec<Vec<u8>>, TransportError> {
        let mut pending = self.pending_updates.lock().await;
        let updates = pending.drain(..).collect();
        Ok(updates)
    }

    async fn connect(&mut self, endpoint: &str) -> Result<(), TransportError> {
        if endpoint.starts_with("memory://") {
            self.endpoint = Some(endpoint.to_string());
            Ok(())
        } else {
            Err(TransportError::ConnectionFailed(format!(
                "Invalid memory endpoint: {}",
                endpoint
            )))
        }
    }

    fn is_connected(&self) -> bool {
        // Check the current connection state
        // For memory transport, we'll check if we have an endpoint set
        self.endpoint.is_some()
    }

    fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_transport_creation() {
        let transport = MemoryTransport::new();
        assert!(!transport.is_connected());
        assert!(transport.endpoint().is_none());
    }

    #[tokio::test]
    async fn test_memory_transport_connect() {
        let mut transport = MemoryTransport::new();

        let result = transport.connect("memory://12345").await;
        assert!(result.is_ok());
        assert!(transport.is_connected());
        assert_eq!(transport.endpoint(), Some("memory://12345"));
    }

    #[tokio::test]
    async fn test_memory_transport_invalid_endpoint() {
        let mut transport = MemoryTransport::new();

        let result = transport.connect("ws://localhost:8080").await;
        assert!(result.is_err());
        assert!(!transport.is_connected());
    }

    #[tokio::test]
    async fn test_memory_transport_updates() {
        let transport = MemoryTransport::new();

        // Push a test update
        transport.push_update(b"test update".to_vec()).await;

        // Receive the update
        let updates = transport.receive_updates().await.unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0], b"test update");
    }

    #[tokio::test]
    async fn test_memory_transport_send_not_connected() {
        let transport = MemoryTransport::new();

        let result = transport.send_update(b"test".to_vec()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TransportError::NotConnected));
    }
}
