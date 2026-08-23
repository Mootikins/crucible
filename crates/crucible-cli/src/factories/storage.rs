//! Storage factory - creates storage implementations
//!
//! Daemon-only: all storage access goes through the daemon via RPC.

use crate::common::daemon_client;
use crate::config::CliConfig;
use anyhow::Result;
use crucible_core::storage::NoteStore;
use crucible_daemon::{DaemonNoteStore, DaemonStorageClient};
use std::sync::Arc;
use tracing::info;

/// Handle for daemon-backed storage.
///
/// Wraps a `DaemonStorageClient` — the daemon is the only storage backend.
#[derive(Clone)]
pub struct CliStorageHandle(Arc<DaemonStorageClient>);

impl CliStorageHandle {
    /// Get the inner `DaemonStorageClient`.
    pub fn as_daemon_client(&self) -> &Arc<DaemonStorageClient> {
        &self.0
    }

    /// List notes in the kiln.
    pub async fn list_notes(
        &self,
        path_filter: Option<&str>,
    ) -> Result<Vec<crucible_core::traits::NoteInfo>> {
        use crucible_core::traits::KnowledgeRepository;

        let repo = Arc::clone(&self.0) as Arc<dyn KnowledgeRepository>;
        repo.list_notes(path_filter)
            .await
            .map_err(|e| anyhow::anyhow!("list_notes failed: {}", e))
    }

    /// Get NoteStore trait object.
    pub fn note_store(&self) -> Arc<dyn NoteStore> {
        Arc::new(DaemonNoteStore::new(Arc::clone(&self.0)))
    }
}

/// What opening the kiln had to do, as the daemon reports it.
///
/// `kiln.open` already returned these counts and every caller threw them away,
/// so a command that spent minutes indexing looked identical to one that spent
/// milliseconds. They are the only thing that can explain the wait to the user.
#[derive(Debug, Default, Clone)]
pub struct KilnOpenSummary {
    pub discovered: u64,
    pub processed: u64,
    pub skipped: u64,
    pub errors: u64,
    /// Set when processing failed as a whole rather than file by file — an
    /// unreachable embedding provider, say. The daemon reports this in place of
    /// the counts, and answers `status: "ok"` while doing so, so a summary that
    /// only read counts called a total failure a no-op and said nothing.
    pub process_error: Option<String>,
}

impl KilnOpenSummary {
    fn from_response(value: &serde_json::Value) -> Self {
        let field = |name: &str| value.get(name).and_then(serde_json::Value::as_u64);
        Self {
            discovered: field("discovered").unwrap_or(0),
            processed: field("processed").unwrap_or(0),
            skipped: field("skipped").unwrap_or(0),
            errors: value
                .get("errors")
                .and_then(serde_json::Value::as_array)
                .map(|e| e.len() as u64)
                .unwrap_or(0),
            process_error: value
                .get("process_error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        }
    }

    /// One line for the user, or `None` when there is nothing worth saying —
    /// an open that reindexed nothing should stay quiet. An open that FAILED
    /// is not that case, and says so.
    pub fn describe(&self) -> Option<String> {
        if let Some(error) = &self.process_error {
            return Some(format!("Kiln: indexing failed: {error}"));
        }
        if self.processed == 0 && self.errors == 0 {
            return None;
        }
        let mut parts = vec![format!("indexed {}", self.processed)];
        if self.skipped > 0 {
            parts.push(format!("{} unchanged", self.skipped));
        }
        if self.errors > 0 {
            parts.push(format!("{} failed", self.errors));
        }
        Some(format!("Kiln: {}", parts.join(", ")))
    }
}

/// Get daemon-backed storage.
///
/// Connects to the daemon (auto-starting if needed), opens the kiln,
/// and returns a `CliStorageHandle` for queries.
pub async fn get_storage(config: &CliConfig) -> Result<CliStorageHandle> {
    Ok(get_storage_with_summary(config).await?.0)
}

/// As [`get_storage`], and also what the open had to index.
///
/// Opening processes pending files, which is unbounded work: a kiln the daemon
/// has never seen is parsed and embedded note by note before this returns. A
/// caller that makes the user wait for it should be able to say why.
pub async fn get_storage_with_summary(
    config: &CliConfig,
) -> Result<(CliStorageHandle, KilnOpenSummary)> {
    info!("Using daemon storage mode");
    let client = daemon_client().await?;
    let kiln_path = config.kiln_path.clone();

    // Open the kiln in the daemon (required before any queries).
    // process=true ensures files are processed on open, replacing the old
    // separate process_files_with_change_detection call.
    let response = client
        .kiln_open_with_options(&kiln_path, true, false)
        .await?;
    let summary = KilnOpenSummary::from_response(&response);
    info!(
        discovered = summary.discovered,
        processed = summary.processed,
        skipped = summary.skipped,
        errors = summary.errors,
        "Kiln opened"
    );

    let client = Arc::new(client);
    Ok((
        CliStorageHandle(Arc::new(DaemonStorageClient::new(client, kiln_path))),
        summary,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The shape `handle_kiln_open` sends when `open_and_process` fails as a
    /// whole. It answers `status: "ok"` and carries no counts, so a summary
    /// that reads only counts sees an open that did nothing.
    #[test]
    fn a_whole_batch_failure_is_reported_not_swallowed() {
        let summary = KilnOpenSummary::from_response(&json!({
            "status": "ok",
            "process_error": "embedding provider unavailable",
        }));

        assert_eq!(
            summary.describe().as_deref(),
            Some("Kiln: indexing failed: embedding provider unavailable")
        );
    }

    #[test]
    fn an_open_that_indexed_nothing_stays_quiet() {
        let summary = KilnOpenSummary::from_response(&json!({
            "status": "ok",
            "discovered": 150,
            "processed": 0,
            "skipped": 150,
            "errors": [],
        }));

        assert_eq!(summary.describe(), None);
    }

    #[test]
    fn counts_and_per_file_errors_are_read_from_the_response() {
        let summary = KilnOpenSummary::from_response(&json!({
            "status": "ok",
            "discovered": 150,
            "processed": 3,
            "skipped": 145,
            "errors": [{"path": "a.md", "error": "boom"}, {"path": "b.md", "error": "boom"}],
        }));

        assert_eq!(summary.discovered, 150);
        assert_eq!(summary.skipped, 145);
        assert_eq!(
            summary.describe().as_deref(),
            Some("Kiln: indexed 3, 145 unchanged, 2 failed")
        );
    }
}
