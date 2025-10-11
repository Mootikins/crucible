use anyhow::Result;
use rune::{ContextError, Module};
use std::sync::Arc;

/// Build the Crucible standard library module for Rune
///
/// Simplified version - just provides basic logging for now.
/// TODO: Add full db and obsidian modules once we figure out correct Rune 0.13 API
pub fn build_crucible_module(
    _db: Arc<crate::database::EmbeddingDatabase>,
    _obsidian: Arc<crate::obsidian_client::ObsidianClient>,
) -> Result<Module, ContextError> {
    let mut module = Module::with_crate("crucible")?;

    // Simple logging functions (flat namespace for now)
    module.function("log_info", |msg: String| {
        tracing::info!("[Rune] {}", msg);
    }).build()?;

    module.function("log_error", |msg: String| {
        tracing::error!("[Rune] {}", msg);
    }).build()?;

    module.function("log_debug", |msg: String| {
        tracing::debug!("[Rune] {}", msg);
    }).build()?;

    Ok(module)
}
