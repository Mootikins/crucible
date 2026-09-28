use super::*;
use crate::test_support::temp_session_manager;
use tempfile::TempDir;

// ── End-to-end fixture: real git worktree, real ledger ──

/// A session whose ledger tracks a one-file git repo, plus the managers
/// the handlers need. Held together because `TempDir` must outlive the
/// ledger that points at it.
struct Fixture {
    dir: TempDir,
    am: Arc<AgentManager>,
    sm: Arc<SessionManager>,
    event_tx: crate::EventBus,
}

use crate::test_support::git;

impl Fixture {
    async fn new(initial: &str) -> Self {
        use crate::agent_manager::AgentManagerParams;
        use crate::background_manager::BackgroundJobManager;
        use crate::kiln_manager::KilnManager;

        let dir = TempDir::new().unwrap();
        git(dir.path(), &["init", "-q"]).await;
        git(dir.path(), &["config", "user.email", "t@t"]).await;
        git(dir.path(), &["config", "user.name", "t"]).await;
        std::fs::write(dir.path().join("a.txt"), initial).unwrap();
        git(dir.path(), &["add", "."]).await;
        git(dir.path(), &["commit", "-q", "-m", "init"]).await;

        let (event_tx, _) = crate::EventBus::channel(64);
        let kiln_manager = Arc::new(KilnManager::new());
        let session_manager = temp_session_manager();
        let am = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager,
            session_manager: session_manager.clone(),
            background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: None,
            source_roots: Default::default(),
            review_snapshot_root: crate::test_support::scratch_snapshot_root(),
        }));

        Self {
            dir,
            am,
            sm: session_manager,
            event_tx,
        }
    }
}

/// The Lua/plugin review surface and the RPC handlers are backed by the same
/// free functions precisely so they cannot drift. They drifted here: every RPC
/// handler opens with `ensure_loaded`, and none of the five bridge methods did
/// — so a delegating agent asking a resumed session for its hunks was answered
/// `[]` with no error ("the child changed nothing") while a browser hitting the
/// same session got the record restored from `review.jsonl`.
///
/// `cru.diff.get` on the session record goes through the `diff.get` handler,
/// so it restores the record too.
#[tokio::test]
async fn the_lua_bridge_restores_a_resumed_sessions_record_like_the_handler_does() {
    use crucible_core::session::{Session, SessionType};
    use crucible_lua::DaemonSessionApi;

    let fx = Fixture::new("one\n").await;
    let _kiln = TempDir::new().unwrap();
    let session = Session::new(
        SessionType::Chat,
        vec![crate::test_support::kiln_name("kiln")],
    )
    .with_workspace(Some(fx.dir.path().to_path_buf()));
    let id = session.id.clone();
    let storage = session.storage_path(fx.sm.sessions_root());
    fx.sm.register_transient(session);

    fx.am
        .review
        .open_or_restore(&id, &storage, &[fx.dir.path().to_path_buf()])
        .await
        .unwrap();
    let handle = fx.am.review.open_bracket(&id).await.unwrap();
    std::fs::write(fx.dir.path().join("a.txt"), "two\n").unwrap();
    fx.am.review.close(&id, handle, "call-1", 1).await.unwrap();

    // A daemon restart: `review.jsonl` is on disk and nothing is in memory.
    // `register_transient` is exactly what a resume does, and it touches no
    // ledger.
    fx.am.review.clear_session(&id);
    assert!(!fx.am.review.is_open(&id));

    let bridge = crate::session_bridge::DaemonSessionBridge::new(Arc::new(RpcContext::for_test(
        Arc::new(crate::kiln_manager::KilnManager::new()),
        fx.sm.clone(),
        fx.am.clone(),
        Arc::new(crate::project_manager::ProjectManager::new(
            fx.dir.path().join("projects.json"),
        )),
        fx.event_tx.clone(),
        fx.dir.path().to_path_buf(),
    )));
    let through_lua = bridge.review_list_hunks(id.to_string()).await.unwrap();
    assert_eq!(
        through_lua.len(),
        1,
        "the plugin surface read a resumed session as having changed nothing"
    );
    assert_eq!(through_lua[0]["tool_call_ids"][0], "call-1");

    fx.am.review.clear_session(&id);
    let record = bridge
        .diff(
            crucible_lua::DiffOp::Get,
            serde_json::json!({ "source": { "kind": "session_record", "session": id.to_string() } }),
        )
        .await
        .unwrap();
    assert_eq!(
        record["files"][0]["path"], "a.txt",
        "cru.diff.get read a resumed session record as empty: {record}"
    );
}

// ── The crossing: a plugin session's own writes are its own review record ───

/// An agent whose one turn writes a note with `create_note`, then waits for
/// the tool result before it ends the turn.
///
/// Modelled on `session_bridge/tests/mod.rs`'s `BashCallingAgent`: the agent
/// only *asks* for the call. The real scheduler, the real review bracket and
/// the real note tool do the work, which is the point — a double at any of
/// those three would prove nothing about the crossing.
struct NoteWritingAgent;

