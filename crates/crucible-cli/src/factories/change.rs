//! Change detection factory - creates in-memory change detector
//!
//! TODO: Add SurrealDB-backed version for persistence across restarts

use std::sync::Arc;
use crucible_core::processing::{ChangeDetectionStore, InMemoryChangeDetectionStore};

/// Create in-memory change detection store
///
/// Note: This loses state across restarts. Consider implementing
/// SurrealDB-backed version for production use.
pub fn create_inmemory_change_detector() -> Arc<dyn ChangeDetectionStore> {
    Arc::new(InMemoryChangeDetectionStore::new())
}
