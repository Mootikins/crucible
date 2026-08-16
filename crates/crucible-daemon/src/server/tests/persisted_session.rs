use super::*;

// ── Session Observe Handler Tests ──────────────────────────────────
//
// Every handler here is keyed on a session *id* against an injected sessions
// root. That root is a `TempDir`, never the developer's `~/.crucible`, and the
// handlers can no longer be pointed at an arbitrary directory — which is the
// whole point of the relocation: a caller that could name the directory could
// name any directory.

/// A sessions root holding one session whose JSONL has three sample events.
/// Returns the root and the session id.
fn seed_session(tmp: &TempDir) -> (PathBuf, String) {
    let sessions_root = tmp.path().join("sessions");
    let session_id = "chat-20260101-1200-abcd".to_string();
    let session_dir = sessions_root.join(&session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let events = [
        "{\"type\":\"init\",\"ts\":\"2026-01-01T12:00:00Z\",\"session_id\":\"chat-20260101-1200-abcd\"}",
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"Hello world\"}",
        "{\"type\":\"assistant\",\"ts\":\"2026-01-01T12:00:02Z\",\"content\":\"Hi there!\"}",
    ];
    std::fs::write(session_dir.join("session.jsonl"), events.join("\n") + "\n").unwrap();
    (sessions_root, session_id)
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
    let (sessions_root, session_id) = seed_session(&tmp);

    let req = make_request("session.load_events", json!({ "session_id": session_id }));
    let resp = handle_session_load_events(req, &sessions_root).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    let events = result.as_array().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["type"], "init");
    assert_eq!(events[1]["type"], "user");
    assert_eq!(events[2]["type"], "assistant");
}

#[tokio::test]
async fn session_load_events_unknown_id_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let (sessions_root, _) = seed_session(&tmp);

    let req = make_request(
        "session.load_events",
        json!({ "session_id": "chat-does-not-exist" }),
    );
    let resp = handle_session_load_events(req, &sessions_root).await;

    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    assert!(result.as_array().unwrap().is_empty());
}

/// A traversing id must not reach outside the sessions root. `session_id` is
/// joined onto a daemon-owned path, so `../` in it would otherwise be a
/// readable-anything primitive over the RPC surface.
#[tokio::test]
async fn session_load_events_refuses_an_id_that_escapes_the_sessions_root() {
    let tmp = TempDir::new().unwrap();
    let (sessions_root, session_id) = seed_session(&tmp);
    // A sibling of the sessions root, shaped like a session so that reaching it
    // would succeed rather than merely miss.
    let outside = tmp.path().join("elsewhere").join(&session_id);
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(
        outside.join("session.jsonl"),
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"secret\"}\n",
    )
    .unwrap();

    let req = make_request(
        "session.load_events",
        json!({ "session_id": format!("../elsewhere/{session_id}") }),
    );
    let resp = handle_session_load_events(req, &sessions_root).await;

    let leaked = serde_json::to_string(&resp).unwrap();
    assert!(
        !leaked.contains("secret"),
        "a traversing session id reached outside the sessions root: {leaked}"
    );
}

#[tokio::test]
async fn session_render_markdown_produces_output() {
    let tmp = TempDir::new().unwrap();
    let (sessions_root, session_id) = seed_session(&tmp);

    let req = make_request(
        "session.render_markdown",
        json!({ "session_id": session_id }),
    );
    let resp = handle_session_render_markdown(req, &sessions_root).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    let md = result["markdown"].as_str().unwrap();
    assert!(md.contains("Hello world"), "should contain user message");
    assert!(md.contains("Hi there!"), "should contain assistant message");
}

