// The mock daemon + router helpers are shared with integration tests
// (tests/route_contract_tests/) via the `test-utils` feature — crucible-cli
// dev-depends on itself with that feature, so they compile in every test
// build without CI feature flags. Do NOT fork a second copy: the two copies
// this replaced drifted (different resume_from_storage shapes).
use crate::services::daemon::{AppState, EventBroker, ReconnectingDaemon};
#[cfg(any(test, feature = "test-utils"))]
use axum::Router;
#[cfg(any(test, feature = "test-utils"))]
use crucible_core::config::CliAppConfig;
#[cfg(any(test, feature = "test-utils"))]
use crucible_core::protocol::requests::{
    DiffCommentKey, DiffCommentReply, DiffCommentRequest, DiffFileRequest, DiffsetRef,
};
#[cfg(any(test, feature = "test-utils"))]
use crucible_core::protocol::rpc::RpcMethod;
#[cfg(any(test, feature = "test-utils"))]
use crucible_daemon::DaemonClient;
#[cfg(any(test, feature = "test-utils"))]
use serde_json::{json, Value};
#[cfg(any(test, feature = "test-utils"))]
use std::collections::HashMap;
#[cfg(any(test, feature = "test-utils"))]
use std::path::PathBuf;
#[cfg(any(test, feature = "test-utils"))]
use std::sync::Arc;
#[cfg(any(test, feature = "test-utils"))]
use tempfile::TempDir;
#[cfg(any(test, feature = "test-utils"))]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(any(test, feature = "test-utils"))]
use tokio::net::UnixListener;

use proptest::prelude::*;

pub fn arb_traversal_path() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9/_-]{0,24}\\.\\.[a-zA-Z0-9/_-]{0,24}".prop_map(|s| s.to_string()),
        "[a-zA-Z0-9/_-]{0,24}\\x00[a-zA-Z0-9/_-]{0,24}".prop_map(|s| s.to_string()),
        Just("../etc/passwd".to_string()),
        Just("safe/..\0/evil".to_string()),
    ]
}

pub fn arb_safe_path() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-][a-zA-Z0-9/_-]{0,63}"
        .prop_filter("no traversal, null byte, or absolute path", |s| {
            !s.contains("..") && !s.contains('\0') && !s.starts_with('/')
        })
}

#[cfg(any(test, feature = "test-utils"))]
/// A mock daemon that listens on a Unix socket and responds to JSON-RPC calls
/// with canned responses. This allows testing HTTP routes without a real daemon.
pub struct MockDaemon {
    _tmp: TempDir,
    /// Every JSON-RPC request the mock received, in order. Lets a test assert a
    /// call was (or crucially, was NOT) made — e.g. that a validation failure
    /// short-circuits before `session.create` leaves an orphan session — and
    /// what params rode along, for routes whose contract is "forward this
    /// untouched".
    calls: Arc<std::sync::Mutex<Vec<Value>>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl MockDaemon {
    /// The JSON-RPC methods received so far, in order. A request whose name
    /// no [`RpcMethod`] carries is not in the list.
    pub fn received_methods(&self) -> Vec<RpcMethod> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter_map(|req| req.get("method").and_then(|m| m.as_str()))
            .filter_map(RpcMethod::parse)
            .collect()
    }

    /// The `params` object of the first request for `method`, exactly as it
    /// arrived on the wire. `None` when the method was never called.
    ///
    /// Wire-level, deliberately: a passthrough route's whole contract is that
    /// the value it was handed reaches the daemon unrewritten, and only the
    /// serialized form can show that a field was omitted rather than sent as
    /// `null`.
    pub fn received_params(&self, method: RpcMethod) -> Option<Value> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .find(|req| req.get("method").and_then(|m| m.as_str()) == Some(method.as_str()))
            .map(|req| req.get("params").cloned().unwrap_or(Value::Null))
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// Per-method scripted error envelopes: method → (code, message).
/// Methods present here answer `{"error": {...}}` instead of a result, so
/// tests can exercise the daemon-error → HTTP-status surface.
pub type MockErrors = HashMap<RpcMethod, (i64, String)>;

#[cfg(any(test, feature = "test-utils"))]
/// Start a mock daemon on a temporary Unix socket. Returns the mock daemon
/// handle (holds TempDir alive) and a connected DaemonClient.
pub async fn start_mock_daemon() -> (MockDaemon, DaemonClient) {
    start_mock_daemon_with_errors(MockErrors::new()).await
}

#[cfg(any(test, feature = "test-utils"))]
/// A real daemon in this process, with each directory of `kilns` registered
/// as an eager kiln, and a client connected to it.
///
/// A route test that reads or writes a real file needs the daemon's own root
/// rule, its readers and its writers. The mock does not copy them. Before this
/// function returns, the daemon indexes each markdown note that the kilns hold,
/// so the index routes see the notes that the test wrote. Keep the returned
/// daemon alive for the duration of the test, because the drop removes its
/// data home.
pub async fn start_real_daemon_with_kilns(
    kilns: &[PathBuf],
) -> (crucible_daemon::test_support::InProcessDaemon, DaemonClient) {
    let builder = kilns.iter().enumerate().fold(
        crucible_daemon::test_support::InProcessDaemonBuilder::new()
            .expect("a temporary data home for the daemon"),
        |builder, (index, kiln)| builder.with_kiln_at(&format!("kiln{index}"), kiln),
    );
    let daemon = builder.start().await.expect("start the in-process daemon");
    let client = daemon.connect().await;
    for kiln in kilns {
        let notes = markdown_notes(kiln);
        if !notes.is_empty() {
            client
                .process_batch(kiln, &notes)
                .await
                .expect("the daemon indexes the notes of the kiln");
        }
    }
    (daemon, client)
}

#[cfg(any(test, feature = "test-utils"))]
/// Each markdown note under `dir`, at any depth. A dot directory, such as the
/// `.crucible` directory of the daemon, is not read.
fn markdown_notes(dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .flat_map(|entry| {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => markdown_notes(&path),
                Ok(kind) if kind.is_file() && crucible_core::is_note_file(&path) => vec![path],
                _ => Vec::new(),
            }
        })
        .collect()
}

#[cfg(any(test, feature = "test-utils"))]
/// Like [`start_mock_daemon`], but methods listed in `errors` respond with a
/// JSON-RPC error envelope instead of their canned result.
pub async fn start_mock_daemon_with_errors(errors: MockErrors) -> (MockDaemon, DaemonClient) {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let socket_path = tmp.path().join("mock-daemon.sock");

    let listener = UnixListener::bind(&socket_path).expect("Failed to bind mock socket");

    // Spawn mock daemon server
    let errors = Arc::new(errors);
    let calls: Arc<std::sync::Mutex<Vec<Value>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls_srv = calls.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let errors = errors.clone();
            let calls = calls_srv.clone();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut reader = BufReader::new(read);
                let mut line = String::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let msg: Value = match serde_json::from_str(&line) {
                                Ok(m) => m,
                                Err(_) => continue,
                            };
                            calls.lock().unwrap().push(msg.clone());
                            let response = mock_rpc_envelope(&msg, &errors).await;

                            let mut resp_str = serde_json::to_string(&response).unwrap();
                            resp_str.push('\n');

                            if write.write_all(resp_str.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });

    // Give the listener a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let client = DaemonClient::connect_to(&socket_path)
        .await
        .expect("Failed to connect to mock daemon");

    (MockDaemon { _tmp: tmp, calls }, client)
}

#[cfg(any(test, feature = "test-utils"))]
/// The JSON-RPC envelope the mock daemon answers `msg` with.
///
/// A method name that [`RpcMethod`] does not know gets the "method not found"
/// error of the real daemon. A known method gets its scripted error, or else
/// its canned reply.
async fn mock_rpc_envelope(msg: &Value, errors: &MockErrors) -> Value {
    let id = msg.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
    let name = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let error = |code: i64, message: String| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        })
    };
    let Some(method) = RpcMethod::parse(name) else {
        return error(-32601, format!("Method not found: {name}"));
    };
    if let Some((code, message)) = errors
        .get(&method)
        .cloned()
        .or_else(|| mock_rpc_error(method, msg))
    {
        return error(code, message);
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": mock_rpc_response(method, msg).await
    })
}

