//! Tests for event system initialization and runtime behavior.

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
async fn test_handler_priorities() {
    use crucible_enrichment::EmbeddingHandler;
    use crucible_surrealdb::event_handlers::{StorageHandler, TagHandler};

    // Verify priority ordering
    assert!(
        StorageHandler::PRIORITY < TagHandler::PRIORITY,
        "StorageHandler should run before TagHandler"
    );
    assert!(
        TagHandler::PRIORITY < EmbeddingHandler::PRIORITY,
        "TagHandler should run before EmbeddingHandler"
    );
}