#[tokio::test]
async fn session_export_to_file_writes_markdown() {
    let tmp = TempDir::new().unwrap();
    let (sessions_root, session_id) = seed_session(&tmp);
    let output = tmp.path().join("exported.md");

    let req = make_request(
        "session.export_to_file",
        json!({
            "session_id": session_id,
            "output_path": output.to_string_lossy().to_string(),
        }),
    );
    let resp = handle_session_export_to_file(req, &sessions_root).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    assert_eq!(result["status"], "ok");
    assert!(output.exists(), "exported file should exist");
    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains("Hello world"));
}

// ── Scoped handlers: list_persisted and cleanup ────────────────────
//
// These two read and delete across the flat sessions root, so the per-kiln
// directory that used to bound them is gone. Kiln-set overlap replaces it,
// exactly as `handle_session_search` does it.

/// A session manager rooted at `tmp/sessions`.
fn manager_for(tmp: &TempDir) -> Arc<SessionManager> {
    Arc::new(SessionManager::new(FileSessionStorage::root_for(
        tmp.path(),
    )))
}

/// Persist a session attached to `kilns` with `body` as its transcript.
async fn seed_scoped_session(
    sm: &SessionManager,
    kilns: Vec<PathBuf>,
    body: &str,
) -> (String, PathBuf) {
    let session =
        crucible_core::session::Session::new(crucible_core::session::SessionType::Chat, kilns);
    sm.update_session(&session).await.unwrap();
    let dir = sm.session_dir(&session.id);
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join("session.jsonl"), body)
        .await
        .unwrap();
    (session.id.to_string(), dir)
}

fn listed_ids(resp: &Response) -> Vec<String> {
    resp.result.as_ref().unwrap()["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect()
}

/// Seeded through `Session::new` — the id the daemon actually mints
/// (`chat-2026-01-04T1530-a1b2c3`), not the `SessionId::parse` shape
/// (`chat-20260104-1530-a1b2`) the old directory walk filtered on. The two
/// schemes have never agreed, so a handler that enumerated the root through
/// `SessionId` saw none of a running daemon's sessions.
#[tokio::test]
async fn session_list_persisted_returns_sessions() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let kiln = tmp.path().join("mine");
    let (sid, _) = seed_scoped_session(
        &sm,
        vec![kiln.clone()],
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"Test message\"}",
    )
    .await;

    let req = make_request(
        "session.list_persisted",
        json!({ "kiln": kiln.to_string_lossy() }),
    );
    let resp = handle_session_list_persisted(req, &sm).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    assert_eq!(result["total"], 1);
    let sessions = result["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], sid);
    assert_eq!(sessions[0]["message_count"], 1);
}

#[tokio::test]
async fn session_list_persisted_empty_root_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    std::fs::create_dir_all(sm.sessions_root()).unwrap();

    let req = make_request(
        "session.list_persisted",
        json!({ "kiln": tmp.path().join("mine").to_string_lossy() }),
    );
    let resp = handle_session_list_persisted(req, &sm).await;

    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    assert_eq!(result["total"], 0);
    assert_eq!(result["sessions"].as_array().unwrap().len(), 0);
}

/// `title` is not metadata — it is the first 50 characters of the session's
/// first user message. Listing over the flat root without a scope filter
/// therefore hands back the opening line of every conversation on the box,
/// including kilns the caller never attached.
#[tokio::test]
async fn session_list_persisted_returns_only_sessions_sharing_a_kiln_with_the_caller() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let mine = tmp.path().join("mine");
    let theirs = tmp.path().join("theirs");

    let (ours, _) = seed_scoped_session(
        &sm,
        vec![mine.clone()],
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"my own question\"}",
    )
    .await;
    let (foreign, _) = seed_scoped_session(
        &sm,
        vec![theirs],
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:02Z\",\"content\":\"a secret from another corpus\"}",
    )
    .await;

    let req = make_request(
        "session.list_persisted",
        json!({ "kiln": mine.to_string_lossy() }),
    );
    let resp = handle_session_list_persisted(req, &sm).await;

    let ids = listed_ids(&resp);
    assert!(ids.contains(&ours), "own-kiln session missing: {ids:?}");
    assert!(
        !ids.contains(&foreign),
        "session from an unshared kiln leaked: {ids:?}"
    );
    let body = serde_json::to_string(resp.result.as_ref().unwrap()).unwrap();
    assert!(
        !body.contains("a secret from another corpus"),
        "message content from an unshared kiln leaked: {body}"
    );
}