#[cfg(any(test, feature = "test-utils"))]
/// Param-dependent scripted errors that the per-method [`MockErrors`] map can't
/// express. Mirrors the real daemon: `session.create` now owns agent
/// resolution, so an unresolvable agent fails atomically with `INVALID_PARAMS`
/// (JSON-RPC `-32602`) and creates nothing. `"missing"` is the sentinel name.
///
/// The branch on `agent_type` is not cosmetic. `agent_name` names two different
/// things daemon-side: an ACP profile when `agent_type` is `"acp"`, and an
/// agent card otherwise — the deprecated alias this crate is the last caller of
/// (`routes/session/mod.rs` sends a card name in `agent_name`). Collapsing the
/// two here would let the web's card path pass a test the daemon fails.
fn mock_rpc_error(method: RpcMethod, msg: &Value) -> Option<(i64, String)> {
    if method != RpcMethod::SessionCreate {
        return None;
    }
    let params = msg.get("params")?;
    let str_param = |key: &str| params.get(key).and_then(|v| v.as_str());
    if str_param("agent_name") != Some("missing") {
        return None;
    }
    Some(match str_param("agent_type") {
        Some("acp") => (-32602, "Unknown ACP agent profile: missing".to_string()),
        _ => (-32602, "Unknown agent card: missing".to_string()),
    })
}

