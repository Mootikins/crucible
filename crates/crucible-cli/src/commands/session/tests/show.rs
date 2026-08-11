use super::super::export::export;
use super::super::io::{format_events_markdown, read_session_events};
use super::super::show::show;
use super::setup_test_session;
use crate::config::CliConfig;
use tempfile::TempDir;

#[tokio::test]
async fn test_show_session() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let id = setup_test_session(&sessions_path).await;

    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let result = show(config, id.to_string(), "text".to_string()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_show_session_not_found() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let result = show(
        config,
        "chat-20260104-1530-a1b2".to_string(),
        "text".to_string(),
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_export_session() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let id = setup_test_session(&sessions_path).await;

    let config = CliConfig {
        kiln_path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let output_path = tmp.path().join("exported.md");
    let result = export(config, id.to_string(), Some(output_path.clone()), false).await;
    assert!(result.is_ok());
    assert!(output_path.exists());

    // Deliberately not asserting the file's *content* here. `export`
    // (`export.rs`) prefers the daemon's `session.export_to_file` RPC and
    // only falls back to rendering locally, so what lands in this file depends
    // on whether a daemon happens to be reachable and which build it is — a
    // developer with a stale `cru daemon serve` running gets that daemon's
    // answer. The content of the CLI's own path is pinned by
    // `export_renders_the_fixtures_conversation` below, and the daemon's by
    // `crucible-daemon`'s `session_export_to_file_writes_markdown`.
}

/// The fallback renderer `export` uses when no daemon is reachable
/// (`export.rs`), against the real wire-format log.
///
/// The fixture's conversation, not the old three-message `"You are helpful"` /
/// `"Hello, how are you?"` script: `setup_test_session` now materializes
/// `assets/fixtures/session_log_wire.jsonl`, which is one user message, a
/// thinking block, a tool call and its result, and the completed response.
/// There is no `system` event in it — the daemon does not persist system
/// prompts. Before the read-path fix this rendered to the empty string.
#[tokio::test]
async fn export_renders_the_fixtures_conversation() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = tmp.path().join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_path).unwrap();

    let id = setup_test_session(&sessions_path).await;
    let events = read_session_events(&sessions_path.join(id.as_str()))
        .await
        .unwrap();
    let md = format_events_markdown(&events, false);

    assert!(md.contains("## User"), "{md}");
    assert!(md.contains("how do I read a file"), "{md}");
    assert!(md.contains("Use std::fs::read_to_string."), "{md}");
    assert!(md.contains("### Tool: read_file"), "{md}");
}
