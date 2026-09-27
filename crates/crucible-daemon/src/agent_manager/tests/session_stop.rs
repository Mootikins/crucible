//! Every way a live session stops runs the same steps, in the same order.
//!
//! The steps are the end stage (the plugin end hooks, which release the
//! isolation claim and the container), the release of the context attachment,
//! the state change, `cleanup_session`, the sweep of the handlers that the
//! session activated, and one `session:ended` broadcast. Before one owner ran
//! them, each stop site ran a different subset. An archive, a delete or the
//! auto-archive sweep of a live session kept its claim and its container, and
//! it sent no event. A Lua pause ran no end hooks at all.
//!
//! These tests stop a live, isolated session through each real door and read
//! what is left behind.

use super::revive_isolation::{sandbox_plugin_with, Daemon};
use super::script;
use crucible_core::protocol::SessionEventMessage;
use crucible_core::turn::{StopReason, TurnEvent};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

/// A plugin that isolates each session that asks for it, counts its end hooks,
/// and activates one handler for each session, as a workflow plugin does.
const STOP_PLUGIN: &str = r#"
cru.on_session_start(function(session)
  if session.isolation then
    cru.isolation.require{ session = session.id, plugin = "sandbox" }
  end
  cru.on("turn:complete", { session = session.id, key = "stop-test" }, function() end)
end, { required = true })
cru.on_session_end(function(session)
  _G.ended = _G.ended or {}
  _G.ended[session.id] = (_G.ended[session.id] or 0) + 1
  _G.end_reasons = _G.end_reasons or {}
  _G.end_reasons[session.id] = session.end_reason
end)
return { name = "sandbox", version = "0.1.0", description = "test stop steps" }
"#;

/// The attachment key that the tests queue before a stop. A second attach
/// with the same key succeeds only when the stop released the attachment.
const ATTACH_KEY: &str = "stop-test";

struct Rig {
    daemon: Daemon,
    id: String,
    events: broadcast::Receiver<SessionEventMessage>,
    _data_home: TempDir,
    _kiln: TempDir,
}

impl Rig {
    /// A daemon with a live, isolated session that holds a claim, an
    /// attachment and a session-scoped handler.
    async fn new() -> Self {
        Self::with_hooks(None).await
    }

