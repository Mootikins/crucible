//! Regression test: `cru process <path>` must process the path the user named.
//!
//! The command used to open `config.kiln_path` for storage while only *reporting*
//! the user's `--path` in its summary. With a configured kiln of A and
//! `cru process B --force`, every file landed in A's index (or was skipped as
//! unchanged there) while the output claimed B was processed — silently
//! indexing the wrong kiln.

use anyhow::{bail, Result};
use crucible_cli::commands::process;
use crucible_cli::config::CliConfig;
use crucible_core::config::{AcpConfig, BackendType, LlmConfig, LlmProviderConfig};
use crucible_core::storage::Scope;
use crucible_core::test_support::EnvVarGuard;
use crucible_daemon::rpc_client::lifecycle;
use crucible_daemon::rpc_client::DaemonClient;
use crucible_daemon::Server;
use serial_test::serial;
use std::path::{Path, PathBuf};
use tokio::net::UnixStream;
use tokio::time::{Duration, Instant};

const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(2);
const DAEMON_READY_POLL: Duration = Duration::from_millis(10);

async fn wait_for_daemon_ready(socket_path: &Path) -> Result<()> {
    let deadline = Instant::now() + DAEMON_READY_TIMEOUT;
    loop {
        if UnixStream::connect(socket_path).await.is_ok() {
            return Ok(());
        }
        if Instant::now() > deadline {
            bail!(
                "daemon at {} did not become connectable within {:?}",
                socket_path.display(),
                DAEMON_READY_TIMEOUT
            );
        }
        tokio::time::sleep(DAEMON_READY_POLL).await;
    }
}

fn config_with_kiln(kiln_path: PathBuf) -> CliConfig {
    #![allow(clippy::field_reassign_with_default)]
    #[allow(clippy::field_reassign_with_default)]
    let mut llm_config = LlmConfig::default();
    llm_config.default = Some("local".to_string());
    llm_config.providers.insert(
        "local".to_string(),
        LlmProviderConfig::builder(BackendType::FastEmbed).build(),
    );

    CliConfig {
        kiln_path,
        acp: AcpConfig {
            default_agent: Some("test-agent".to_string()),
            ..Default::default()
        },
        llm: llm_config,
        ..Default::default()
    }
}

fn write_note(dir: &Path, name: &str, body: &str) -> Result<PathBuf> {
    let crucible = dir.join(".crucible");
    std::fs::create_dir_all(&crucible)?;
    let p = dir.join(name);
    std::fs::write(&p, body)?;
    Ok(p)
}

#[tokio::test]
#[serial]
async fn process_with_explicit_path_targets_that_kiln_not_the_configured_one() -> Result<()> {
    // Two distinct kilns: the CONFIGURED one (what config.kiln_path names) and
    // the TARGET one (what the user passes on the command line).
    let configured = tempfile::tempdir()?;
    let target = tempfile::tempdir()?;
    let env_dir = tempfile::tempdir()?;

    let _env_guard = EnvVarGuard::set(
        "XDG_RUNTIME_DIR",
        env_dir.path().to_str().unwrap().to_string(),
    );
    let socket_path = lifecycle::default_socket_path();
    let server = Server::bind_with_data_home(&socket_path, env_dir.path().to_path_buf()).await?;
    let shutdown = server.shutdown_handle();
    let handle = tokio::spawn(async move {
        let _ = server.run().await;
    });
    wait_for_daemon_ready(&socket_path).await?;

    write_note(
        configured.path(),
        "configured-kiln-note.md",
        "# Decoy\n\nLives in the configured kiln and must not be indexed by this run.",
    )?;
    write_note(
        target.path(),
        "target-kiln-note.md",
        "# Target\n\nThe note whose kiln the user named on the command line.",
    )?;

    // Config points at the WRONG (configured) kiln on purpose; the command line
    // names the target.
    let config = config_with_kiln(configured.path().to_path_buf());

    process::execute(
        config,
        Some(target.path().to_path_buf()),
        true,
        false,
        false,
        false,
        false,
    )
    .await?;

    // Ask the daemon what landed where. The target kiln must hold its own note;
    // the configured kiln must NOT have absorbed it.
    let client = DaemonClient::connect_or_start().await?;
    let scope_target = Scope::workspace(target.path())?;
    let target_notes = client
        .list_notes(target.path(), None, Some(scope_target))
        .await?;
    let target_has_note = target_notes.iter().any(|row| {
        let path = &row.path;
        path == "target-kiln-note.md" || path.ends_with("target-kiln-note.md")
    });
    assert!(
        target_has_note,
        "target kiln missing its note; indexed paths: {:?}",
        target_notes.iter().map(|row| &row.path).collect::<Vec<_>>()
    );

    let scope_configured = Scope::workspace(configured.path())?;
    let configured_notes = client
        .list_notes(configured.path(), None, Some(scope_configured))
        .await?;
    let configured_got_stray = configured_notes
        .iter()
        .any(|row| !row.path.contains("configured-kiln-note.md"));
    assert!(
        !configured_got_stray || configured_notes.len() <= 1,
        "configured kiln unexpectedly received notes from the run: {:?}",
        configured_notes
            .iter()
            .map(|row| &row.path)
            .collect::<Vec<_>>()
    );
    assert!(
        configured_notes.is_empty(),
        "the run processed the CONFIGURED kiln instead of the named one; got {:?}",
        configured_notes
            .iter()
            .map(|row| &row.path)
            .collect::<Vec<_>>()
    );

    shutdown.send(()).ok();
    handle.abort();
    Ok(())
}
