//! Factory for creating FileWatcher implementations
//!
//! This factory follows the Dependency Inversion Principle by returning
//! trait objects, allowing commands to depend on abstractions rather than
//! concrete types.

use crate::config::CliConfig;
use anyhow::Result;
use crucible_watch::FileWatcher;
use crucible_watch::NotifyWatcher;
use std::sync::Arc;

/// Create a FileWatcher implementation
///
/// Returns an Arc<dyn FileWatcher> to enable dependency injection.
/// The concrete type (NotifyWatcher) is hidden behind the trait.
///
/// # Arguments
///
/// * `_config` - CLI configuration (reserved for future configuration of the watcher)
///
/// # Returns
///
/// A trait object representing a FileWatcher instance
pub fn create_file_watcher(_config: &CliConfig) -> Result<Arc<dyn FileWatcher>> {
    let watcher = NotifyWatcher::new();
    Ok(Arc::new(watcher))
}
