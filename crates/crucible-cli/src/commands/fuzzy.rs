//! Deprecated fuzzy search command
//!
//! This module provides backwards compatibility for the old fuzzy command.
//! The functionality has been integrated into the unified search command.

use crate::config::CliConfig;
use anyhow::Result;

/// Execute deprecated fuzzy search command
///
/// This function maintains backwards compatibility by forwarding to the unified search command
/// with appropriate parameters and a deprecation warning.
pub async fn execute(
    config: CliConfig,
    query: Option<String>,
    _search_content: bool,
    _search_tags: bool,
    _search_paths: bool,
    limit: u32,
) -> Result<()> {
    // Forward to the new unified search implementation with deprecation warning
    super::search::execute_fuzzy_deprecated(config, query, limit).await
}