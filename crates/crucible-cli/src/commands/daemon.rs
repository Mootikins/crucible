//! Daemon management commands

use crate::config::CliConfig;
use anyhow::Result;
use clap::Subcommand;
use crucible_daemon::rpc_client::lifecycle::is_daemon_running;
use crucible_daemon::DaemonClient;
use crucible_daemon::{socket_path, BindWithPluginConfigParams, Server};
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
    /// Restart the daemon (stop if running, then start)
    Restart {
        /// Wait for daemon to be ready
        #[arg(long)]
        wait: bool,
    },
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
        DaemonCommands::Restart { wait: _ } => restart_daemon(config_path).await,
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
        let server = Server::bind_with_plugin_config(BindWithPluginConfigParams {
            path: sock.clone(),
            mcp_config: None,
            plugin_config: std::collections::HashMap::new(),
            plugin_watch: false,
            auto_archive_hours: config.server.as_ref().and_then(|s| s.auto_archive_hours),
            llm_config: Some(config.llm.clone()),
            enrichment_config: config.enrichment.as_ref().map(|e| e.provider.clone()),
            max_precognition_chars: config
                .enrichment
                .as_ref()
                .map(|e| e.pipeline.max_precognition_chars)
                .unwrap_or_else(crucible_config::default_max_precognition_chars),
            acp_config: Some(config.acp.clone()),
            permission_config: config.permissions.clone(),
            web_config: None,
        })
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

async fn restart_daemon(config_path: Option<PathBuf>) -> Result<()> {
    let sock = socket_path();
    if is_daemon_running(&sock) {
        // Stop the existing daemon
        match DaemonClient::connect().await {
            Ok(client) => {
                let _ = client.shutdown().await;
                println!("Stopping daemon...");
                // Wait for daemon to release socket
                for _ in 0..50 {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    if !is_daemon_running(&sock) {
                        break;
                    }
                }
            }
            Err(e) => {
                println!("Warning: couldn't connect to stop daemon: {e}");
            }
        }
    }

    // Start fresh daemon
    start_daemon(false, true, config_path).await?;
    println!("Daemon restarted");
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