#[cfg(any(test, feature = "test-utils"))]
/// One string param off a JSON-RPC request, or `""`. Lets an arm echo what it
/// was sent, which is how a passthrough route's contract gets asserted.
fn param_str<'a>(msg: &'a Value, key: &str) -> &'a str {
    msg.get("params")
        .and_then(|p| p.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

#[cfg(any(test, feature = "test-utils"))]
/// A `crucible_core::session::Comment` on the wire — all twelve fields, so a
/// route test sees what the frontend's `ReviewComment` will actually receive.
fn review_comment_fixture(id: &str, body: &str) -> Value {
    json!({
        "id": id,
        "diffset": "session-test-session-001",
        "root": "/tmp/test-project",
        "path": "src/a.rs",
        "anchor": { "kind": "snapshot", "id": "0000000000000000000000000000000000000000" },
        "side": "current",
        "line_range": { "start": 1, "end": 2 },
        "quoted": "a\n",
        "body": body,
        "author": "human",
        "resolved": false,
        "created_at": "2026-01-01T00:00:00Z",
    })
}

#[cfg(any(test, feature = "test-utils"))]
/// The kiln the mock daemon's effective config names — a different directory
/// from any web-side copy, so a route test can tell which one it read.
pub const MOCK_DAEMON_KILN_PATH: &str = "/daemon/kiln";

#[cfg(any(test, feature = "test-utils"))]
/// The file the mock daemon's pinned leaf comes from. A refusal must carry it
/// to the browser, or a user cannot open the line that holds the key.
pub const MOCK_PIN_FILE: &str = "/daemon/config/init.lua";

/// The `config.effective` provenance row for a leaf a human's `init.lua`
/// holds, in the wire form the real daemon sends.
///
/// Serialised from a real [`ConfigSource`], so the fixture cannot describe a
/// shape the type no longer has. A hand-written literal here was the only
/// thing asserting this variant's field shape, and a literal does not drift
/// with the type it is imitating.
///
/// [`ConfigSource`]: crucible_core::config::ConfigSource
fn mock_lua_provenance() -> serde_json::Value {
    serde_json::to_value(crucible_core::config::ConfigSource::Lua {
        last_set: crucible_core::config::LastSet::new(
            crucible_core::lua_source::LuaSource::UserLua,
            MOCK_PIN_FILE,
            Some(12),
        ),
    })
    .expect("a config source serialises")
}

#[cfg(any(test, feature = "test-utils"))]
/// The top-level key the mock daemon treats as pinned — see the `config.save`
/// arm of [`mock_rpc_response`].
pub const MOCK_PINNED_KEY: &str = "mock_pinned";

#[cfg(any(test, feature = "test-utils"))]
/// The reason the mock daemon gives for its one read-only leaf. A read-only
/// key must reach the browser WITH its reason, or the control is a dead end.
pub const MOCK_LOCATION_REASON: &str = "A location key names WHERE the daemon acts.";

// ── The replies the `fs.*`, `project.*` and `scm.*` mocks answer with ──────
//
// Each one is built from the type the DAEMON owns, then serialised. A route
// test that compares its HTTP body against the same fixture therefore runs a
// round trip through the daemon's own reply type: daemon type → JSON → route →
// reply type → body. A named reply that dropped a key the daemon writes fails
// it, which a `status == 200` assertion never could.

#[cfg(any(test, feature = "test-utils"))]
/// The listing `fs.list_dir` answers with: one file and one directory.
///
/// Not an empty listing. An empty `entries` proves the envelope and nothing
/// about a row, and every field of a row is part of the cross-language
/// contract.
pub fn mock_fs_listing() -> crucible_daemon::FsListing {
    crucible_daemon::FsListing {
        entries: vec![
            crucible_daemon::FsEntry {
                name: "notes".to_string(),
                rel_path: "notes".to_string(),
                is_dir: true,
                size: 0,
                modified: Some(1_700_000_000),
                status: None,
            },
            crucible_daemon::FsEntry {
                name: "a.md".to_string(),
                rel_path: "a.md".to_string(),
                is_dir: false,
                size: 42,
                // The one platform-dependent field. A row that cannot report
                // it still writes the key.
                modified: None,
                status: None,
            },
        ],
        truncated: false,
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// The reply `fs.move` answers a kiln note move with: the link-rewrite report.
///
/// The richer of the two arms on purpose. A project-file move answers `moved`
/// alone, which cannot show that the two report keys survive the route.
pub fn mock_fs_move_reply() -> crucible_daemon::FsMoveReply {
    crucible_daemon::FsMoveReply {
        moved: true,
        rewritten_sources: Some(vec!["index.md".to_string()]),
        skipped: Some(vec![crucible_daemon::SkippedRef {
            source_path: "other.md".to_string(),
            raw_target: "a".to_string(),
            reason: crucible_daemon::SkipReason::Ambiguous,
        }]),
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// The reply `fs.trash` answers with.
pub fn mock_fs_trash_reply() -> crucible_daemon::FsTrashReply {
    crucible_daemon::FsTrashReply {
        trashed: true,
        trash_path: ".crucible/trash/0-x".to_string(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// The project `project.register`, `project.list` and `project.get` answer
/// with.
///
/// It carries a named kiln, an unnamed one and a repository, so the three
/// optional corners of the shape are all on the wire. `/tmp/test-project` is
/// deliberately absent from disk: the registration re-check canonicalizes it
/// and must behave as it did.
pub fn mock_project() -> crucible_core::Project {
    crucible_core::Project {
        path: PathBuf::from("/tmp/test-project"),
        name: "test-project".to_string(),
        kilns: vec![
            crucible_core::project::ProjectKiln {
                path: PathBuf::from("/tmp/test-project/.crucible"),
                name: Some("test-kiln".to_string()),
            },
            // An unnamed kiln sends NO `name` key.
            crucible_core::project::ProjectKiln {
                path: PathBuf::from("/tmp/test-project/docs"),
                name: None,
            },
        ],
        last_accessed: "2025-01-01T00:00:00Z".parse().expect("a fixed timestamp"),
        repository: Some(crucible_core::project::RepositoryInfo {
            root: PathBuf::from("/tmp/test-project"),
            remote_url: Some("https://example.invalid/test-project.git".to_string()),
            is_worktree: false,
            main_repo_git_dir: None,
        }),
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// The reply `scm.clone` answers with.
pub fn mock_scm_clone() -> crucible_daemon::ScmCloneResponse {
    crucible_daemon::ScmCloneResponse {
        path: "/tmp/test-project".to_string(),
        project: mock_project(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// The diffset `diff.get` answers for `source`: one renamed file.
///
/// The mock echoes the source, so a route test sees the source that the
/// route sent.
pub fn mock_diffset_for(
    source: crucible_core::diff::DiffsetSource,
) -> crucible_core::diff::Diffset {
    use crucible_core::diff::{DiffFileEntry, Diffset, FileStatus};
    use crucible_core::session::PhysicalRoot;
    Diffset {
        id: source.id(),
        source,
        files: vec![DiffFileEntry {
            root: PhysicalRoot::from_top_level("/tmp/test-project"),
            path: "new.md".to_string(),
            status: FileStatus::Renamed {
                from: "old.md".to_string(),
            },
            added: 2,
            removed: 1,
            binary: false,
            too_large: false,
        }],
        unreadable_roots: Vec::new(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// The texts `diff.file` answers for `request`.
///
/// The base text is the old path, or else the root. The current text is the
/// path. Thus a route test sees the paths that the route sent.
pub fn mock_diff_file_text_for(request: &DiffFileRequest) -> crucible_core::diff::DiffFileText {
    crucible_core::diff::DiffFileText {
        base_text: request
            .from
            .clone()
            .or_else(|| request.root.as_ref().map(|r| r.display().to_string())),
        current_text: Some(request.path.clone()),
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// The comment `diff.comment` answers for `request`.
///
/// The mock echoes the request into the comment, so a route test sees the
/// source, the side, the range and the author that the route sent.
pub fn mock_diff_comment_for(request: &DiffCommentRequest) -> DiffCommentReply {
    use crucible_core::session::{Comment, CommentAnchor, CommentAuthor, LineRange, PhysicalRoot};
    let diffset = request.source.id();
    let comment = Comment::new(
        diffset.clone(),
        CommentAnchor::Commit("0".repeat(40)),
        request
            .root
            .clone()
            .unwrap_or_else(|| PhysicalRoot::from_top_level("/tmp/test-project")),
        request.path.clone(),
        request.side,
        LineRange::new(
            request.line_start,
            request.line_end.unwrap_or(request.line_start + 1),
        ),
        "quoted\n",
        request.body.clone(),
        request.author.unwrap_or(CommentAuthor::Human),
    );
    DiffCommentReply { diffset, comment }
}

#[cfg(any(test, feature = "test-utils"))]
/// The id of the proposal that the mock daemon lists.
pub fn mock_proposal_id() -> crucible_core::proposal::ProposalId {
    "7a1c2f3e-0000-4000-8000-000000000001"
        .parse()
        .expect("a valid UUID")
}

#[cfg(any(test, feature = "test-utils"))]
/// A proposal of the mock daemon, with `id` and `state`: one new note.
pub fn mock_proposal_for(
    id: crucible_core::proposal::ProposalId,
    state: crucible_core::proposal::ProposalState,
) -> crucible_core::proposal::Proposal {
    use crucible_core::file_write::ExpectedBase;
    use crucible_core::proposal::{Proposal, ProposalAuthor, ProposedWrite};
    use crucible_core::session::PhysicalRoot;
    Proposal {
        id,
        author: ProposalAuthor::Plugin {
            name: "reflection".to_string(),
        },
        session: None,
        title: "Change notes/a.md".to_string(),
        rationale: None,
        created_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).expect("a valid time"),
        state,
        writes: vec![ProposedWrite {
            root: PhysicalRoot::from_top_level("/tmp/test-kiln"),
            path: "notes/a.md".to_string(),
            base: ExpectedBase::Absent,
            new_text: "new\n".to_string(),
            remove: false,
            moved_from: None,
        }],
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// `proposal`, with a title that names `paths` when a decision covers only
/// some of the files. Thus a route test sees the paths that the route sent.
fn titled_proposal(mut proposal: crucible_core::proposal::Proposal, paths: &[String]) -> Value {
    if !paths.is_empty() {
        proposal.title = format!("Change {}", paths.join(", "));
    }
    as_rpc_result(proposal)
}

#[cfg(any(test, feature = "test-utils"))]
/// The typed params of a mock request. A route that sends a wrong shape
/// fails the test here.
fn mock_params<T: serde::de::DeserializeOwned>(method: RpcMethod, msg: &Value) -> T {
    serde_json::from_value(msg["params"].clone()).unwrap_or_else(|e| panic!("{method} params: {e}"))
}

#[cfg(any(test, feature = "test-utils"))]
/// A fixture as the mock daemon puts it on the wire.
fn as_rpc_result<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).expect("a mock reply serialises")
}

#[cfg(any(test, feature = "test-utils"))]
/// The canned reply of the mock daemon for `method`.
///
/// The `match` is exhaustive over [`RpcMethod`] and has no wildcard arm. Thus a
/// new method does not compile until this mock gives it a reply, or names it
/// in the `null` arm at the end.
#[deny(clippy::wildcard_enum_match_arm)]
#[deny(clippy::match_wildcard_for_single_variants)]
async fn mock_rpc_response(method: RpcMethod, msg: &Value) -> Value {
    match method {
        // Two rows, because `handle_kiln_list` writes two kinds: a registry
        // entry, named and attachable, and an open directory no entry names,
        // which carries `registered: false` and the empty string. Both paths
        // are deliberately absent from disk, so every containment check that
        // canonicalizes still finds no root and the file routes behave as
        // they did when this answered nothing.
        RpcMethod::KilnList => json!([
            {
                "path": MOCK_DAEMON_KILN_PATH,
                "name": "daemon-kiln",
                "registered": true,
                "open": true,
                "last_access_secs_ago": 12,
                "git": true,
            },
            {
                "path": "/daemon/unnamed",
                "name": "",
                "registered": false,
                "open": true,
                "last_access_secs_ago": 0,
                "git": false,
            },
        ]),
        RpcMethod::KilnGraph => json!({
            "notes": [
                { "path": "Alpha.md", "title": "Alpha", "tags": ["rust"] },
                { "path": "Beta.md", "title": "Beta", "tags": [] }
            ],
            "links": [
                { "source": "Alpha.md", "target": "Beta.md", "resolved": true },
                { "source": "Alpha.md", "target": "ghost", "resolved": false }
            ]
        }),
        // The daemon's `NoteListRow`, all six fields. One note carries a
        // title, tags and frontmatter; the other carries none of them, so a
        // reader that treats an absent title as a missing key fails here.
        RpcMethod::ListNotes => json!([
            {
                "name": "Kilns",
                "path": "notes/kilns.md",
                "title": "Kilns",
                "tags": ["knowledge"],
                "updated_at": "2026-01-01T00:00:00Z",
                "properties": { "status": "draft" },
            },
            {
                "name": "Untitled",
                "path": "notes/untitled.md",
                "title": Value::Null,
                "tags": [],
                "updated_at": Value::Null,
                "properties": {},
            },
        ]),
        // Note name "missing" resolves to nothing (the 404 path); anything
        // else resolves to a note with both link spellings the daemon writes.
        RpcMethod::GetNoteByName => {
            if param_str(msg, "name") == "missing" {
                Value::Null
            } else {
                json!({
                    "path": "notes/kilns.md",
                    "title": "Kilns",
                    "tags": ["knowledge"],
                    "links_to": ["Projects"],
                    "wikilinks": [{ "target": "Projects" }],
                    "content_hash": "a".repeat(64),
                })
            }
        }
        // Note name "missing" resolves to nothing (404 path); anything else
        // resolves to a focused note with one linked mention.
        RpcMethod::GetBacklinks => {
            let name = msg
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if name == "missing" {
                Value::Null
            } else {
                json!({
                    "path": "notes/focused.md",
                    "title": "Focused Note",
                    "backlinks": [
                        {"name": "linker", "path": "notes/linker.md", "title": "Linker Note"}
                    ]
                })
            }
        }
        // Includes a self-mention ("Focused Note") that the backlinks route
        // must filter out of `unlinked`.
        RpcMethod::SuggestLinks => json!({
            "suggestions": [
                {"mention": "Other Note", "target": "Other Note", "offset": 0},
                {"mention": "Focused Note", "target": "Focused Note", "offset": 20}
            ]
        }),
        RpcMethod::NoteUpsert => json!({}),
        RpcMethod::EmbedQuery => json!({ "vector": [0.1, 0.2, 0.3] }),
        // Three block hits, best first. Two of them sit in one note, so the
        // semantic route must fold them into one row.
        RpcMethod::SearchVectors => json!([
            {
                "document_id": "notes/kilns.md",
                "score": 0.91,
                "block": { "span_start": 40, "span_end": 90, "kind": "paragraph" },
                "snippet": "A kiln is where knowledge goes."
            },
            {
                "document_id": "notes/kilns.md",
                "score": 0.83,
                "block": { "span_start": 120, "span_end": 170, "kind": "paragraph" },
                "snippet": "A session attaches a flat set of kilns."
            },
            {
                "document_id": "notes/projects.md",
                "score": 0.70,
                "block": { "span_start": 0, "span_end": 30, "kind": "heading" },
                "snippet": "Projects"
            }
        ]),
        RpcMethod::SearchGrep => json!({
            "hits": [
                {
                    "path": "/tmp/test-kiln/a.md",
                    "rel_path": "a.md",
                    "line": 3,
                    "text": "a needle here",
                    "match_start": 2,
                    "match_end": 8
                }
            ],
            "truncated": false
        }),
        // Sentinel session_type "__no_session_id__" yields a create response
        // WITHOUT a session_id, to exercise the protocol-drift guard.
        RpcMethod::SessionCreate => {
            // The wire field is `type` (SessionCreateRequest renames it).
            let session_type = msg
                .get("params")
                .and_then(|p| p.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if session_type == "__no_session_id__" {
                json!({})
            } else {
                // The daemon's own projection (`server/session/create.rs`):
                // the full `SessionSummary` — every required field has a
                // real value the moment the record exists, `event_count: 0`
                // included.
                json!({
                    "session_id": "test-session-001",
                    "type": "chat",
                    "kilns": ["test-kiln"],
                    "workspace": "/tmp/test-kiln",
                    "state": "active",
                    "started_at": "2026-01-01T00:00:00Z",
                    "event_count": 0,
                    "archived": false,
                    "agent_model": "ollama:llama3.2",
                })
            }
        }
        // Two slots from two different plugins — one the repo ships, one the
        // web has never heard of — so a route test can prove the status
        // channel is generic rather than oci-shaped. Session id
        // "quiet-session" has published nothing (the daemon answers an empty
        // array for an unknown session, never an error).
        //
        // `progress` is always written by the real daemon
        // (`server/plugins.rs`), never omitted: `null` for a state slot,
        // a fraction for one mid-work. "oci" mirrors the state case, "weather"
        // the fraction case, matching how each is described above.
        RpcMethod::SessionStatus => {
            let session_id = msg
                .get("params")
                .and_then(|p| p.get("session_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if session_id == "quiet-session" {
                json!({"status": []})
            } else {
                json!({"status": [
                    {"id": "plugin_turns:goal", "plugin": "goal", "text": "goal · ask", "progress": null, "color_group": "warn", "priority": 0, "action": "plugin_approval", "pinned": true, "kind": "plugin_turns"},
                    {"id": "oci", "plugin": "oci", "text": "sandboxed: alpine:latest", "progress": null, "color_group": "hue-4", "priority": 30, "action": null, "pinned": false, "kind": "published"},
                    {"id": "weather", "plugin": "weather", "text": "storm warning", "progress": 0.6, "color_group": "warn", "priority": 80, "action": null, "pinned": false, "kind": "published"},
                ]})
            }
        }
        // `{sessions, total}`, as `server/session/list.rs:145` builds it.
        RpcMethod::SessionList => json!({"sessions": [], "total": 0}),
        // The daemon's own projection (`server/session/list.rs:302`): the wire
        // name is `type`, the model is nested under `agent`, and
        // `event_count`, `last_activity` and `archived` are absent — all three
        // of which `session.list` sends.
        RpcMethod::SessionGet => json!({
            "session_id": "test-session-001",
            "plugin_turn_limit": 5,
            "type": "chat",
            "kilns": ["test-kiln"],
            "workspace": "/tmp/test-kiln",
            "state": "active",
            "started_at": "2026-01-01T00:00:00Z",
            "event_count": 0,
            "archived": false,
            "title": null,
            "continued_from": null,
            "parent_session_id": null,
            "agent": {
                "agent_type": "internal",
                "provider": "ollama",
                "model": "ollama:llama3.2",
                "system_prompt": "",
                "precognition_enabled": true,
                "context_strategy": "Truncate"
            }
        }),
        // `server/session/lifecycle.rs:13` and `:33` answer the state change,
        // `:116` answers the session's kilns instead of a previous state.
        RpcMethod::SessionPause => json!({
            "session_id": "test-session-001",
            "previous_state": "active",
            "state": "paused"
        }),
        RpcMethod::SessionResume => json!({
            "session_id": "test-session-001",
            "previous_state": "paused",
            "state": "active"
        }),
        RpcMethod::SessionEnd => json!({
            "session_id": "test-session-001",
            "state": "ended",
            "kilns": ["test-kiln"]
        }),
        RpcMethod::SessionCancel => json!({"cancelled": true}),
        RpcMethod::SessionClear => json!({"session_id": "test-session-001"}),
        RpcMethod::SessionDelete => json!({"deleted": true}),
        RpcMethod::SessionArchive => json!({"archived": true}),
        RpcMethod::SessionUnarchive => json!({"archived": false}),
        RpcMethod::SessionSubscribe => json!(null),
        RpcMethod::SessionConfigureAgent => json!(null),
        RpcMethod::SessionSendMessage => json!({
            "session_id": "test-session-001",
            "outcome": "turn",
            "message_id": "msg-001"
        }),
        // The built-in commands, then one plugin command.
        RpcMethod::SessionCommands => {
            let mut commands: Vec<Value> = crucible_core::types::BuiltinCommand::entries()
                .into_iter()
                .map(|c| serde_json::to_value(c).unwrap())
                .collect();
            commands.push(json!({
                "name": "reflect", "description": "Run a reflection pass",
                "kind": "plugin", "plugin": "alpha"
            }));
            json!({ "session_id": "test-session-001", "commands": commands })
        }
        RpcMethod::SessionUndo => json!({ "undone": [] }),
        RpcMethod::SessionInteractionRespond => json!(null),
        RpcMethod::SessionListModels => json!({"models": ["llama3.2", "mistral"]}),
        RpcMethod::SessionKnobSet => json!(null),
        // Echoes a canned value per knob, keyed by the request's own `knob`
        // field, so a route test reads back the same shape
        // `session.knob.set` would have written.
        RpcMethod::SessionKnobGet => {
            let request: crucible_core::protocol::requests::Scoped<
                crucible_core::protocol::requests::KnobRef,
            > = mock_params(method, msg);
            let value = match request.body.knob {
                crucible_core::types::SessionKnob::Model => {
                    crucible_core::types::KnobValue::Model("llama3.2".to_string())
                }
                crucible_core::types::SessionKnob::Mode => {
                    crucible_core::types::KnobValue::Mode(Some("plan".to_string()))
                }
                crucible_core::types::SessionKnob::ContextStrategy => {
                    crucible_core::types::KnobValue::ContextStrategy("recent".to_string())
                }
                crucible_core::types::SessionKnob::Precognition => {
                    crucible_core::types::KnobValue::Precognition(true)
                }
                crucible_core::types::SessionKnob::PluginTurnLimit => {
                    crucible_core::types::KnobValue::PluginTurnLimit(25)
                }
            };
            as_rpc_result(value)
        }
        // `{knobs: [{id, supported}]}`, one entry per `SessionKnob::ALL`
        // member. The route answered `null` here until it named its reply.
        RpcMethod::SessionListKnobs => json!({"knobs": [
            {"id": "model", "supported": true},
            {"id": "mode", "supported": true},
            {"id": "context_strategy", "supported": true},
            {"id": "precognition", "supported": true},
        ]}),
        RpcMethod::SessionListModes => json!({
            "session_id": "test-session-001",
            "current_mode_id": "ask",
            "modes": [
                {"id": "ask", "name": "Ask", "description": "Ask before each change",
                 "icon": null, "color": null},
                {"id": "plan", "name": "Plan", "description": "Read-only exploration mode",
                 "icon": null, "color": null},
                {"id": "propose", "name": "Propose", "description": "Propose note changes for review",
                 "icon": null, "color": null, "writes": "propose"},
            ],
        }),
        RpcMethod::SessionSetTitle => json!(null),
        RpcMethod::SessionGenerateTitle => json!({
            "session_id": "test-session-001",
            "title": "Merkle tree sync design"
        }),
        // `{matches, total}` of transcript LINES, not of sessions
        // (`server/session/list.rs:277`). A search with no kiln scope searched
        // nothing, and the daemon says so in a `note` rather than answering a
        // bare empty list (`:206`).
        RpcMethod::SessionSearch => {
            let scoped = msg
                .get("params")
                .and_then(|p| p.get("kilns"))
                .and_then(|v| v.as_array())
                .is_some_and(|kilns| !kilns.is_empty());
            let query = msg
                .get("params")
                .and_then(|p| p.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if scoped && query == "two hits" {
                // A query naming its own fixture, so the one-match case above
                // stays untouched for every other test that scopes a search.
                json!({
                    "matches": [
                        {"session_id": "s1", "line": 12, "context": "Test Session one"},
                        {"session_id": "s2", "line": 0, "context": "[active] Test Session two"}
                    ],
                    "total": 2
                })
            } else if scoped {
                json!({
                    "matches": [{"session_id": "s1", "line": 12, "context": "Test Session"}],
                    "total": 1
                })
            } else {
                json!({
                    "matches": [],
                    "total": 0,
                    "note": "Specify 'kilns' to scope the search to sessions that share one"
                })
            }
        }
        // Mirrors the daemon's real response shape: a `history` array of
        // SessionEventMessage entries, NOT a `messages` array. Session id
        // "empty-session-001" yields an empty history for fallback tests.
        RpcMethod::SessionResumeFromStorage | RpcMethod::SessionHistory => {
            let mut reply = mock_history_reply(msg);
            // The daemon folds the stored log; the mock folds its own with
            // the same core fold.
            let events: Vec<crucible_core::protocol::SessionEventMessage> =
                serde_json::from_value(reply["history"].clone()).unwrap_or_default();
            reply["transcript"] = serde_json::to_value(
                crucible_core::transcript::TranscriptFold::of_events(&events),
            )
            .expect("a transcript serializes");
            reply
        }
        // The wire envelopes `session.events_after` replays for
        // "test-session-001": a two-turn transcript, seqs 1-4, filtered by the
        // caller's cursor exactly as the daemon's reader does. Other sessions
        // answer the same empty tail an unknown id does.
        RpcMethod::SessionEventsAfter => {
            let session_id = msg
                .get("params")
                .and_then(|p| p.get("session_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let after = msg
                .get("params")
                .and_then(|p| p.get("after"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if session_id == "test-session-001" {
                let log = [
                    json!({
                        "type": "event", "session_id": session_id, "event": "user_message",
                        "data": {"message_id": "msg-001", "content": "First turn"},
                        "timestamp": "2026-01-01T00:00:00Z", "seq": 1,
                    }),
                    json!({
                        "type": "event", "session_id": session_id, "event": "message_complete",
                        "data": {"message_id": "msg-001", "full_response": "First answer"},
                        "timestamp": "2026-01-01T00:00:01Z", "seq": 2,
                    }),
                    json!({
                        "type": "event", "session_id": session_id, "event": "user_message",
                        "data": {"message_id": "msg-002", "content": "Second turn"},
                        "timestamp": "2026-01-01T00:00:02Z", "seq": 3,
                    }),
                    json!({
                        "type": "event", "session_id": session_id, "event": "message_complete",
                        "data": {"message_id": "msg-002", "full_response": "Second answer"},
                        "timestamp": "2026-01-01T00:00:03Z", "seq": 4,
                    }),
                ];
                Value::Array(
                    log.into_iter()
                        .filter(|e| e["seq"].as_u64().is_some_and(|seq| seq > after))
                        .collect(),
                )
            } else {
                json!([])
            }
        }
        RpcMethod::ProjectList => as_rpc_result(vec![mock_project()]),
        RpcMethod::DiffGet => {
            let request: DiffsetRef =
                serde_json::from_value(msg["params"].clone()).expect("diff.get params");
            as_rpc_result(mock_diffset_for(request.source))
        }
        RpcMethod::DiffFile => {
            let request: DiffFileRequest =
                serde_json::from_value(msg["params"].clone()).expect("diff.file params");
            as_rpc_result(mock_diff_file_text_for(&request))
        }
        RpcMethod::DiffComment => {
            let request: DiffCommentRequest =
                serde_json::from_value(msg["params"].clone()).expect("diff.comment params");
            as_rpc_result(mock_diff_comment_for(&request))
        }
        RpcMethod::DiffResolveComment => {
            let request: DiffCommentKey =
                serde_json::from_value(msg["params"].clone()).expect("diff.resolve_comment params");
            json!({
                "diffset": request.source.id(),
                "comment_id": request.comment_id,
                "resolved": true,
            })
        }
        RpcMethod::DiffDeleteComment => {
            let request: DiffCommentKey =
                serde_json::from_value(msg["params"].clone()).expect("diff.delete_comment params");
            json!({
                "diffset": request.source.id(),
                "comment_id": request.comment_id,
                "deleted": true,
            })
        }
        // One comment whose quoted text is gone: the `outdated` flag reaches
        // the browser only if the route keeps it.
        RpcMethod::DiffComments => {
            let request: DiffsetRef =
                serde_json::from_value(msg["params"].clone()).expect("diff.comments params");
            json!({
                "diffset": request.source.id(),
                "comments": [{
                    "comment": review_comment_fixture("comment-1", "why this?"),
                    "outdated": true,
                }],
            })
        }
        // The proposal answers echo the id, the reason and the paths of the
        // request, so a route test sees what the route sent.
        RpcMethod::ProposalList => {
            use crucible_core::proposal::{ProposalId, ProposalState};
            let request: crucible_core::protocol::requests::ProposalListRequest =
                mock_params(method, msg);
            let mut listed = vec![mock_proposal_for(mock_proposal_id(), ProposalState::Open)];
            if request.all {
                listed.push(mock_proposal_for(
                    ProposalId::generate(),
                    ProposalState::Dismissed,
                ));
            }
            as_rpc_result(listed)
        }
        RpcMethod::ProposalGet => {
            let request: crucible_core::protocol::requests::ProposalIdRequest =
                mock_params(method, msg);
            as_rpc_result(mock_proposal_for(
                request.id,
                crucible_core::proposal::ProposalState::Open,
            ))
        }
        RpcMethod::ProposalAccept => {
            let request: crucible_core::protocol::requests::ProposalAcceptRequest =
                mock_params(method, msg);
            titled_proposal(
                mock_proposal_for(request.id, crucible_core::proposal::ProposalState::Accepted),
                &request.paths,
            )
        }
        RpcMethod::ProposalDismiss => {
            let request: crucible_core::protocol::requests::ProposalIdRequest =
                mock_params(method, msg);
            as_rpc_result(mock_proposal_for(
                request.id,
                crucible_core::proposal::ProposalState::Dismissed,
            ))
        }
        RpcMethod::ProposalReject => {
            let request: crucible_core::protocol::requests::ProposalRejectRequest =
                mock_params(method, msg);
            titled_proposal(
                mock_proposal_for(
                    request.id,
                    crucible_core::proposal::ProposalState::Rejected {
                        reason: request.reason,
                    },
                ),
                &request.paths,
            )
        }
        RpcMethod::ProposalResolve => {
            let request: crucible_core::protocol::requests::ProposalResolveRequest =
                mock_params(method, msg);
            as_rpc_result(mock_proposal_for(
                request.id,
                crucible_core::proposal::ProposalState::Accepted,
            ))
        }
        RpcMethod::FsListDir => as_rpc_result(mock_fs_listing()),
        RpcMethod::FsMove => as_rpc_result(mock_fs_move_reply()),
        RpcMethod::FsMkdir => json!({"created": true}),
        RpcMethod::FsTrash => as_rpc_result(mock_fs_trash_reply()),
        RpcMethod::ScmClone => as_rpc_result(mock_scm_clone()),
        RpcMethod::ProjectRegister => as_rpc_result(mock_project()),
        RpcMethod::ProjectUnregister => json!(null),
        // The daemon answers null for a path no project is registered for, so
        // the mock answers for ITS project and null for anything else. A mock
        // that answered for every path would make the route's 404 untestable.
        RpcMethod::ProjectGet => {
            let asked = msg
                .get("params")
                .and_then(|p| p.get("path"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let project = mock_project();
            if asked == project.path.to_string_lossy() {
                as_rpc_result(project)
            } else {
                Value::Null
            }
        }
        RpcMethod::SessionConnectKiln => json!({
            "session_id": "test-session-001",
            "kilns": ["test-kiln", "extra-kiln"],
            "workspace": "/tmp/test-kiln",
        }),
        RpcMethod::SessionDisconnectKiln => json!({
            "session_id": "test-session-001",
            "kilns": ["test-kiln"],
            "workspace": "/tmp/test-kiln",
        }),
        RpcMethod::SessionSetWorkspace => {
            let workspace = msg
                .get("params")
                .and_then(|p| p.get("workspace"))
                .and_then(|v| v.as_str())
                .unwrap_or("/tmp/test-kiln");
            json!({
                "session_id": "test-session-001",
                "kilns": ["test-kiln"],
                "workspace": workspace,
            })
        }
        // The settings the session's external agent advertised for itself, as
        // `server/session/modes.rs` sends them: one option per control shape,
        // because the kind tag and the value belong together and a client that
        // read `current` without it would draw the wrong control.
        RpcMethod::SessionListAgentOptions => json!({
            "session_id": param_str(msg, "session_id"),
            "options": [
                {
                    "id": "reasoning",
                    "name": "Reasoning effort",
                    "description": "How long the agent thinks",
                    "category": "model",
                    "kind": "select",
                    "current": "medium",
                    "choices": [
                        {"value": "medium", "name": "Medium"},
                        {"value": "high", "name": "High"},
                    ],
                },
                {
                    "id": "web_search",
                    "name": "Web search",
                    "description": Value::Null,
                    "category": Value::Null,
                    "kind": "toggle",
                    "current": false,
                },
            ],
        }),
        RpcMethod::SessionSetAgentOption => json!({ "ok": true }),
        RpcMethod::SessionSetPluginApproval => json!({"plugin": "alpha", "approval": "ask"}),
        RpcMethod::SessionGetPluginApproval => json!({"plugin": "alpha", "approval": "ask"}),
        RpcMethod::SessionListPluginApprovals => {
            json!({"approvals": {"alpha": "ask", "beta": "stop"}})
        }
        RpcMethod::SessionRenderMarkdown => {
            json!({"markdown": "# Test Session\n\nExported content"})
        }
        RpcMethod::ProvidersList => json!({"providers": []}),
        RpcMethod::ModelsList => json!({"models": ["ollama/llama3.2", "openai/gpt-4o"]}),
        // SERIALISED from the daemon's own reply type, never hand-written, so
        // a route test that reads the rows back is a round trip rather than an
        // agreement between this file and the route.
        RpcMethod::AgentsListProfiles => as_rpc_result(crucible_daemon::AgentProfilesReply {
            profiles: vec![
                crucible_daemon::AgentProfileEntry {
                    name: "claude".to_string(),
                    description: "Claude Code via ACP".to_string(),
                    command: "npx".to_string(),
                    is_builtin: true,
                    available: false,
                },
                crucible_daemon::AgentProfileEntry {
                    name: "opencode".to_string(),
                    description: "OpenCode AI (Go)".to_string(),
                    command: "opencode".to_string(),
                    is_builtin: true,
                    available: true,
                },
            ],
        }),
        // Name "missing" is unknown (null); anything else resolves.
        RpcMethod::AgentsResolveProfile => {
            let name = msg
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if name == "missing" {
                Value::Null
            } else {
                json!({
                    "name": name,
                    "description": "Mock ACP agent",
                    "command": "mock-agent",
                    "is_builtin": true,
                    "args": [],
                    "env": {},
                })
            }
        }
        // The three app-config methods `/api/config` forwards to. The
        // effective config carries one leaf a human's `init.lua` holds, and
        // `config.origin` reports that same leaf, so a route test can see a
        // value and its provenance travel together.
        RpcMethod::ConfigEffective => json!({
            "config": {
                "kiln_path": MOCK_DAEMON_KILN_PATH,
                "chat": { "model": "daemon-model" },
            },
            "config_root": "/daemon/config",
            "boot_hash": "mock-boot-hash",
            "kiln_path_is_default": false,
            // SERIALISED from a real value, never hand-written. The wire form
            // of a `ConfigSource` is the only thing that told the CLI where a
            // leaf came from, and a literal here could not drift with the
            // type — which is how one variant came to spell itself
            // `plugin_default` on this wire while `short()` said `plugin`.
            "provenance": { "chat.model": mock_lua_provenance() },
        }),
        // A stand-in tree, not the daemon's real one: this crate does not
        // link the Lua VM that owns it, and the route only forwards. The
        // group name is a sentinel, so the route test sees the daemon's answer
        // travel rather than a shape this file and the route agreed on.
        RpcMethod::ConfigControls => json!({
            "options": {
                "type": "group",
                "name": "Crucible",
                "args": [{
                    "key": "chat",
                    "path": "chat",
                    "type": "group",
                    "name": "Chat",
                    "order": 10,
                    "args": [{
                        "key": "model",
                        "path": "chat.model",
                        "type": "input",
                        "name": "Model",
                        "desc": "Default model for a new session.",
                        "order": 1,
                        "default": "",
                        "writable": true,
                    }],
                }],
            },
            "read_only": [{ "path": "data_home", "reason": MOCK_LOCATION_REASON }],
        }),
        // SERIALISED from the daemon's own row type. A literal here had no
        // `pinned` key, which the daemon always writes, so a route test could
        // not have noticed the route dropping the whole row.
        RpcMethod::ConfigOrigin => json!({
            "origins": [as_rpc_result(crucible_daemon::ConfigOriginRow {
                key: "chat.model".to_string(),
                value: json!("daemon-model"),
                origin: crucible_core::config::LeafOrigin {
                    pinned: true,
                    origin: crucible_core::config::SourceOrigin {
                        source: "lua".to_string(),
                        file: Some(MOCK_PIN_FILE.to_string()),
                        line: Some(12),
                    },
                },
            })],
        }),
        // Which leaves a save may write is the daemon's rule, not this mock's.
        // A top-level [`MOCK_PINNED_KEY`] is the sentinel for "a human's line
        // holds this", so a route test sees a refusal envelope without a
        // second implementation of the layering rule living here.
        RpcMethod::ConfigSave => {
            let pinned = msg
                .get("params")
                .and_then(|p| p.get("values"))
                .and_then(|values| values.get(MOCK_PINNED_KEY))
                .is_some();
            if pinned {
                json!({
                    "ok": false,
                    "rejected": [],
                    "refused": [{
                        "key": format!("{MOCK_PINNED_KEY}.leaf"),
                        "source": "lua",
                        "file": MOCK_PIN_FILE,
                        "line": 12,
                    }],
                })
            } else {
                json!({ "ok": true, "refused": [], "rejected": [] })
            }
        }
        // Echoes the `key` it was asked for, so a contract test can prove the
        // route's `?key=` actually reaches the daemon rather than being
        // dropped and filtered client-side.
        // `effect` is written by every row `commands_json` builds — an
        // undeclared command arrives as `write` rather than as nothing — so
        // the fixture writes it too. Without it the route's reply struct
        // could not be exercised at all.
        RpcMethod::PluginCommands => json!({
            "commands": [{
                "plugin": "mock-plugin",
                "name": "mock_command",
                "description": "A mock command",
                "hint": "<arg>",
                "parameters": [
                    { "name": "target", "type": "string", "desc": "What to act on" },
                    { "name": "count", "type": "number", "desc": "How many", "optional": true },
                ],
                "effect": "write",
            }]
        }),
        RpcMethod::PluginPublications => {
            let key = msg
                .get("params")
                .and_then(|p| p.get("key"))
                .and_then(|k| k.as_str());
            match key {
                Some(k) => {
                    json!({ "publications": { k: { "mock-plugin": { "narrowed": true } } } })
                }
                None => json!({
                    "publications": {
                        "everything": { "mock-plugin": { "narrowed": false } },
                        "and-more": { "other-plugin": { "narrowed": false } },
                    }
                }),
            }
        }
        RpcMethod::PluginList => json!({
            "plugins": ["mock-plugin"],
            "plugin_info": [{
                "name": "mock-plugin",
                "version": "0.1.0",
                "source": "User",
                "state": "Active",
                // Written for every row, so "broken" stays distinguishable
                // from "not installed". Null is a healthy plugin.
                "last_error": Value::Null,
                "dir": "/tmp/mock-plugin",
                "tools": 3,
                "commands": 1,
                "handlers": 2,
                "services": 0,
            }],
            "errors": [],
            "spec": [],
        }),
        // Shaped like a real tree so the contract test exercises pass-through
        // rather than a hand-built stub: a group, a leaf, and a button.
        RpcMethod::PluginOptions => json!({
            "options": {
                "mock-plugin": {
                    "type": "group",
                    "name": "Mock",
                    "order": 100,
                    "args": [
                        {
                            "key": "image", "type": "input", "name": "Image",
                            "order": 1, "writable": true,
                        },
                        {
                            "key": "cleanup", "type": "execute", "name": "Clean up",
                            "order": -1,
                        },
                    ],
                },
            },
        }),
        // Shaped as `surface_json` writes it: one panel, two rows, one of
        // them marked. The route answered nothing for this method before, so
        // the fallthrough `null` read back as an empty list and no contract
        // test could see a row at all.
        RpcMethod::SurfaceList => json!({
            "surfaces": [{
                "plugin": "mock-plugin",
                "name": "sessions",
                "title": "Sessions",
                "shape": "list",
                "session": Value::Null,
                "version": 3,
                "rows": [
                    { "id": "a", "text": "first", "detail": "and more", "mark": "busy" },
                    { "id": "b", "text": "second", "detail": Value::Null, "mark": Value::Null },
                ],
            }]
        }),
        // Echoes the name it was asked for, beside an opaque result, exactly
        // as `handle_plugin_run_command` does.
        RpcMethod::PluginRunCommand => json!({
            "name": param_str(msg, "name"),
            "result": { "branches": ["main", "next"] },
        }),
        RpcMethod::PluginOptionGet => json!({ "value": "alpine" }),
        RpcMethod::PluginOptionSet => json!({ "ok": true }),
        RpcMethod::PluginOptionExecute => json!({ "ok": true }),
        RpcMethod::PluginReload => json!({
            "name": "mock-plugin",
            "reloaded": true,
            "tools": 3,
            "commands": 1,
            "handlers": 2,
            "services": 0,
        }),
        RpcMethod::PluginInstall => json!({
            "name": "installed-plugin",
            "outcome": { "kind": "cloned", "dest": "/tmp/installed-plugin" },
            "manifest": "/tmp/plugins.installed.json",
            "installed": true,
            "loaded": true,
            "tools": 0,
            "commands": 0,
            "services": 0,
            "error": Value::Null,
            "watch": "not hot-watched until restart",
        }),
        RpcMethod::PluginRemove => json!({
            "name": "removed-plugin",
            "manifest": "/tmp/plugins.installed.json",
            "purge_error": Value::Null,
            "kept_dir": Value::Null,
            "purged_dir": Value::Null,
        }),
        RpcMethod::SkillsList => as_rpc_result(crucible_daemon::SkillsReply {
            skills: vec![crucible_daemon::SkillSummary {
                name: "test-skill".to_string(),
                scope: "user".to_string(),
                description: "A test skill".to_string(),
                shadowed_count: 0,
            }],
        }),
        RpcMethod::SkillsGet => as_rpc_result(crucible_daemon::SkillDetail {
            name: "test-skill".to_string(),
            scope: "user".to_string(),
            description: "A test skill".to_string(),
            source_path: "/tmp/skill.md".to_string(),
            agent: None,
            license: None,
            body: "# Test Skill\n\nContent.".to_string(),
        }),
        RpcMethod::SkillsSearch => as_rpc_result(crucible_daemon::SkillsReply {
            skills: vec![crucible_daemon::SkillSummary {
                name: "matched-skill".to_string(),
                scope: "user".to_string(),
                description: "Matched".to_string(),
                shadowed_count: 0,
            }],
        }),
        // A daemon with no MCP server running. The stopped arm writes ONE key,
        // which is what makes the route test's "and nothing else" assertion
        // mean something.
        RpcMethod::McpStatus => as_rpc_result(crucible_daemon::McpStatus::Stopped(
            crucible_daemon::McpStopped { running: false },
        )),
        RpcMethod::WebhookReceive => as_rpc_result(crucible_daemon::WebhookReceiveReply {
            status: "ok".to_string(),
        }),
        // One permission request, in the daemon's own wire shape, so the web
        // route's normalisation is exercised on a real `InteractionRequest`
        // rather than on a shape this file invented.
        RpcMethod::SessionPendingInteractions => json!({
            "pending": [
                {
                    "session_id": "session-001",
                    "request_id": "req-001",
                    "request": {
                        "kind": "permission",
                        "action": { "type": "bash", "tokens": ["ls"] },
                    },
                }
            ]
        }),

        // The mock holds no kiln and no project, so every path is outside
        // every root. The reader and the writer of the daemon give that
        // refusal. A test that needs a real root uses
        // [`start_real_daemon_with_kilns`].
        RpcMethod::FsRead => {
            let request = serde_json::from_value(msg["params"].clone()).unwrap();
            crucible_daemon::file_write::read_for_roots(request, &[], &[]).await
        }
        RpcMethod::FsWrite => {
            let request = serde_json::from_value(msg["params"].clone()).unwrap();
            crucible_daemon::file_write::write_for_roots(request, &[], &[]).await
        }
        // No route test reads a reply of these methods. The mock answers
        // `null` for each one. To give one a reply, move its variant to an
        // arm above.
        RpcMethod::Ping
        | RpcMethod::DaemonCapabilities
        | RpcMethod::Shutdown
        | RpcMethod::KilnOpen
        | RpcMethod::KilnClose
        | RpcMethod::KilnRegister
        | RpcMethod::KilnRegistryList
        | RpcMethod::KilnForget
        | RpcMethod::LlmRegisterProvider
        | RpcMethod::SearchText
        | RpcMethod::BaseList
        | RpcMethod::BaseViews
        | RpcMethod::BaseQuery
        | RpcMethod::BaseCreateEntry
        | RpcMethod::BaseSetProperty
        | RpcMethod::BaseReorderGroups
        | RpcMethod::NoteGet
        | RpcMethod::NoteDelete
        | RpcMethod::NoteList
        | RpcMethod::ProcessFile
        | RpcMethod::ProcessBatch
        | RpcMethod::SessionCompact
        | RpcMethod::SessionUnsubscribe
        | RpcMethod::SessionCacheStats
        | RpcMethod::SessionAddNotification
        | RpcMethod::SessionListNotifications
        | RpcMethod::SessionDismissNotification
        | RpcMethod::NotificationList
        | RpcMethod::NotificationDismiss
        | RpcMethod::SessionInjectContext
        | RpcMethod::SessionTestInteraction
        | RpcMethod::SessionFork
        | RpcMethod::SessionListPersisted
        | RpcMethod::SessionExportToFile
        | RpcMethod::SessionReplay
        | RpcMethod::SessionCleanup
        | RpcMethod::SessionReindex
        | RpcMethod::SessionCanUndo
        | RpcMethod::SessionUndoDepth
        | RpcMethod::SurfaceGet
        | RpcMethod::LuaInitSession
        | RpcMethod::LuaShutdownSession
        | RpcMethod::LuaDiscoverPlugins
        | RpcMethod::LuaPluginHealth
        | RpcMethod::LuaGenerateStubs
        | RpcMethod::LuaRunPluginTests
        | RpcMethod::LuaRegisterCommands
        | RpcMethod::LuaEval
        | RpcMethod::ConfigGet
        | RpcMethod::ConfigSet
        | RpcMethod::ConfigReset
        | RpcMethod::ConfigPop
        | RpcMethod::ConfigUnset
        | RpcMethod::UiConfig
        | RpcMethod::UiSetTheme
        | RpcMethod::ProjectOpenKilns
        | RpcMethod::ProjectRegistryList
        | RpcMethod::NoteRename
        | RpcMethod::NoteMove
        | RpcMethod::StorageVerify
        | RpcMethod::StorageCleanup
        | RpcMethod::StorageBackup
        | RpcMethod::StorageRestore
        | RpcMethod::McpStart
        | RpcMethod::McpStop
        | RpcMethod::AgentsListCards
        | RpcMethod::EmbeddingsModels
        | RpcMethod::SubagentCollect
        | RpcMethod::WorkflowStart
        | RpcMethod::WorkflowApproveGate
        | RpcMethod::WorkflowStatus
        | RpcMethod::WorkflowCancel => Value::Null,
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// Build an AppState using a mock daemon client.
pub fn build_state(client: DaemonClient) -> AppState {
    build_state_with_config(client, CliAppConfig::default())
}

#[cfg(any(test, feature = "test-utils"))]
/// Build an AppState using a mock daemon client and a caller-supplied config.
/// For routes that read `state.config` rather than the daemon (`/api/config`),
/// where the default config can only ever produce empty answers.
pub fn build_state_with_config(client: DaemonClient, config: CliAppConfig) -> AppState {
    let broker = Arc::new(EventBroker::new());
    // Mock client has no live event stream; a dropped sender ends the router.
    let (_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<crucible_daemon::SessionEvent>();
    AppState {
        daemon: Arc::new(ReconnectingDaemon::new(client, event_rx, broker.clone())),
        events: broker,
        config: Arc::new(config),
        http_client: reqwest::Client::new(),
        layout_path: Arc::new(unique_test_layout_path()),
        remote_shell: false,
        swr: Arc::new(crate::services::catalog::SwrCache::default()),
        recents_lock: Arc::new(tokio::sync::Mutex::new(())),
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// Per-call unique layout path so parallel tests never share a file.
/// Layout-specific tests build their own AppState over a TempDir instead.
pub fn unique_test_layout_path() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "crucible-test-layout-{}-{}.json",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(any(test, feature = "test-utils"))]
/// Build the full app router with mock state.
pub fn build_test_app(state: AppState) -> Router {
    use crate::middleware::auth::{ApiKeyState, HostPolicy};
    use crate::server::{build_router, WebConfig};

    // Contract fixtures represent a remote bind with authentication disabled.
    // Security-composition tests inject credentials and real request headers.
    let config = WebConfig {
        host: "0.0.0.0".into(),
        port: 3000,
        ..Default::default()
    };
    let credentials = Arc::new(ApiKeyState::new_at(
        None,
        HostPolicy::from_web_config(&config).unwrap(),
        None,
    ));
    build_router(&config, state, credentials).layer(axum::middleware::from_fn(
        async |mut request: axum::extract::Request, next: axum::middleware::Next| {
            request
                .headers_mut()
                .entry(axum::http::header::HOST)
                .or_insert(axum::http::HeaderValue::from_static("localhost:3000"));
            next.run(request).await
        },
    ))
}

#[cfg(any(test, feature = "test-utils"))]
/// Drive one request through a fresh mock-daemon-backed app and decode the
/// JSON body. `body` is `None` for empty-body requests (GET/POST-without-body),
/// `Some(v)` for a JSON payload. Returns the status and the parsed body
/// (`Value::Null` when the body is empty/non-JSON). Consolidates the near-
/// identical per-module test helpers so route tests don't each re-spin the
/// mock+state+app boilerplate.
pub async fn request_json(
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (axum::http::StatusCode, Value) {
    request_json_as(method, uri, body, Vec::new()).await
}

#[cfg(any(test, feature = "test-utils"))]
/// [`request_json`], against a real daemon that holds `kilns` open.
///
/// The knowledge routes serve only a path inside an open root, so a test that
/// reads or writes a real file through one needs a kiln that holds it. The
/// plain [`request_json`] cannot supply one: its mock holds no root.
pub async fn request_json_in_kilns(
    method: &str,
    uri: &str,
    body: Option<Value>,
    kilns: Vec<PathBuf>,
) -> (axum::http::StatusCode, Value) {
    use tower::ServiceExt;

    let (_daemon, client) = start_real_daemon_with_kilns(&kilns).await;
    let app = build_test_app(build_state(client));

    let builder = axum::http::Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(axum::body::Body::empty()).unwrap(),
    };

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

#[cfg(any(test, feature = "test-utils"))]
/// [`shape`], against a real daemon that holds `kilns` open. See
/// [`request_json_in_kilns`].
pub async fn shape_in_kilns<T: serde::de::DeserializeOwned>(
    method: &str,
    uri: &str,
    body: Option<Value>,
    kilns: Vec<PathBuf>,
) -> T {
    let (status, json) = request_json_in_kilns(method, uri, body, kilns).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{method} {uri}: {json}");
    serde_json::from_value(json.clone()).unwrap_or_else(|e| {
        panic!("{method} {uri} answered a body the struct cannot read: {e}\n{json}")
    })
}

/// [`request_json`], plus the headers a route reads.
///
/// The plugin routes need one: `PluginCaller` refuses a request that declares
/// no identity, so a test that cannot set `x-crucible-plugin` can only ever see
/// their 403.
pub async fn request_json_as(
    method: &str,
    uri: &str,
    body: Option<Value>,
    headers: Vec<(&str, String)>,
) -> (axum::http::StatusCode, Value) {
    use tower::ServiceExt;

    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let app = build_test_app(state);

    let mut builder = axum::http::Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(name, value);
    }
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(axum::body::Body::empty()).unwrap(),
    };

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// Drive one request and decode the body into the reply struct `T`.
///
/// A test that reads `status == 200` proves nothing about a shape: the route
/// groups answered `serde_json::Value` until tasks A5 to A10 named their
/// replies, and a renamed field passed every such test. Decoding fails on a
/// missing or retyped field instead.
pub async fn shape<T: serde::de::DeserializeOwned>(
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> T {
    shape_as(method, uri, body, Vec::new()).await
}

/// [`shape`], plus the headers a route reads. See [`request_json_as`].
pub async fn shape_as<T: serde::de::DeserializeOwned>(
    method: &str,
    uri: &str,
    body: Option<Value>,
    headers: Vec<(&str, String)>,
) -> T {
    let (status, json) = request_json_as(method, uri, body, headers).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{method} {uri}: {json}");
    serde_json::from_value(json.clone()).unwrap_or_else(|e| {
        panic!("{method} {uri} answered a body the struct cannot read: {e}\n{json}")
    })
}

/// A reply struct reads what the daemon wrote, and writes the same thing back.
///
/// The claim that makes naming a forwarded reply safe at all. A route that
/// passed the daemon's object through verbatim could not drop a key the daemon
/// added later; a named struct can. So the caller builds the object from the
/// daemon's *own* type, reads it into the row, writes it back, and demands the
/// same JSON — which turns a silently dropped field into a failing test.
pub fn survives<T: serde::Serialize + serde::de::DeserializeOwned>(sent: &impl serde::Serialize) {
    let wire = serde_json::to_value(sent).expect("the daemon's type writes JSON");
    let row: T = serde_json::from_value(wire.clone())
        .unwrap_or_else(|e| panic!("the row cannot read what the daemon sent: {e}\n{wire}"));
    assert_eq!(
        serde_json::to_value(row).expect("the row writes JSON"),
        wire,
        "the row changed the object on the way through"
    );
}

/// The mock answer of `session.history`: an empty session, or two stored
/// events.
fn mock_history_reply(msg: &Value) -> Value {
    let session_id = msg
        .get("params")
        .and_then(|p| p.get("session_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("test-session-001");
    if session_id == "empty-session-001" {
        json!({
            "session_id": session_id,
            "type": "chat",
            "state": "active",
            "kilns": ["test-kiln"],
            "history": [],
            "total_events": 0
        })
    } else {
        json!({
            "session_id": session_id,
            "type": "chat",
            "state": "active",
            "kilns": ["test-kiln"],
            "history": [
                {
                    "type": "event",
                    "session_id": session_id,
                    "event": "user_message",
                    "data": {"message_id": "msg-001", "content": "Explain the merkle tree sync design"},
                    "timestamp": "2026-01-01T00:00:00Z",
                    "seq": 1
                },
                {
                    "type": "event",
                    "session_id": session_id,
                    "event": "agent_message",
                    "data": {"message_id": "msg-002", "content": "Sure — the merkle tree..."},
                    "timestamp": "2026-01-01T00:00:01Z",
                    "seq": 2
                }
            ],
            "total_events": 2
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn smoke_arb_traversal_path(path in arb_traversal_path()) {
            prop_assert!(path.contains("..") || path.contains('\0'));
        }

        #[test]
        fn smoke_arb_safe_path(path in arb_safe_path()) {
            prop_assert!(!path.contains(".."));
            prop_assert!(!path.contains('\0'));
        }
    }
}
