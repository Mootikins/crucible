//! Integration test for `SurrealClientHandle::as_note_store()`
//!
//! This test validates that the adapters module correctly exposes NoteStore
//! functionality through the opaque client handle.

use crucible_core::storage::NoteStore;
use crucible_surrealdb::{adapters, SurrealDbConfig};

#[tokio::test]
async fn test_as_note_store_returns_valid_impl() {
    let config = SurrealDbConfig {
        path: "memory".to_string(),
        namespace: "test".to_string(),
        database: "adapter_test".to_string(),
        max_connections: Some(1),
        timeout_seconds: Some(5),
    };

    let client = adapters::create_surreal_client(config).await.unwrap();
    let note_store = client.as_note_store();

    // Should be able to call NoteStore methods
    let result = note_store.list().await;
    assert!(result.is_ok());
}
