//! Tests for StorageHandle::note_store() accessor
//!
//! Verifies that StorageHandle correctly exposes the NoteStore trait
//! for different storage modes.

use crucible_cli::config::CliConfig;
use crucible_cli::factories::{get_storage, StorageHandle};
use crucible_core::storage::NoteStore;
use tempfile::TempDir;

#[tokio::test]
#[cfg_attr(
    not(feature = "storage-sqlite"),
    ignore = "requires storage-sqlite feature"
)]
async fn test_storage_handle_note_store_embedded() {
    let temp = TempDir::new().unwrap();
    let config = CliConfig {
        kiln_path: temp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = get_storage(&config).await.unwrap();

    // Should be able to get NoteStore
    let note_store = storage.note_store();
    assert!(
        note_store.is_some(),
        "Embedded mode should provide NoteStore"
    );

    // Should work for queries
    let result = note_store.unwrap().list().await;
    assert!(result.is_ok(), "list() should succeed: {:?}", result.err());
}

#[tokio::test]
async fn test_storage_handle_note_store_lightweight() {
    use crucible_config::StorageMode;

    let temp = TempDir::new().unwrap();
    let mut config = CliConfig {
        kiln_path: temp.path().to_path_buf(),
        ..Default::default()
    };

    // Set lightweight mode
    config.storage = Some(crucible_config::StorageConfig {
        mode: StorageMode::Lightweight,
        ..Default::default()
    });

    let storage = get_storage(&config).await.unwrap();

    // Lightweight mode should also provide NoteStore (via LanceNoteStore)
    let note_store = storage.note_store();
    assert!(
        note_store.is_some(),
        "Lightweight mode should provide NoteStore"
    );

    // Should work for queries
    let result = note_store.unwrap().list().await;
    assert!(result.is_ok(), "list() should succeed: {:?}", result.err());
}

#[test]
fn test_storage_handle_has_note_store_method() {
    // Compile-time check that the method exists with correct signature
    fn _assert_note_store_method(handle: &StorageHandle) {
        let _: Option<std::sync::Arc<dyn NoteStore>> = handle.note_store();
    }
}