/// Overlap is over the caller's WHOLE kiln set, the same rule
/// `session.search` follows. A caller attached to `[a, b]` that can only spell
/// one of them sees a fraction of its own reach — its `b` sessions read as
/// another corpus's.
#[tokio::test]
async fn session_list_persisted_spans_every_kiln_in_the_callers_set() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    let elsewhere = tmp.path().join("elsewhere");
    let msg = "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"hello\"}";

    let (on_a, _) = seed_scoped_session(&sm, vec![a.clone()], msg).await;
    let (on_b, _) = seed_scoped_session(&sm, vec![b.clone()], msg).await;
    let (foreign, _) = seed_scoped_session(&sm, vec![elsewhere], msg).await;

    let req = make_request(
        "session.list_persisted",
        json!({ "kilns": [a.to_string_lossy(), b.to_string_lossy()] }),
    );
    let resp = handle_session_list_persisted(req, &sm).await;

    let ids = listed_ids(&resp);
    assert!(ids.contains(&on_a), "first-kiln session missing: {ids:?}");
    assert!(
        ids.contains(&on_b),
        "session sharing only the caller's second kiln missing: {ids:?}"
    );
    assert!(
        !ids.contains(&foreign),
        "session from an unshared kiln leaked: {ids:?}"
    );
}

/// No scope is not every scope, the same rule `session.search` follows.
#[tokio::test]
async fn session_list_persisted_without_a_kiln_scope_returns_nothing() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    seed_scoped_session(
        &sm,
        vec![tmp.path().join("somewhere")],
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"hello\"}",
    )
    .await;

    let req = make_request("session.list_persisted", json!({}));
    let resp = handle_session_list_persisted(req, &sm).await;

    assert!(resp.error.is_none());
    assert!(listed_ids(&resp).is_empty());
}

#[tokio::test]
async fn session_cleanup_dry_run_does_not_delete() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let kiln = tmp.path().join("mine");
    let (_, session_dir) = seed_scoped_session(
        &sm,
        vec![kiln.clone()],
        "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}",
    )
    .await;

    let req = make_request(
        "session.cleanup",
        json!({ "older_than_days": 1, "dry_run": true, "kiln": kiln.to_string_lossy() }),
    );
    let resp = handle_session_cleanup(req, &sm).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    assert_eq!(result["dry_run"], true);
    assert_eq!(result["total"], 1);
    assert!(session_dir.exists(), "dry run should not delete");
}

#[tokio::test]
async fn session_cleanup_deletes_old_sessions() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let kiln = tmp.path().join("mine");
    let (_, session_dir) = seed_scoped_session(
        &sm,
        vec![kiln.clone()],
        "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}",
    )
    .await;

    let req = make_request(
        "session.cleanup",
        json!({ "older_than_days": 1, "dry_run": false, "kiln": kiln.to_string_lossy() }),
    );
    let resp = handle_session_cleanup(req, &sm).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    let result = resp.result.unwrap();
    assert_eq!(result["dry_run"], false);
    assert_eq!(result["total"], 1);
    assert!(!session_dir.exists(), "old session should be deleted");
}

