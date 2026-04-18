use super::*;

// ── Session Observe Handler Tests ──────────────────────────────────

/// Create a test session directory with a JSONL file containing sample events.
fn create_test_session_dir(tmp: &TempDir) -> PathBuf {
    let session_dir = tmp.path().join("chat-20260101-1200-abcd");
    std::fs::create_dir_all(&session_dir).unwrap();
    let jsonl = session_dir.join("session.jsonl");
    let events = [
        "{\"type\":\"init\",\"ts\":\"2026-01-01T12:00:00Z\",\"session_id\":\"chat-20260101-1200-abcd\"}",
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"Hello world\"}",
        "{\"type\":\"assistant\",\"ts\":\"2026-01-01T12:00:02Z\",\"content\":\"Hi there!\"}",
    ];
    std::fs::write(&jsonl, events.join("\n") + "\n").unwrap();
    session_dir
}

fn make_request(method: &str, params: Value) -> Request {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    }))
    .unwrap()
}

#[tokio::test]
async fn session_load_events_returns_events_from_jsonl() {
    let tmp = TempDir::new().unwrap();
    let session_dir = create_test_session_dir(&tmp);

    let req = make_request(
        "session.load_events",
        json!({ "session_dir": session_dir.to_string_lossy().to_string() }),
    );
    let resp = handle_session_load_events(req).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    let events = result.as_array().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["type"], "init");
    assert_eq!(events[1]["type"], "user");
    assert_eq!(events[2]["type"], "assistant");
}

#[tokio::test]
async fn session_load_events_missing_dir_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("nonexistent");

    let req = make_request(
        "session.load_events",
        json!({ "session_dir": missing.to_string_lossy().to_string() }),
    );
    let resp = handle_session_load_events(req).await;

    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    let events = result.as_array().unwrap();
    assert!(events.is_empty());
}

#[tokio::test]
async fn session_render_markdown_produces_output() {
    let tmp = TempDir::new().unwrap();
    let session_dir = create_test_session_dir(&tmp);

    let req = make_request(
        "session.render_markdown",
        json!({ "session_dir": session_dir.to_string_lossy().to_string() }),
    );
    let resp = handle_session_render_markdown(req).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    let md = result["markdown"].as_str().unwrap();
    assert!(md.contains("Hello world"), "should contain user message");
    assert!(md.contains("Hi there!"), "should contain assistant message");
}

#[tokio::test]
async fn session_export_to_file_writes_markdown() {
    let tmp = TempDir::new().unwrap();
    let session_dir = create_test_session_dir(&tmp);
    let output = tmp.path().join("exported.md");

    let req = make_request(
        "session.export_to_file",
        json!({
            "session_dir": session_dir.to_string_lossy().to_string(),
            "output_path": output.to_string_lossy().to_string(),
        }),
    );
    let resp = handle_session_export_to_file(req).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    assert_eq!(result["status"], "ok");
    assert!(output.exists(), "exported file should exist");
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains("Hello world"));
}

#[tokio::test]
async fn session_list_persisted_returns_sessions() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    let sessions_dir = kiln.join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let sid = "chat-20260101-1200-abcd";
    let session_dir = sessions_dir.join(sid);
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("session.jsonl"),
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"Test message\"}",
    )
    .unwrap();

    let req = make_request(
        "session.list_persisted",
        json!({ "kiln": kiln.to_string_lossy().to_string() }),
    );
    let resp = handle_session_list_persisted(req).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    assert_eq!(result["total"], 1);
    let sessions = result["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], sid);
    assert_eq!(sessions[0]["message_count"], 1);
}

#[tokio::test]
async fn session_list_persisted_empty_kiln_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("empty-kiln");
    std::fs::create_dir_all(&kiln).unwrap();

    let req = make_request(
        "session.list_persisted",
        json!({ "kiln": kiln.to_string_lossy().to_string() }),
    );
    let resp = handle_session_list_persisted(req).await;

    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    assert_eq!(result["total"], 0);
    assert_eq!(result["sessions"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn session_cleanup_dry_run_does_not_delete() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    let sessions_dir = kiln.join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let sid = "chat-20200101-1200-a0b1";
    let session_dir = sessions_dir.join(sid);
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("session.jsonl"),
        "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}",
    )
    .unwrap();

    let req = make_request(
        "session.cleanup",
        json!({
            "kiln": kiln.to_string_lossy().to_string(),
            "older_than_days": 1,
            "dry_run": true,
        }),
    );
    let resp = handle_session_cleanup(req).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    assert_eq!(result["dry_run"], true);
    assert_eq!(result["total"], 1);
    assert!(session_dir.exists(), "dry run should not delete");
}

#[tokio::test]
async fn session_cleanup_deletes_old_sessions() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    let sessions_dir = kiln.join(".crucible").join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let sid = "chat-20200101-1200-a0b2";
    let session_dir = sessions_dir.join(sid);
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("session.jsonl"),
        "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}",
    )
    .unwrap();

    let req = make_request(
        "session.cleanup",
        json!({
            "kiln": kiln.to_string_lossy().to_string(),
            "older_than_days": 1,
            "dry_run": false,
        }),
    );
    let resp = handle_session_cleanup(req).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    assert_eq!(result["dry_run"], false);
    assert_eq!(result["total"], 1);
    assert!(!session_dir.exists(), "old session should be deleted");
}
