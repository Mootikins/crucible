use anyhow::Result;
use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::prelude::*; // For SubscriberExt trait

use crucible_cli::{
    cli::{Cli, Commands},
    commands, config,
};

/// Parse log level string to LevelFilter
fn parse_log_level(level: &str) -> Option<LevelFilter> {
    match level.to_lowercase().as_str() {
        "off" => Some(LevelFilter::OFF),
        "error" => Some(LevelFilter::ERROR),
        "warn" => Some(LevelFilter::WARN),
        "info" => Some(LevelFilter::INFO),
        "debug" => Some(LevelFilter::DEBUG),
        "trace" => Some(LevelFilter::TRACE),
        _ => None,
    }
}

/// Cleans up the standalone daemon socket on drop.
struct SocketCleanup(std::path::PathBuf);
impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn main() -> Result<()> {
    // Parse CLI before entering the async runtime so we can set env vars safely.
    let cli = Cli::parse();

    // Standalone mode: configure the socket path BEFORE spawning any threads.
    // set_var is not thread-safe, so it must happen before the tokio runtime starts.
    let standalone_sock = if cli.standalone {
        let pid = std::process::id();
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        let sock = runtime_dir.join(format!("crucible-standalone-{}.sock", pid));
        let _ = std::fs::remove_file(&sock);
        std::env::set_var("CRUCIBLE_SOCKET", &sock);
        Some(sock)
    } else {
        None
    };

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main(cli, standalone_sock))
}

