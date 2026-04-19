use super::io::sessions_dir;
use crate::common::daemon_client;
use crate::config::CliConfig;
use anyhow::Result;

pub(super) async fn cleanup(config: CliConfig, older_than: u32, dry_run: bool) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions directory found.");
        return Ok(());
    }

    let client = daemon_client().await?;

    let result = client
        .session_cleanup(&config.kiln_path, older_than as u64, dry_run)
        .await?;

    let deleted = result["deleted"].as_array().cloned().unwrap_or_default();
    let total = result["total"].as_u64().unwrap_or(0);
    let is_dry_run = result["dry_run"].as_bool().unwrap_or(false);

    if total == 0 {
        println!("No sessions older than {} days found.", older_than);
        return Ok(());
    }

    println!("Found {} sessions older than {} days:", total, older_than);

    for id in &deleted {
        if let Some(s) = id.as_str() {
            println!("  {}", s);
        }
    }

    if is_dry_run {
        println!("\nDry run - no sessions deleted.");
    } else {
        println!("\nCleanup complete.");
    }

    Ok(())
}
