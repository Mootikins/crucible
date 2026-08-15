use crate::common::daemon_client;
use crate::config::CliConfig;
use anyhow::{Context, Result};

/// Search past sessions via the daemon's `session.search` RPC.
///
/// The daemon is the single search mechanism: it owns the session logs and the
/// scan semantics (case-insensitive, first matching line per session). If it
/// can't be reached or started, this fails like every other daemon-dependent
/// command rather than falling back to a divergent client-side scan.
pub(super) async fn search(
    config: CliConfig,
    query: String,
    limit: u32,
    format: String,
) -> Result<()> {
    let client = daemon_client().await?;
    let result = client
        .session_search(&query, Some(&config.kiln_path), Some(limit as usize))
        .await
        .context("Session search failed")?;

    let matches = result
        .get("matches")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if format == "json" {
        println!("{}", serde_json::json!({"matches": matches}));
    } else if matches.is_empty() {
        println!("No sessions matching '{}' found.", query);
    } else {
        println!("Sessions matching '{}':\n", query);
        for m in &matches {
            let session_id = m["session_id"].as_str().unwrap_or("");
            let line = m["line"].as_u64().unwrap_or(0);
            let context = m["context"].as_str().unwrap_or("");
            println!("  {} (line {})", session_id, line);
            println!("    {}\n", context);
        }
    }
    Ok(())
}