async fn async_main(cli: Cli, standalone_sock: Option<std::path::PathBuf>) -> Result<()> {
    // Install ring as the rustls CryptoProvider before any TLS usage.
    // Both ring and aws-lc-rs are compiled (via lancedb), so rustls can't auto-detect.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let config = config::CliConfig::load(
        cli.config.clone(),
        cli.embedding_url.clone(),
        cli.embedding_model.clone(),
    )?;

    // Standalone mode: start the in-process daemon on the pre-configured socket.
    let _standalone_guard = if let Some(sock) = standalone_sock {
        let server = crucible_daemon::Server::bind_with_plugin_config(
            &sock,
            None,
            std::collections::HashMap::new(),
            false,
            Some(config.llm.clone()),
            Some(config.acp.clone()),
            None,
            None,
        )
        .await?;
        info!("Standalone daemon listening on {:?}", sock);
        tokio::spawn(async move {
            if let Err(e) = server.run().await {
                error!("Standalone daemon error: {}", e);
            }
        });
        for _ in 0..50 {
            if sock.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        if !sock.exists() {
            anyhow::bail!("Standalone daemon failed to start (socket not created after 500ms)");
        }
        Some(SocketCleanup(sock))
    } else {
        None
    };

    // Check if the command uses stdio for communication (needs file logging)
    // MCP and Chat use stdio (stdin/stdout) for JSON-RPC, so we must avoid stderr output
    let uses_stdio = match &cli.command {
        // None defaults to chat mode
        Some(Commands::Mcp { stdio, .. }) => *stdio,
        Some(Commands::Chat { .. }) | None => true,
        _ => false,
    };

    // Determine base log level from CLI flags or config
    // Priority: --log-level flag > --verbose flag > config file > default
    // Default: WARN for stdio commands (always log errors/warnings), OFF for others
    let base_level: LevelFilter = if let Some(level) = cli.log_level {
        level.into()
    } else if cli.verbose {
        LevelFilter::DEBUG
    } else if let Some(config_level) = config.logging_level() {
        parse_log_level(&config_level).unwrap_or(if uses_stdio {
            LevelFilter::WARN
        } else {
            LevelFilter::OFF
        })
    } else if uses_stdio {
        LevelFilter::WARN // Default to WARN for chat/mcp (always capture errors)
    } else {
        LevelFilter::OFF
    };

    // Build env filter: RUST_LOG overrides, with base_level as fallback
    // This allows: RUST_LOG=crucible_daemon=info,crucible_cli=debug cargo run -- chat
    let env_filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(base_level.into())
        .from_env_lossy();

    // Initialize logging based on command type
    if base_level != LevelFilter::OFF || std::env::var("RUST_LOG").is_ok() {
        if uses_stdio {
            // File-only logging for stdio-based commands (MCP, Chat)
            // Default to ~/.crucible/<command>.log, override with CRUCIBLE_LOG_FILE
            let log_file_name = match &cli.command {
                Some(Commands::Mcp { .. }) => "mcp.log",
                Some(Commands::Chat { .. }) | None => "chat.log",
                _ => "crucible.log",
            };

            let log_file_path = std::env::var("CRUCIBLE_LOG_FILE").unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                    .join(".crucible")
                    .join(log_file_name)
                    .to_string_lossy()
                    .to_string()
            });

            // Create parent directory if it doesn't exist
            if let Some(parent) = std::path::Path::new(&log_file_path).parent() {
                std::fs::create_dir_all(parent)?;
            }

            let log_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file_path)?;

            let file_layer = tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Arc::new(log_file))
                .with_ansi(false) // No ANSI codes in log files
                .with_target(true)
                .with_thread_ids(true);

            tracing_subscriber::registry()
                .with(file_layer)
                .with(env_filter)
                .init();
        } else {
            // Normal stderr logging for other commands
            tracing_subscriber::fmt().with_env_filter(env_filter).init();
        }
    }

    // Log configuration in verbose mode
    if cli.verbose {
        config.log_config();
    }

    // Execute command
    let cli_config_path = cli.config.clone();
    match cli.command {
        // New ACP-based commands
        Some(Commands::Chat {
            query,
            agent,
            resume,
            env,
            provider,
            max_context,
            no_context,
            context_size,
            plan,
            set_overrides,
            record,
            replay,
            replay_speed,
            replay_auto_exit,
        }) => {
            commands::chat::execute(
                config,
                agent,
                query,
                plan,
                no_context,
                Some(context_size),
                provider,
                max_context,
                env,
                resume,
                set_overrides,
                record,
                replay,
                replay_speed,
                replay_auto_exit,
            )
            .await?
        }

        Some(Commands::Mcp {
            stdio,
            port,
            kiln_path,
            just_dir,
            no_just,
            log_file,
        }) => {
            let args = commands::mcp::McpArgs {
                stdio,
                port,
                kiln_path,
                just_dir,
                no_just,
                log_file,
            };
            commands::mcp::execute(config, args).await?
        }

        Some(Commands::Process {
            path,
            force,
            watch,
            dry_run,
            parallel,
        }) => {
            commands::process::execute(config, path, force, watch, cli.verbose, dry_run, parallel)
                .await?
        }

        Some(Commands::Stats) => commands::stats::execute(config).await?,

        Some(Commands::Models) => commands::models::execute(config).await?,

        Some(Commands::Config(cmd)) => commands::config::execute(cmd).await?,

        Some(Commands::Status {
            path,
            format,
            detailed,
            recent,
        }) => commands::status::execute(config, path, format, detailed, recent).await?,

        Some(Commands::Storage(cmd)) => commands::storage::execute(config, cmd).await?,

        Some(Commands::Agents { command }) => commands::agents::execute(config, command).await?,

        Some(Commands::Tasks { file, command }) => {
            commands::tasks::execute(config, file, command).await?
        }

        Some(Commands::Daemon(cmd)) => {
            commands::daemon::handle(cmd, cli_config_path).await?;
        }

        Some(Commands::Skills(cmd)) => {
            commands::skills::execute(config, cmd).await?;
        }

        Some(Commands::Tools(cmd)) => {
            commands::tools::execute(config, cmd).await?;
        }

        Some(Commands::Plugin(cmd)) => {
            commands::plugin::execute(config, cmd).await?;
        }

        Some(Commands::Init {
            path,
            force,
            interactive,
        }) => {
            commands::init::execute(path, force, interactive).await?;
        }

        Some(Commands::Session(cmd)) => {
            commands::session::execute(config, cmd).await?;
        }

        Some(Commands::Auth { command }) => {
            commands::auth::execute(command).await?;
        }

        Some(Commands::Set { settings, session }) => {
            commands::set::execute(settings, session).await?;
        }

        #[cfg(feature = "web")]
        Some(Commands::Web(cmd)) => {
            commands::web::handle(cmd).await?;
        }

        None => {
            commands::chat::execute(
                config,
                None,
                None,
                false,
                false,
                Some(5),
                None,
                16384,
                vec![],
                None,
                vec![],
                None,
                None,
                1.0,
                None,
            )
            .await?
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- parse_log_level ----

    #[test]
    fn parse_log_level_all_valid_lowercase() {
        assert_eq!(parse_log_level("off"), Some(LevelFilter::OFF));
        assert_eq!(parse_log_level("error"), Some(LevelFilter::ERROR));
        assert_eq!(parse_log_level("warn"), Some(LevelFilter::WARN));
        assert_eq!(parse_log_level("info"), Some(LevelFilter::INFO));
        assert_eq!(parse_log_level("debug"), Some(LevelFilter::DEBUG));
        assert_eq!(parse_log_level("trace"), Some(LevelFilter::TRACE));
    }

    #[test]
    fn parse_log_level_case_insensitive() {
        assert_eq!(parse_log_level("OFF"), Some(LevelFilter::OFF));
        assert_eq!(parse_log_level("Debug"), Some(LevelFilter::DEBUG));
        assert_eq!(parse_log_level("Trace"), Some(LevelFilter::TRACE));
    }

    #[test]
    fn parse_log_level_invalid_returns_none() {
        assert_eq!(parse_log_level("verbose"), None);
        assert_eq!(parse_log_level(""), None);
        assert_eq!(parse_log_level("quiet"), None);
        assert_eq!(parse_log_level("warning"), None);
    }

    // ---- SocketCleanup ----

    #[test]
    fn socket_cleanup_removes_file_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        std::fs::write(&sock, b"").unwrap();
        assert!(sock.exists(), "file should exist before drop");

        {
            let _guard = SocketCleanup(sock.clone());
        } // dropped here

        assert!(!sock.exists(), "file should be removed after drop");
    }

    #[test]
    fn socket_cleanup_no_panic_on_missing() {
        let path = std::path::PathBuf::from("/tmp/crucible_nonexistent_socket_test.sock");
        assert!(!path.exists());

        // Should not panic when dropping a cleanup guard for a nonexistent file
        let _guard = SocketCleanup(path);
        drop(_guard);
    }
}
