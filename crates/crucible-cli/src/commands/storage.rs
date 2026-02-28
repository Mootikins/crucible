//! Storage management commands

use anyhow::Result;
use std::future::Future;
use std::time::Instant;

use crate::cli::StorageCommands;
use crate::common::daemon_client;
use crate::config::CliConfig;
use crate::output;
use crucible_rpc::DaemonClient;

/// Execute storage commands
pub async fn execute(config: CliConfig, command: StorageCommands) -> Result<()> {
    match command {
        StorageCommands::Mode => execute_mode(&config).await,
        StorageCommands::Stats { format, .. } => execute_stats(config, format).await,
        StorageCommands::Verify { path, .. } => {
            let kiln = path.unwrap_or_else(|| config.kiln_path.clone());
            rpc_stub("Verifying storage integrity...", |c| async move {
                c.storage_verify(&kiln).await
            })
            .await
        }
        StorageCommands::Cleanup { .. } => {
            let kiln = config.kiln_path.clone();
            rpc_stub("Starting storage cleanup...", |c| async move {
                c.storage_cleanup(&kiln).await
            })
            .await
        }
        StorageCommands::Backup { dest, .. } => {
            let kiln = config.kiln_path.clone();
            rpc_stub(
                &format!("Starting backup to: {}", dest.display()),
                |c| async move { c.storage_backup(&kiln, &dest).await },
            )
            .await
        }
        StorageCommands::Restore { source, .. } => {
            if !source.exists() {
                return Err(anyhow::anyhow!(
                    "Backup file does not exist: {}",
                    source.display()
                ));
            }
            let kiln = config.kiln_path.clone();
            rpc_stub(
                &format!("Starting restore from: {}", source.display()),
                |c| async move { c.storage_restore(&kiln, &source).await },
            )
            .await
        }
    }
}

async fn execute_mode(_config: &CliConfig) -> Result<()> {
    output::header("Storage Mode");
    println!("  Current mode: daemon");
    println!();
    println!("  Description: Client-server mode with shared database");
    println!("  Backend: SQLite daemon process");
    println!("  Use case: Multiple concurrent CLI sessions");
    println!();
    Ok(())
}

/// Connect to daemon and run a storage RPC stub, printing the result message.
async fn rpc_stub<F, Fut>(label: &str, call: F) -> Result<()>
where
    F: FnOnce(DaemonClient) -> Fut,
    Fut: Future<Output = Result<serde_json::Value>>,
{
    output::info(label);
    let client = daemon_client().await?;
    match call(client).await {
        Ok(result) => {
            let msg = result
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Done");
            output::warning(msg);
        }
        Err(e) => output::warning(&format!("Daemon error: {}", e)),
    }
    Ok(())
}

/// Execute storage stats command
async fn execute_stats(config: CliConfig, _format: String) -> Result<()> {
    let start_time = Instant::now();
    output::info("Gathering storage statistics...");

    let storage = crate::factories::get_storage(&config).await?;
    let note_store = storage.note_store();
    let notes = note_store.list().await?;

    output::header("Storage Statistics");
    println!("  Total Notes: {}", notes.len());
    println!("  Storage Mode: daemon");

    let duration = start_time.elapsed();
    output::success(&format!(
        "Stats completed in {:.2}s",
        duration.as_secs_f32()
    ));
    Ok(())
}
