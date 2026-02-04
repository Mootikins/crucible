//! Daemon management commands

use anyhow::Result;
use clap::Subcommand;
use crucible_daemon::{socket_path, Server};
use crucible_rpc::lifecycle::is_daemon_running;
use crucible_rpc::DaemonClient;
use std::process::Stdio;
use tracing::info;

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Start the daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
        /// Wait for daemon to be ready
        #[arg(long)]
        wait: bool,
    },
    /// Stop the daemon
    Stop,
    /// Check daemon status
    Status,
}

pub async fn handle(cmd: DaemonCommands) -> Result<()> {
    match cmd {
        DaemonCommands::Start { foreground, wait } => start_daemon(foreground, wait).await,
        DaemonCommands::Stop => stop_daemon().await,
        DaemonCommands::Status => show_status().await,
    }
}

async fn start_daemon(foreground: bool, wait: bool) -> Result<()> {
    let sock = socket_path();

    if is_daemon_running(&sock) {
        println!("Daemon is already running");
        return Ok(());
    }

    if foreground {
        // Run server directly in this process
        info!("Starting daemon in foreground");
        let server = Server::bind(&sock, None).await?;

        println!("Daemon listening on {:?}", sock);
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

        if wait {
            // Poll until daemon responds
            for _ in 0..50 {
                if let Ok(client) = DaemonClient::connect().await {
                    if client.ping().await.is_ok() {
                        println!("Daemon started");
                        return Ok(());
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            anyhow::bail!("Daemon failed to start within 5 seconds");
        } else {
            // Just wait for socket to appear
            for _ in 0..50 {
                if sock.exists() {
                    println!("Daemon starting...");
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            anyhow::bail!("Daemon failed to start (socket not created)");
        }
    }

    Ok(())
}

async fn stop_daemon() -> Result<()> {
    let sock = socket_path();
    if !is_daemon_running(&sock) {
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
    let sock = socket_path();
    if is_daemon_running(&sock) {
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
                println!("Daemon socket exists but cannot connect");
            }
        }
    } else {
        println!("Daemon is not running");
    }

    Ok(())
}

/// Ensure daemon is running, starting it if necessary
pub async fn ensure_daemon() -> Result<DaemonClient> {
    let sock = socket_path();
    if !is_daemon_running(&sock) {
        // Start daemon in background and wait for it to be ready
        start_daemon(false, true).await?;
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

    #[test]
    fn test_daemon_commands_start_default() {
        let cmd = DaemonCommands::Start {
            foreground: false,
            wait: false,
        };
        matches!(cmd, DaemonCommands::Start { .. });
    }

    #[test]
    fn test_daemon_commands_start_foreground() {
        let cmd = DaemonCommands::Start {
            foreground: true,
            wait: false,
        };
        if let DaemonCommands::Start { foreground, wait } = cmd {
            assert!(foreground);
            assert!(!wait);
        } else {
            panic!("Expected Start variant");
        }
    }

    #[test]
    fn test_daemon_commands_start_wait() {
        let cmd = DaemonCommands::Start {
            foreground: false,
            wait: true,
        };
        if let DaemonCommands::Start { foreground, wait } = cmd {
            assert!(!foreground);
            assert!(wait);
        } else {
            panic!("Expected Start variant");
        }
    }

    #[test]
    fn test_daemon_commands_stop() {
        let cmd = DaemonCommands::Stop;
        matches!(cmd, DaemonCommands::Stop);
    }

    #[test]
    fn test_daemon_commands_status() {
        let cmd = DaemonCommands::Status;
        matches!(cmd, DaemonCommands::Status);
    }

    #[test]
    fn test_socket_path_returns_path() {
        let path = socket_path();
        assert!(path.file_name().is_some());
    }
}
