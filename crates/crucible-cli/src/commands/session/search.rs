use crate::common::daemon_client;
use crate::config::CliAppConfig;
use anyhow::{Context, Result};

/// Search past sessions via the daemon's `session.search` RPC.
///
/// The daemon is the single search mechanism: it owns the session logs and the
/// scan semantics (case-insensitive, first matching line per session). If it
/// can't be reached or started, this fails like every other daemon-dependent
/// command rather than falling back to a divergent client-side scan.
pub(super) async fn search(
    config: CliAppConfig,
    query: String,
    limit: u32,
    format: String,
) -> Result<()> {
    let client = daemon_client().await?;
    let result = client
        .session_search(
            &query,
            config.session_kiln_name().as_slice(),
            Some(limit as usize),
        )
        .await
        .context("Session search failed")?;

    if format == "json" {
        println!("{}", serde_json::json!({"matches": result.matches}));
    } else {
        println!("{}", result.to_text(&query));
    }
    Ok(())
}
