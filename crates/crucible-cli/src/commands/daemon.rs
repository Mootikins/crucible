//! Daemon management commands

use crate::config::CliConfig;
use anyhow::Result;
use clap::Subcommand;
use crucible_daemon::{socket_path, Server};
use crucible_rpc::lifecycle::is_daemon_running;
use crucible_rpc::DaemonClient;
use std::path::PathBuf;
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
    /// Internal: run as foreground daemon (used by auto-spawn)
    #[command(hide = true)]
    Serve,
}

pub async fn handle(cmd: DaemonCommands, config_path: Option<PathBuf>) -> Result<()> {
    match cmd {
        DaemonCommands::Start { foreground, wait } => {
            start_daemon(foreground, wait, config_path).await
        }
        DaemonCommands::Stop => stop_daemon().await,
        DaemonCommands::Serve => start_daemon(true, false, config_path).await,
        DaemonCommands::Status => show_status().await,
    }
}

async fn start_daemon(foreground: bool, wait: bool, config_path: Option<PathBuf>) -> Result<()> {
    let sock = socket_path();

    if is_daemon_running(&sock) {
        println!("Daemon is already running");
        return Ok(());
    }

    if foreground {
        // Run server directly in this process
        info!("Starting daemon in foreground");
        let config = CliConfig::load(config_path.clone(), None, None)?;
        let server = Server::bind_with_plugin_config(
            &sock,
            None,
            std::collections::HashMap::new(),
            false,
            None,
            Some(config.acp.clone()),
            None,
            None,
        )
        .await?;

        println!("Daemon listening on {:?}", sock);
        server.run().await?;
    } else {
        // Fork and exec ourselves with --foreground
        // This is the cleanest way to daemonize from a single binary
        let exe = std::env::current_exe()?;

        // Use fork via Command with pre_exec
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(["daemon", "start", "--foreground"]);
        if let Some(path) = config_path {
            cmd.arg("--config").arg(path);
        }

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

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_socket_path_returns_path() {
        let path = socket_path();
        assert!(path.file_name().is_some());
    }
}
