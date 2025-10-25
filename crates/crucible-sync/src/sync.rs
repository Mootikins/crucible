//! Main synchronization orchestrator
//!
//! This module provides the main SyncInstance that coordinates
//! document operations, transport, and synchronization.

use crate::{Document, SyncError, SyncResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::transport::{Transport, MemoryTransport};

/// A sync instance that manages document synchronization
pub struct SyncInstance {
    document: Document,
    #[allow(dead_code)]
    transport: Arc<RwLock<dyn Transport>>,
    peers: Arc<RwLock<HashMap<String, Arc<dyn Transport>>>>,
}

impl SyncInstance {
    /// Create a new sync instance with in-memory transport
    pub async fn new(document_id: impl Into<String>) -> SyncResult<Self> {
        let document = Document::new(document_id);
        let transport: Arc<RwLock<dyn Transport>> = Arc::new(RwLock::new(MemoryTransport::new()));

        Ok(Self {
            document,
            transport,
            peers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get the document ID
    pub fn id(&self) -> &str {
        self.document.id()
    }

    /// Insert text into the document
    pub async fn insert_text(&self, index: u32, text: &str) -> SyncResult<()> {
        self.document.insert_text(index, text).await?;
        self.broadcast_update().await?;
        Ok(())
    }

    /// Delete text from the document
    pub async fn delete_text(&self, index: u32, length: u32) -> SyncResult<()> {
        self.document.delete_text(index, length).await?;
        self.broadcast_update().await?;
        Ok(())
    }

    /// Get the current document content
    pub async fn get_text(&self) -> String {
        self.document.get_content().await
    }

    /// Sync with another instance (direct connection)
    pub async fn sync_with(&self, other: &SyncInstance) -> SyncResult<()> {
        // Get our current state vector
        let our_sv = self.document.get_state_vector().await;

        // Get updates from other instance
        let their_updates = other.document.get_updates_since(our_sv).await?;

        // Apply their updates
        for update in their_updates {
            self.document.apply_update(update).await?;
        }

        // Get their state vector
        let their_sv = other.document.get_state_vector().await;

        // Get our updates they don't have
        let our_updates = self.document.get_updates_since(their_sv).await?;

        // Apply our updates to them
        for update in our_updates {
            other.document.apply_update(update).await?;
        }

        Ok(())
    }

    /// Apply changes using a closure for batch operations
    pub async fn apply_change<F, R>(&self, f: F) -> SyncResult<R>
    where
        F: FnOnce(&Document) -> R,
    {
        let result = f(&self.document);
        self.broadcast_update().await?;
        Ok(result)
    }

    /// Sync with server
    pub async fn sync_with_server(&self) -> SyncResult<()> {
        // This will be implemented when we add server support
        // For now, it's a no-op
        Ok(())
    }

    /// Broadcast current state to all connected peers
    async fn broadcast_update(&self) -> SyncResult<()> {
        // Get all updates since empty state vector (full sync)
        let updates = self.document.get_updates_since(vec![]).await?;

        // Send to all peers
        let peers = self.peers.read().await;
        for (_, peer_transport) in peers.iter() {
            for update in &updates {
                peer_transport.send_update(update.clone()).await
                    .map_err(|e| SyncError::Transport(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Add a peer for synchronization
    pub async fn add_peer(&self, peer_id: &str, transport: Arc<dyn Transport>) {
        let mut peers = self.peers.write().await;
        peers.insert(peer_id.to_string(), transport);
    }

    /// Remove a peer
    pub async fn remove_peer(&self, peer_id: &str) {
        let mut peers = self.peers.write().await;
        peers.remove(peer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_sync_instance_creation() {
        let sync = SyncInstance::new("test-doc").await.unwrap();
        assert_eq!(sync.id(), "test-doc");
        assert_eq!(sync.get_text().await, "");
    }

    #[tokio::test]
    async fn test_basic_text_operations() {
        let sync = SyncInstance::new("test-doc").await.unwrap();

        sync.insert_text(0, "Hello").await.unwrap();
        assert_eq!(sync.get_text().await, "Hello");

        sync.insert_text(5, ", World").await.unwrap();
        assert_eq!(sync.get_text().await, "Hello, World");

        sync.delete_text(5, 2).await.unwrap(); // Delete ", "
        assert_eq!(sync.get_text().await, "HelloWorld");
    }

    #[tokio::test]
    async fn test_sync_with_instance() -> SyncResult<()> {
        // Arrange: Two sync instances with same document
        let sync_a = SyncInstance::new("doc1").await?;
        let sync_b = SyncInstance::new("doc1").await?;

        // Act: Make change in instance A
        sync_a.insert_text(0, "Hello World").await?;

        // Sync A -> B
        sync_a.sync_with(&sync_b).await?;

        // Assert: B should have the change
        assert_eq!(sync_b.get_text().await, "Hello World");

        Ok(())
    }

    #[tokio::test]
    async fn test_bidirectional_sync() -> SyncResult<()> {
        // Arrange: Two sync instances
        let sync_a = SyncInstance::new("doc1").await?;
        let sync_b = SyncInstance::new("doc1").await?;

        // Act: Both instances make changes
        sync_a.insert_text(0, "Hello").await?;
        sync_b.insert_text(5, ", World").await?;

        // Sync in both directions
        sync_a.sync_with(&sync_b).await?;
        sync_b.sync_with(&sync_a).await?;

        // Assert: Both should have merged content
        let content_a = sync_a.get_text().await;
        let content_b = sync_b.get_text().await;

        assert_eq!(content_a, content_b);
        assert!(content_a.contains("Hello"));
        assert!(content_a.contains("World"));

        Ok(())
    }

    #[tokio::test]
    async fn test_apply_change() -> SyncResult<()> {
        let sync = SyncInstance::new("test-doc").await?;

        let result = sync.apply_change(|doc| {
            // This would normally modify the document
            doc.id().to_string()
        }).await?;

        assert_eq!(result, "test-doc");

        Ok(())
    }
}