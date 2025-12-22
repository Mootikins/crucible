//! Daemon management commands

use anyhow::Result;
use clap::Subcommand;
use crucible_daemon::{is_daemon_running, pid_path, socket_path, write_pid_file, Server};
use crucible_daemon_client::DaemonClient;
use std::process::Stdio;
use tracing::info;

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Start the daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the daemon
    Stop,
    /// Check daemon status
    Status,
}

pub async fn handle(cmd: DaemonCommands) -> Result<()> {
    match cmd {
        DaemonCommands::Start { foreground } => start_daemon(foreground).await,
        DaemonCommands::Stop => stop_daemon().await,
        DaemonCommands::Status => show_status().await,
    }
}

async fn start_daemon(foreground: bool) -> Result<()> {
    if is_daemon_running() {
        println!("Daemon is already running");
        return Ok(());
    }

    if foreground {
        // Run server directly in this process
        info!("Starting daemon in foreground");
        let path = socket_path();
        let server = Server::bind(&path).await?;

        // Write PID file
        write_pid_file(&pid_path())?;

        println!("Daemon listening on {:?}", path);
        server.run().await?;
    } else {
        // Fork and exec ourselves with --foreground
        // This is the cleanest way to daemonize from a single binary
        let exe = std::env::current_exe()?;

        // Use fork via Command with pre_exec
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(["daemon", "start", "--foreground"]);

        // Daemonize: redirect stdio and detach
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        // Spawn detached
        cmd.spawn()?;

        // Wait for socket to appear
        let sock = socket_path();
        for _ in 0..50 {
            if sock.exists() {
                println!("Daemon started");
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        anyhow::bail!("Daemon failed to start (socket not created)");
    }

    Ok(())
}

async fn stop_daemon() -> Result<()> {
    if !is_daemon_running() {
        println!("Daemon is not running");
        return Ok(());
    }

    match DaemonClient::connect().await {
        Ok(client) => {
            client.shutdown().await?;
            println!("Daemon stopped");
        }
        Err(e) => {
            println!("Failed to connect to daemon: {}", e);
        }
    }

    Ok(())
}

async fn show_status() -> Result<()> {
    if is_daemon_running() {
        match DaemonClient::connect().await {
            Ok(client) => {
                let _ = client.ping().await?;
                println!("Daemon is running");

                // Show open kilns
                let kilns = client.kiln_list().await?;
                if !kilns.is_empty() {
                    println!("\nOpen kilns:");
                    for kiln in kilns {
                        if let Some(path) = kiln.get("path").and_then(|v| v.as_str()) {
                            let secs = kiln
                                .get("last_access_secs_ago")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            println!("  {} (last access: {}s ago)", path, secs);
                        }
                    }
                }
            }
            Err(_) => {
                println!("Daemon PID file exists but cannot connect");
            }
        }
    } else {
        println!("Daemon is not running");
    }

    Ok(())
}

/// Ensure daemon is running, starting it if necessary
pub async fn ensure_daemon() -> Result<DaemonClient> {
    if !is_daemon_running() {
        // Start daemon in background
        start_daemon(false).await?;
    }

    // Connect with retry
    for attempt in 0..10 {
        match DaemonClient::connect().await {
            Ok(client) => return Ok(client),
            Err(_) if attempt < 9 => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Err(e) => return Err(e),
        }
    }

    anyhow::bail!("Failed to connect to daemon after starting")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_commands_parse() {
        // Basic smoke test that the commands are defined correctly
        // Actual integration tests would need a running daemon
    }
}
