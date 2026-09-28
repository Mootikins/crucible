use super::super::export::export;
use super::super::io::{read_transcript, transcript_text};
use super::super::show::show;
use super::{setup_test_session, test_config, test_sessions_dir};
use tempfile::TempDir;

#[tokio::test]
async fn test_show_session() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = test_sessions_dir(tmp.path());

    let id = setup_test_session(&sessions_path).await;

    let config = test_config(tmp.path());

    let result = show(config, id.to_string(), "text".to_string()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_show_session_not_found() {
    let tmp = TempDir::new().unwrap();
    let _sessions_dir = test_sessions_dir(tmp.path());

    let config = test_config(tmp.path());

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
    let sessions_path = test_sessions_dir(tmp.path());

    let id = setup_test_session(&sessions_path).await;

    let config = test_config(tmp.path());

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

/// The offline path of `export` (`export.rs`) against the real wire-format
/// log: the daemon's fold and the daemon's export renderer.
#[tokio::test]
async fn export_renders_the_fixtures_conversation() {
    let tmp = TempDir::new().unwrap();
    let sessions_path = test_sessions_dir(tmp.path());

    let id = setup_test_session(&sessions_path).await;
    let transcript = read_transcript(&sessions_path.join(id.as_str()))
        .await
        .unwrap();
    let md = crucible_daemon::render_to_markdown(&transcript, &Default::default());

    assert!(md.contains("## User"), "{md}");
    assert!(md.contains("how do I read a file"), "{md}");
    assert!(md.contains("Use std::fs::read_to_string."), "{md}");
    assert!(md.contains("### Tool: `read_file`"), "{md}");
}

/// The offline views of `cru session show` and `list`, pinned against the
/// fixtures. The markdown view is the daemon's export, which
/// `crucible-daemon`'s `observe::golden_tests` pins.
/// `CRUCIBLE_WRITE_GOLDEN=1` writes the files.
#[test]
fn the_offline_session_views_of_each_fixture_match_their_golden_files() {
    use crucible_daemon::test_support::{assert_golden, fixture_path, stored_log, READER_FIXTURES};
    let dir = fixture_path("golden").join("cli_session");
    for name in READER_FIXTURES {
        let transcript = crucible_daemon::transcript_of_log("s1", &stored_log(name));
        let stem = name.trim_end_matches(".jsonl");
        assert_golden(
            &dir.join(format!("{stem}.view")),
            &transcript_text(stem, &transcript),
        );
        let (count, title) = crucible_daemon::transcript_summary(&transcript);
        assert_golden(
            &dir.join(format!("{stem}.list.view")),
            &format!("({count} messages)\n{title}"),
        );
    }
}
