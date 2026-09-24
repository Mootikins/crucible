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
use crucible_core::protocol::SessionEventMessage;
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::broadcast;

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
        let data_home = TempDir::new().unwrap();
        let kiln = TempDir::new().unwrap();
        let plugins = sandbox_plugin_with(data_home.path(), STOP_PLUGIN);
        let daemon = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
        let id = daemon.isolated_session().await;
        daemon
            .ctx
            .agents
            .context_attach()
            .attach(&id, "attached", Some(ATTACH_KEY))
            .expect("precondition: the attachment is queued");
        let rig = Self {
            events: daemon.ctx.event_tx.subscribe(),
            daemon,
            id,
            _data_home: data_home,
            _kiln: kiln,
        };
        assert_eq!(
            rig.scoped_handlers().await,
            1,
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
            .attach(&self.id, "again", Some(ATTACH_KEY))
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
            self.daemon.ctx.agents.session_residue(&self.id),
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
