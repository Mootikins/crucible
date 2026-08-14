//! Daemon management commands

use crate::config::CliConfig;
use anyhow::Result;
use clap::Subcommand;
use crucible_daemon::rpc_client::lifecycle::is_daemon_running;
use crucible_daemon::DaemonClient;
use crucible_daemon::{socket_path, BindWithPluginConfigParams, Server};
use serde::Serialize;
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
    /// Restart the daemon (stop if running, start, wait until it answers)
    Restart,
    /// Check daemon status
    Status {
        /// Emit status as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show recent output from the background daemon
    Logs {
        /// Number of lines to show from the end of the log
        #[arg(short = 'n', long, default_value_t = 50)]
        lines: usize,
    },
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
        DaemonCommands::Restart => restart_daemon(config_path).await,
        DaemonCommands::Serve => start_daemon(true, false, config_path).await,
        DaemonCommands::Status { json } => show_status(json).await,
        DaemonCommands::Logs { lines } => show_logs(lines),
    }
}

fn show_logs(lines: usize) -> Result<()> {
    let path = crucible_daemon::rpc_client::lifecycle::daemon_log_path();
    match crucible_daemon::rpc_client::lifecycle::read_log_tail(&path, lines) {
        Some(tail) => println!("{tail}"),
        None => println!(
            "No daemon output at {} yet — it appears after the first background daemon start.",
            path.display()
        ),
    }
    Ok(())
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

        // Before anything reads the runtimepath. An installed `cru` has no
        // runtime tree next to it — no packaging route puts one there — so
        // plugins, themes and the bundled help skills would all resolve to
        // nothing. This is the one place that writes the compiled-in copy out;
        // it no-ops when a real tree already answers.
        if let Some(root) = crucible_core::runtime_roots::ensure_bundled_runtime() {
            info!(root = %root.display(), "extracted the bundled runtime tree");
        }

        // The help corpus, for the same packaging reason. It is a kiln, not a
        // bespoke index — `docs/` already is one — and it is never mounted on
        // its own: connect it when you want to ask Crucible about itself.
        //
        // Off the startup path. Nothing at boot reads it — the kiln is lazy, so
        // the bytes are wanted only if someone connects it — and writing 84
        // files before binding the socket delays every daemon start, including
        // the several hundred a test run spawns.
        tokio::spawn(async {
            if let Some(docs) =
                tokio::task::spawn_blocking(crucible_core::bundled_docs::ensure_bundled_docs)
                    .await
                    .ok()
                    .flatten()
            {
                info!(
                    docs = %docs.display(),
                    "extracted the bundled help corpus; connect it as a kiln to search it"
                );
            }
        });

        let config = CliConfig::load(config_path.clone(), None, None)?;
        let (plugin_sections, plugin_watch) =
            crucible_daemon::daemon_plugins::split_plugins_config(&config.plugins);
        let server = Server::bind_with_plugin_config(BindWithPluginConfigParams {
            path: sock.clone(),
            mcp_config: None,
            plugin_config: plugin_sections.clone(),
            runtimepath: config.runtimepath.clone(),
            plugin_watch,
            auto_archive_hours: config.server.as_ref().and_then(|s| s.auto_archive_hours),
            llm_config: Some(config.llm.clone()),
            enrichment_config: config.enrichment.as_ref().map(|e| e.provider.clone()),
            max_precognition_chars: config
                .enrichment
                .as_ref()
                .map(|e| e.pipeline.max_precognition_chars)
                .unwrap_or_else(crucible_core::config::default_max_precognition_chars),
            acp_config: Some(config.acp.clone()),
            context_config: config.context.clone(),
            permission_config: config.permissions.clone(),
            web_config: None,
            schedules: config.schedules.clone(),
            app_config: serde_json::to_value(&config).ok(),
            data_home: config.data_home.clone(),
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

        // Daemonize: detach stdin, capture output in the daemon log so a
        // startup crash has somewhere to leave its cause.
        let (out, err) = crucible_daemon::rpc_client::lifecycle::daemon_log_stdio();
        cmd.stdin(Stdio::null());
        cmd.stdout(out);
        cmd.stderr(err);

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

    // Restart always waits for a ping: the no-wait path returns as soon as
    // the socket *file* exists, and a leftover socket from the old daemon
    // makes that instant — "Daemon restarted" would print before the
    // replacement had bound anything.
    start_daemon(false, /* wait */ true, config_path).await?;
    println!("Daemon restarted");
    Ok(())
}

#[derive(Serialize)]
struct KilnStatus {
    path: String,
    last_access_secs_ago: u64,
}

#[derive(Serialize)]
struct DaemonStatus {
    /// "running", "unreachable" (socket present but connect failed), or "stopped"
    state: &'static str,
    socket: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    kilns: Vec<KilnStatus>,
}

async fn show_status(json: bool) -> Result<()> {
    let sock = socket_path();
    let sock_str = sock.display().to_string();

    let status = if is_daemon_running(&sock) {
        match DaemonClient::connect().await {
            Ok(client) => {
                client.ping().await?;
                let kilns = client
                    .kiln_list()
                    .await?
                    .into_iter()
                    .filter_map(|k| {
                        let path = k.get("path")?.as_str()?.to_string();
                        let last_access_secs_ago = k
                            .get("last_access_secs_ago")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        Some(KilnStatus {
                            path,
                            last_access_secs_ago,
                        })
                    })
                    .collect();
                DaemonStatus {
                    state: "running",
                    socket: sock_str,
                    kilns,
                }
            }
            Err(_) => DaemonStatus {
                state: "unreachable",
                socket: sock_str,
                kilns: Vec::new(),
            },
        }
    } else {
        DaemonStatus {
            state: "stopped",
            socket: sock_str,
            kilns: Vec::new(),
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        match status.state {
            "running" => {
                println!("Daemon is running");
                if !status.kilns.is_empty() {
                    println!("\nOpen kilns:");
                    for k in &status.kilns {
                        println!(
                            "  {} (last access: {}s ago)",
                            k.path, k.last_access_secs_ago
                        );
                    }
                }
            }
            "unreachable" => println!("Daemon socket exists but cannot connect"),
            _ => println!("Daemon is not running"),
        }
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