/// The destructive counterpart of the listing rule: `cru session cleanup` run
/// from one kiln must not reach into another kiln's sessions. There is no
/// confirmation prompt and no undo, so the blast radius has to be the scope
/// the caller named.
#[tokio::test]
async fn session_cleanup_deletes_only_sessions_sharing_a_kiln_with_the_caller() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let mine = tmp.path().join("mine");
    let theirs = tmp.path().join("theirs");
    let old = "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}";

    let (_, ours) = seed_scoped_session(&sm, vec![mine.clone()], old).await;
    let (_, foreign) = seed_scoped_session(&sm, vec![theirs], old).await;

    let req = make_request(
        "session.cleanup",
        json!({ "older_than_days": 1, "dry_run": false, "kiln": mine.to_string_lossy() }),
    );
    let resp = handle_session_cleanup(req, &sm).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    assert!(!ours.exists(), "own-kiln session should be deleted");
    assert!(
        foreign.exists(),
        "cleanup deleted a session from a kiln the caller never named"
    );
}

/// The destructive side of the same overlap rule: a caller attached to
/// `[a, b]` that can only name one of them has to run cleanup twice, and the
/// second run is the one nobody remembers. Same predicate as the listing.
#[tokio::test]
async fn session_cleanup_spans_every_kiln_in_the_callers_set() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    let old = "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}";

    let (_, on_a) = seed_scoped_session(&sm, vec![a.clone()], old).await;
    let (_, on_b) = seed_scoped_session(&sm, vec![b.clone()], old).await;
    let (_, foreign) = seed_scoped_session(&sm, vec![tmp.path().join("elsewhere")], old).await;

    let req = make_request(
        "session.cleanup",
        json!({
            "older_than_days": 1,
            "dry_run": false,
            "kilns": [a.to_string_lossy(), b.to_string_lossy()],
        }),
    );
    let resp = handle_session_cleanup(req, &sm).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    assert!(!on_a.exists(), "first-kiln session should be deleted");
    assert!(
        !on_b.exists(),
        "session sharing only the caller's second kiln survived the sweep"
    );
    assert!(
        foreign.exists(),
        "cleanup deleted a session from a kiln the caller never named"
    );
}

/// A scope has to be stated. Silently defaulting to the whole backlog is the
/// machine-wide delete this handler must never perform by accident.
#[tokio::test]
async fn session_cleanup_without_a_scope_deletes_nothing() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let (_, dir) = seed_scoped_session(
        &sm,
        vec![tmp.path().join("mine")],
        "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}",
    )
    .await;

    let req = make_request(
        "session.cleanup",
        json!({ "older_than_days": 1, "dry_run": false }),
    );
    let resp = handle_session_cleanup(req, &sm).await;

    assert!(resp.error.is_some(), "unscoped cleanup should be refused");
    assert!(dir.exists(), "unscoped cleanup deleted a session");
}

/// Sweeping every kiln stays possible, but only when the caller says so in
/// as many words — which is also the only way a kiln-less session (a
/// legitimate state) can ever be collected.
#[tokio::test]
async fn session_cleanup_sweeps_every_kiln_only_when_explicitly_asked() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let old = "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}";

    let (_, ours) = seed_scoped_session(&sm, vec![tmp.path().join("mine")], old).await;
    let (_, foreign) = seed_scoped_session(&sm, vec![tmp.path().join("theirs")], old).await;
    let (_, kilnless) = seed_scoped_session(&sm, vec![], old).await;

    let req = make_request(
        "session.cleanup",
        json!({ "older_than_days": 1, "dry_run": false, "all_kilns": true }),
    );
    let resp = handle_session_cleanup(req, &sm).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    assert!(!ours.exists());
    assert!(!foreign.exists());
    assert!(
        !kilnless.exists(),
        "kiln-less sessions need a collector too"
    );
}

// ── The id inside meta.json is caller input ───────────────────────────
//
// `session.cleanup`, `session.list_persisted` and `session.search` all resolve
// `{sessions_root}/{summary.id}` — where `summary.id` is the `id` *field
// inside* the session's `meta.json`, not the directory it was found in. A kiln
// shared or synced between machines carries that file, and migration used to
// publish it verbatim, so the field arrives from outside the daemon.