#[async_trait::async_trait]
impl crucible_core::turn::Agent for NoteWritingAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        use crucible_core::turn::{StopReason, TurnEvent};
        let mut inbound = ctx.inbound;
        let body = async_stream::stream! {
            yield TurnEvent::ToolCall {
                id: "call-1".to_string(),
                name: "create_note".to_string(),
                args: serde_json::json!({
                    "path": "Socket rules.md",
                    "content": NOTE_TEXT,
                }),
                call: None,
            };
            yield TurnEvent::ToolBatchEnd;
            if let Some(rx) = inbound.as_mut() {
                while let Some(event) = rx.recv().await {
                    if matches!(event, TurnEvent::ToolResult { .. }) {
                        break;
                    }
                }
            }
            yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
        };
        Ok(Box::pin(body))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

crate::impl_unsupported_session_knobs!(NoteWritingAgent);

#[async_trait::async_trait]
impl crate::agent_manager::AgentHandle for NoteWritingAgent {
    async fn set_mode_str(&mut self, _: &str) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "auto"
    }
}

/// The note body the agent writes, and the `after_content` the review must
/// answer with.
const NOTE_TEXT: &str = "# Socket rules\n\nThe daemon socket is per-uid.\n";

/// The reflection pass stopped staging files: it writes its notes with the
/// note tools, in its own `plugin` session, in `auto` mode. This is the
/// crossing that has to hold for that to mean anything — a note written by a
/// plugin session's turn is a hunk in *that session's* review ledger, with
/// the note's path and the note's text, so a human accepts or rejects it in
/// the Changes panel.
///
/// The kiln is a plain directory with no `.git`, which is the shape a kiln
/// usually has, and the one `RootBackend::Plain` tracks.
///
/// The permissions config allows the call. The stance an unattended pass
/// actually runs under comes from `runtime/defaults/init.luau`, which needs a
/// plugin VM; what this test is about is the ledger, and a prompt nobody can
/// answer would only hang it.
#[tokio::test]
async fn a_plugin_sessions_note_write_lands_in_its_own_review_ledger() {
    use crate::agent_manager::AgentManagerParams;
    use crate::background_manager::BackgroundJobManager;
    use crate::kiln_manager::KilnManager;
    use crucible_core::config::components::permissions::{PermissionConfig, PermissionMode};
    use crucible_core::session::{SessionAgent, SessionType};

    let kiln = TempDir::new().unwrap();
    assert!(
        !kiln.path().join(".git").exists(),
        "the fixture kiln must be outside git"
    );
    let snapshots = TempDir::new().unwrap();

    let (event_tx, _events) = crate::EventBus::channel(256);
    let sm = crate::test_support::temp_session_manager_with_kilns(&[("notes", kiln.path())]);
    let am = Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: sm.clone(),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: Some(PermissionConfig {
            default: PermissionMode::Allow,
            ..Default::default()
        }),
        plugin_loader: None,
        source_roots: Default::default(),
        // A subdirectory, so the comment store beside it is in this
        // fixture and not in the shared temporary directory.
        review_snapshot_root: snapshots.path().join("review-snapshots"),
    }));
    am.set_agent_factory_override(Box::new(|_, _| {
        Box::pin(async {
            Ok(Box::new(NoteWritingAgent)
                as Box<dyn crate::agent_manager::AgentHandle + Send + Sync>)
        })
    }));

    // A pass has no workspace: the kiln is the only root it writes to, so it
    // is the only root the ledger tracks.
    let session = sm
        .create_session(
            SessionType::Plugin,
            vec![crate::test_support::kiln_name("notes")],
            None,
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.to_string();
    am.configure_agent(
        &session_id,
        SessionAgent {
            mode: None,
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some("ollama".to_string()),
            provider: crucible_core::config::BackendType::Ollama,
            model: "llama3.2".to_string(),
            system_prompt: "You are a reflection reviewer.".to_string(),
            max_context_tokens: None,
            endpoint: None,
            env_overrides: Default::default(),
            mcp_servers: Vec::new(),
            agent_card_name: None,
            agent_description: None,
            delegation_config: None,
            precognition_enabled: false,
            context_budget: None,
            context_strategy: Default::default(),
            tool_policy: None,
        },
    )
    .await
    .unwrap();
    // What `aux:set_mode("auto")` does: the write is bracketed and lands on
    // disk.
    am.set_mode(&session_id, "auto", None).await.unwrap();

    let (_message_id, done) = am
        .send_message_notified(&session_id, "reflect".to_string(), &event_tx, false, None)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(60), done)
        .await
        .expect("the turn finishes")
        .expect("the turn reports an outcome");

    assert_eq!(
        std::fs::read_to_string(kiln.path().join("Socket rules.md")).unwrap(),
        NOTE_TEXT,
        "the note has to be on disk before the ledger can attribute it"
    );

    ensure_loaded(&am, &sm, &session_id).await;
    let hunks = list_hunks(&am, &session_id)
        .await
        .expect("the pass's own record");

    assert_eq!(hunks.len(), 1, "one note written, one hunk: {hunks:?}");
    assert_eq!(hunks[0].path, "Socket rules.md");
    assert_eq!(hunks[0].after_content, NOTE_TEXT);
    assert!(
        hunks[0].before_content.is_empty(),
        "a new note has no before text: {:?}",
        hunks[0].before_content
    );
    assert!(
        hunks[0].tool_call_ids.iter().any(|id| id == "call-1"),
        "the hunk is attributed to the note call: {:?}",
        hunks[0].tool_call_ids
    );
}
