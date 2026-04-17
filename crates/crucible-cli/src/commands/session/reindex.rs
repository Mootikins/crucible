use super::io::{list_session_dirs, sessions_dir};
use crate::common::daemon_client;
use crate::config::CliConfig;
use anyhow::Result;

pub(super) async fn reindex(config: CliConfig, force: bool) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions directory found.");
        return Ok(());
    }

    let dirs = list_session_dirs(&sessions_path).await?;
    if dirs.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    let client = daemon_client().await?;

    let result = client.session_reindex(&config.kiln_path, force).await?;

    let indexed = result["indexed"].as_u64().unwrap_or(0);
    let skipped = result["skipped"].as_u64().unwrap_or(0);
    let errors = result["errors"].as_u64().unwrap_or(0);

    println!(
        "\nIndexed {} sessions ({} skipped, {} errors)",
        indexed, skipped, errors
    );

    Ok(())
}