/// Materialize a session directory named `dir_name` whose persisted `id` field
/// says `persisted_id`. Written as raw JSON on purpose: a `Session` cannot be
/// built with an id like this in Rust, which is the property under test.
async fn seed_session_with_persisted_id(
    sm: &SessionManager,
    kilns: Vec<PathBuf>,
    dir_name: &str,
    persisted_id: &str,
) -> PathBuf {
    let session =
        crucible_core::session::Session::new(crucible_core::session::SessionType::Chat, kilns);
    let mut meta = serde_json::to_value(&session).unwrap();
    meta["id"] = serde_json::Value::String(persisted_id.to_string());
    let dir = sm.sessions_root().join(dir_name);
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(
        dir.join("meta.json"),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .await
    .unwrap();
    dir
}

#[tokio::test]
async fn cleanup_never_removes_a_directory_named_by_a_persisted_id() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let kiln = tmp.path().join("mine");

    // A sibling of the sessions root: `{data_home}/backups`. It holds a
    // transcript-shaped file (so cleanup's age check has something to read)
    // alongside material that is not ours to delete — `remove_dir_all` takes
    // the whole directory, not the file that qualified it.
    let bystander = tmp.path().join("backups");
    std::fs::create_dir_all(&bystander).unwrap();
    std::fs::write(
        bystander.join("session.jsonl"),
        "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"old\"}",
    )
    .unwrap();
    std::fs::write(bystander.join("id_ed25519"), "PRIVATE KEY").unwrap();

    seed_session_with_persisted_id(&sm, vec![kiln.clone()], "chat-poisoned", "../backups").await;

    let req = make_request(
        "session.cleanup",
        json!({ "older_than_days": 1, "dry_run": false, "kiln": kiln.to_string_lossy() }),
    );
    let resp = handle_session_cleanup(req, &sm).await;

    assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
    assert!(
        bystander.join("id_ed25519").exists(),
        "cleanup removed a directory outside the sessions root: {:?}",
        resp.result
    );
    assert_eq!(
        resp.result.as_ref().unwrap()["deleted"],
        serde_json::json!([]),
        "cleanup reported deleting a session it must never have seen"
    );
}

#[tokio::test]
async fn list_persisted_never_reads_a_transcript_named_by_a_persisted_id() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let kiln = tmp.path().join("mine");

    let bystander = tmp.path().join("backups");
    std::fs::create_dir_all(&bystander).unwrap();
    std::fs::write(
        bystander.join("session.jsonl"),
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"MY-BANK-PASSWORD\"}",
    )
    .unwrap();

    seed_session_with_persisted_id(&sm, vec![kiln.clone()], "chat-poisoned", "../backups").await;

    let req = make_request(
        "session.list_persisted",
        json!({ "kiln": kiln.to_string_lossy() }),
    );
    let resp = handle_session_list_persisted(req, &sm).await;

    let rendered = format!("{:?}", resp.result);
    assert!(
        !rendered.contains("MY-BANK-PASSWORD"),
        "the listing read a transcript from outside the sessions root: {rendered}"
    );
}

/// The directory name is the id. A `meta.json` that disagrees is either
/// corrupt or planted, and either way the session must not be served under a
/// name the daemon never filed it under.
#[tokio::test]
async fn a_session_whose_persisted_id_is_not_a_path_component_is_not_listed() {
    let tmp = TempDir::new().unwrap();
    let sm = manager_for(&tmp);
    let kiln = tmp.path().join("mine");
    seed_session_with_persisted_id(&sm, vec![kiln.clone()], "chat-poisoned", "../backups").await;

    let req = make_request(
        "session.list_persisted",
        json!({ "kiln": kiln.to_string_lossy() }),
    );
    let resp = handle_session_list_persisted(req, &sm).await;

    assert!(
        listed_ids(&resp).is_empty(),
        "a session with an unusable persisted id was listed: {:?}",
        listed_ids(&resp)
    );
}
