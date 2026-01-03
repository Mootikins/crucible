//! Tests for event system initialization and runtime behavior.
//!
//! Note: StorageHandler and TagHandler were removed in Phase 4 cleanup.
//! The event system now uses simpler handlers that don't depend on EAV storage.

use super::initialization::initialize_event_system;

/// Create a minimal test configuration.
fn create_test_config(kiln_path: std::path::PathBuf) -> crate::config::CliConfig {
    crate::config::CliConfig {
        kiln_path,
        ..Default::default()
    }
}

// Note: Full integration tests that require database initialization
// are in tests/event_system_integration.rs and marked #[ignore]

#[tokio::test]
async fn test_embedding_handler_priority() {
    use crucible_enrichment::EmbeddingHandler;

    // Verify EmbeddingHandler has a reasonable priority
    const _: () = assert!(EmbeddingHandler::PRIORITY > 0);
}