    /// [`Self::new`], with `hook_lua` run on the plugin VM before the session
    /// starts.
    async fn with_hooks(hook_lua: Option<&str>) -> Self {
        let data_home = TempDir::new().unwrap();
        let kiln = TempDir::new().unwrap();
        let plugins = sandbox_plugin_with(data_home.path(), STOP_PLUGIN);
        let daemon = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), hook_lua).await;
        let id = daemon.isolated_session().await;
        daemon
            .ctx
            .agents
            .context_attach()
            .attach(&id, "lua", "attached", Some(ATTACH_KEY))
            .expect("precondition: the attachment is queued");
        let rig = Self {
            events: daemon.ctx.event_tx.subscribe(),
            daemon,
            id,
            _data_home: data_home,
            _kiln: kiln,
        };
        assert!(
            rig.scoped_handlers().await >= 1,
            "precondition: the start hook activated a handler for the session"
        );
        rig
    }

    fn end_hook_runs(&self) -> i64 {
        self.daemon
            .lua()
            .load(format!(r#"return (_G.ended or {{}})["{}"] or 0"#, self.id))
            .eval()
            .unwrap()
    }

    async fn scoped_handlers(&self) -> usize {
        let scope = crucible_lua::SessionScope::Session(self.id.clone());
        let guard = self.daemon.ctx.plugin_loader.lock().await;
        let loader = guard.as_ref().expect("the rig has a plugin runtime");
        loader
            .plugin_handlers()
            .all()
            .iter()
            .filter(|r| r.scope == scope)
            .count()
    }

    fn attachment_released(&self) -> bool {
        self.daemon
            .ctx
            .agents
            .context_attach()
            .attach(&self.id, "lua", "again", Some(ATTACH_KEY))
            .is_ok()
    }

    /// The `session:ended` reasons the bus carried for this session.
    fn ended_reasons(&mut self) -> Vec<String> {
        let mut reasons = Vec::new();
        while let Ok(msg) = self.events.try_recv() {
            if msg.event == "session:ended" && msg.data["session_id"] == self.id.as_str() {
                reasons.push(msg.data["reason"].as_str().unwrap_or_default().to_string());
            }
        }
        reasons
    }

    /// The steps every stop that leaves the session out of service runs.
    async fn assert_stopped(&mut self, door: &str, reason: &str) {
        assert!(
            !self.daemon.claimed(&self.id).await,
            "{door}: the session kept its isolation claim, so its container stays up \
             and the claim outlives the session"
        );
        assert_eq!(
            self.end_hook_runs(),
            1,
            "{door}: the plugin end hooks must run once"
        );
        let end_reason: Option<String> = self
            .daemon
            .lua()
            .load(format!(r#"return (_G.end_reasons or {{}})["{}"]"#, self.id))
            .eval()
            .unwrap();
        assert_eq!(
            end_reason.as_deref(),
            Some(reason),
            "{door}: the end hooks must read the cause as session.end_reason"
        );
        assert_eq!(
            self.scoped_handlers().await,
            0,
            "{door}: the handlers that the session activated must go with it"
        );
        assert_eq!(
            self.ended_reasons(),
            vec![reason.to_string()],
            "{door}: one session:ended event must name the cause"
        );
    }

    /// The steps that release the conversation, which a pause keeps.
    fn assert_released(&self, door: &str) {
        assert!(
            self.attachment_released(),
            "{door}: the context attachment of the session was not released"
        );
        assert_eq!(
            self.daemon
                .ctx
                .agents
                .session_residue(&self.id, &self.daemon.ctx.event_tx),
            Vec::<&str>::new(),
            "{door}: cleanup_session did not run"
        );
    }
}

#[tokio::test]
async fn an_rpc_archive_of_a_live_session_runs_every_stop_step() {
    let mut rig = Rig::new().await;
    let resp = rig
        .daemon
        .rpc("session.archive", json!({ "session_id": rig.id }))
        .await;
    assert!(resp.error.is_none(), "archive: {:?}", resp.error);
    rig.assert_stopped("session.archive", "archived").await;
    rig.assert_released("session.archive");
}

#[tokio::test]
async fn an_rpc_delete_of_a_live_session_runs_every_stop_step() {
    let mut rig = Rig::new().await;
    let resp = rig
        .daemon
        .rpc("session.delete", json!({ "session_id": rig.id }))
        .await;
    assert!(resp.error.is_none(), "delete: {:?}", resp.error);
    rig.assert_stopped("session.delete", "deleted").await;
    rig.assert_released("session.delete");
}

#[tokio::test]
async fn the_auto_archive_sweep_of_a_live_session_runs_every_stop_step() {
    let mut rig = Rig::new().await;
    rig.daemon
        .ctx
        .sessions
        .update_last_activity(&rig.id, chrono::Utc::now() - chrono::Duration::hours(80))
        .await
        .unwrap();
    let archived = crate::server::sweep_and_archive_stale_sessions(
        &rig.daemon.ctx.sessions,
        &rig.daemon.ctx.subscriptions,
        &rig.daemon.ctx.session_lifecycle,
        72,
    )
    .await
    .unwrap();
    assert_eq!(archived, 1, "precondition: the sweep archived the session");
    rig.assert_stopped("the auto-archive sweep", "auto_archived")
        .await;
    rig.assert_released("the auto-archive sweep");
}

#[tokio::test]
async fn an_rpc_end_runs_every_stop_step() {
    let mut rig = Rig::new().await;
    let resp = rig
        .daemon
        .rpc("session.end", json!({ "session_id": rig.id }))
        .await;
    assert!(resp.error.is_none(), "end: {:?}", resp.error);
    rig.assert_stopped("session.end", "ended").await;
    rig.assert_released("session.end");
}

#[tokio::test]
async fn a_lua_end_runs_every_stop_step() {
    let mut rig = Rig::new().await;
    rig.daemon
        .lua()
        .load(format!(
            r#"_G.ok, _G.err = cru.session.end_session("{}")"#,
            rig.id
        ))
        .exec_async()
        .await
        .expect("run the Lua end");
    let err: Option<String> = rig.daemon.lua().globals().get("err").unwrap();
    assert_eq!(err, None, "the Lua end must succeed");
    rig.assert_stopped("cru.session.end_session", "ended").await;
    rig.assert_released("cru.session.end_session");
}

/// A pause keeps the conversation, so a resume goes on from where the session
/// was. It still releases what the start hooks claimed, as the RPC pause does.
#[tokio::test]
async fn a_lua_pause_runs_the_end_stage() {
    let mut rig = Rig::new().await;
    rig.daemon
        .lua()
        .load(format!(
            r#"_G.ok, _G.err = cru.session.pause("{}")"#,
            rig.id
        ))
        .exec_async()
        .await
        .expect("run the Lua pause");
    let err: Option<String> = rig.daemon.lua().globals().get("err").unwrap();
    assert_eq!(err, None, "the Lua pause must succeed");
    rig.assert_stopped("cru.session.pause", "paused").await;
    assert!(
        !rig.attachment_released(),
        "a pause keeps the attachment state, so a resume keeps its budget"
    );
}

#[tokio::test]
async fn an_rpc_pause_runs_the_end_stage() {
    let mut rig = Rig::new().await;
    let resp = rig
        .daemon
        .rpc("session.pause", json!({ "session_id": rig.id }))
        .await;
    assert!(resp.error.is_none(), "pause: {:?}", resp.error);
    rig.assert_stopped("session.pause", "paused").await;
}

/// A `session:ended` handler that code inside the session registers for the
/// session's own end runs, once, and it reads the cause.
///
/// That is the documented teardown observer. Before the fix it never ran: the
/// stop swept the session's handlers before the event went out, and the bus
/// dispatcher reads the event later still.
#[tokio::test]
async fn a_session_ended_handler_scoped_to_the_session_runs_at_its_end() {
    const HOOKS: &str = r#"
cru.on("session:ended", function(_ctx, _event)
  _G.global_ended = (_G.global_ended or 0) + 1
end)
cru.on_session_start(function(session)
  cru.on("session:ended", { session = session.id }, function(ctx, event)
    _G.scoped_ended = (_G.scoped_ended or 0) + 1
    _G.scoped_reason = event.reason
    _G.scoped_ctx = ctx.session_id
  end)
end)
"#;
    let rig = Rig::with_hooks(Some(HOOKS)).await;
    // The daemon's own bus dispatcher, so the global handler runs as it does
    // in production.
    {
        let guard = rig.daemon.ctx.plugin_loader.lock().await;
        let loader = guard.as_ref().expect("the rig has a plugin runtime");
        crate::server::spawn_file_event_hooks(
            rig.daemon.ctx.event_tx.subscribe(),
            loader.plugin_handlers(),
            loader.plugin_lua(),
        );
    }

    let resp = rig
        .daemon
        .rpc("session.end", json!({ "session_id": rig.id }))
        .await;
    assert!(resp.error.is_none(), "end: {:?}", resp.error);

    let lua = rig.daemon.lua();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while lua
            .globals()
            .get::<Option<i64>>("global_ended")
            .unwrap()
            .is_none()
        {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("precondition: the bus delivered session:ended to the global handler");

    let scoped: Option<i64> = lua.globals().get("scoped_ended").unwrap();
    assert_eq!(
        scoped,
        Some(1),
        "the session-scoped session:ended handler must run once at the session's end"
    );
    let reason: Option<String> = lua.globals().get("scoped_reason").unwrap();
    assert_eq!(reason.as_deref(), Some("ended"));
    let ctx_session: Option<String> = lua.globals().get("scoped_ctx").unwrap();
    assert_eq!(ctx_session.as_deref(), Some(rig.id.as_str()));
}

/// An agent that stops in the middle of its turn until the test lets it go,
/// then calls one host tool and waits for the result.
struct GatedToolAgent {
    reached: Arc<tokio::sync::Notify>,
    gate: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl crucible_core::turn::Agent for GatedToolAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<futures::stream::BoxStream<'a, TurnEvent>, crucible_core::turn::AgentError> {
        let reached = Arc::clone(&self.reached);
        let gate = Arc::clone(&self.gate);
        let mut inbound = ctx.inbound;
        Ok(Box::pin(async_stream::stream! {
            yield script::text("working");
            reached.notify_one();
            gate.notified().await;
            yield script::tool_call(
                "call-1",
                "write_file",
                json!({ "path": "escaped.txt", "content": "on the host" }),
            );
            yield TurnEvent::ToolBatchEnd;
            if let Some(rx) = inbound.as_mut() {
                while let Some(event) = rx.recv().await {
                    if matches!(event, TurnEvent::ToolResult { .. }) {
                        break;
                    }
                }
            }
            yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
        }))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

crucible_core::impl_unsupported_session_knobs!(GatedToolAgent);

#[async_trait::async_trait]
impl crucible_core::traits::chat::AgentHandle for GatedToolAgent {
    async fn send_message_fire_and_forget(
        &mut self,
        _: String,
    ) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    async fn clear_history(&mut self) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, _: &str) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
}

/// A pause while a turn runs must not open the sandbox under that turn.
///
/// The pause runs the end hooks, and the end hooks release the isolation
/// claim. The isolation gate reads only the claim. Before the fix, a pause in
/// the middle of a turn released the claim, and the next tool call of that
/// turn ran on the host. Now the pause is refused while a turn runs, with an
/// error that says so, and the claim stays until the turn is over.
#[tokio::test]
async fn a_pause_during_a_turn_does_not_let_that_turn_run_on_the_host() {
    let rig = Rig::new().await;
    let workspace = TempDir::new().unwrap();
    rig.daemon
        .ctx
        .sessions
        .modify_session(&rig.id, |s| {
            s.workspace = Some(workspace.path().to_path_buf());
            true
        })
        .await
        .unwrap();
    let reached = Arc::new(tokio::sync::Notify::new());
    let gate = Arc::new(tokio::sync::Notify::new());
    rig.daemon.ctx.agents.install_agent_for_test(
        rig.id.clone(),
        Arc::new(Mutex::new(Box::new(GatedToolAgent {
            reached: Arc::clone(&reached),
            gate: Arc::clone(&gate),
        }) as _)),
    );
    let mut events = rig.daemon.ctx.event_tx.subscribe();
    // Allow, so a tool call that the isolation gate lets through runs.
    let (_, done) = rig
        .daemon
        .ctx
        .agents
        .send_message_notified(
            &rig.id,
            "write the file".to_string(),
            &rig.daemon.ctx.event_tx,
            false,
            Some(crucible_core::config::components::permissions::PermissionMode::Allow),
        )
        .await
        .expect("the turn starts");
    tokio::time::timeout(Duration::from_secs(10), reached.notified())
        .await
        .expect("the turn reached its gate");

    let paused = rig
        .daemon
        .rpc("session.pause", json!({ "session_id": rig.id }))
        .await;

    gate.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let msg = events.recv().await.expect("the bus stays open");
            if msg.session_id == rig.id && msg.event == "tool_result" {
                return msg.data;
            }
        }
    })
    .await
    .expect("the tool call reported a result");
    let _ = tokio::time::timeout(Duration::from_secs(10), done).await;

    let text = result.to_string();
    assert!(
        !workspace.path().join("escaped.txt").exists(),
        "the tool call wrote the file on the host: {text}"
    );
    assert!(
        text.contains("isolated"),
        "a tool call after a pause in the same turn ran outside the sandbox: {text}"
    );
    let err = paused
        .error
        .expect("a pause while a turn runs must be refused, not open the sandbox");
    assert!(
        err.message.contains("turn"),
        "the refusal must say that a turn runs: {}",
        err.message
    );
    assert!(
        rig.daemon.claimed(&rig.id).await,
        "the refused pause must keep the isolation claim"
    );
}

/// A send to a paused session must not run a turn outside the sandbox.
///
/// The pause ran the end hooks, which released the isolation claim. A send
/// used to take a paused session as it was: no start hook ran, so no plugin
/// claimed it again, and the tool call of the turn ran on the host. A send now
/// resumes the session through the start checks that every revive runs.
#[tokio::test]
async fn a_send_to_a_paused_isolated_session_gets_the_isolation_claim_back() {
    let rig = Rig::new().await;
    let workspace = TempDir::new().unwrap();
    rig.daemon
        .ctx
        .sessions
        .modify_session(&rig.id, |s| {
            s.workspace = Some(workspace.path().to_path_buf());
            true
        })
        .await
        .unwrap();
    let paused = rig
        .daemon
        .rpc("session.pause", json!({ "session_id": rig.id }))
        .await;
    assert!(paused.error.is_none(), "pause: {:?}", paused.error);
    assert!(
        !rig.daemon.claimed(&rig.id).await,
        "precondition: the pause released the claim"
    );

    rig.daemon.ctx.agents.install_agent_for_test(
        rig.id.clone(),
        Arc::new(Mutex::new(Box::new(super::StreamingMockAgent {
            events: vec![script::tool_call(
                "call-1",
                "write_file",
                json!({ "path": "escaped.txt", "content": "on the host" }),
            )],
        }) as _)),
    );
    let sent = rig
        .daemon
        .ctx
        .agents
        .send_message_notified(
            &rig.id,
            "write the file".to_string(),
            &rig.daemon.ctx.event_tx,
            false,
            Some(crucible_core::config::components::permissions::PermissionMode::Allow),
        )
        .await;
    let ran = sent.is_ok();
    if let Ok((_, done)) = sent {
        let _ = tokio::time::timeout(Duration::from_secs(10), done).await;
    }
    assert!(
        !workspace.path().join("escaped.txt").exists(),
        "a send to a paused isolated session wrote the file on the host"
    );
    if ran {
        assert!(
            rig.daemon.claimed(&rig.id).await,
            "a send to a paused session ran its turn with no isolation claim"
        );
    }
}

/// The RPC resume of a paused session runs the same start checks, so the
/// isolation claim that the pause released comes back.
#[tokio::test]
async fn an_rpc_resume_of_a_paused_session_gets_the_isolation_claim_back() {
    let rig = Rig::new().await;
    let paused = rig
        .daemon
        .rpc("session.pause", json!({ "session_id": rig.id }))
        .await;
    assert!(paused.error.is_none(), "pause: {:?}", paused.error);
    assert!(!rig.daemon.claimed(&rig.id).await, "precondition: no claim");

    let resumed = rig
        .daemon
        .rpc("session.resume", json!({ "session_id": rig.id }))
        .await;
    assert!(resumed.error.is_none(), "resume: {:?}", resumed.error);
    assert!(
        rig.daemon.claimed(&rig.id).await,
        "session.resume made a paused session live with no isolation claim"
    );
}
