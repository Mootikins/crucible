//! Every path that makes a stored session live again runs the start checks.
//!
//! The isolation registry is memory. A daemon restart empties it, and an RPC
//! `session.pause` releases a claim with the end hooks. The claim comes back
//! only when the plugin start hooks fire again. Revive-on-send and the Lua
//! `resume` did not fire them, so an isolated session came back live with no
//! claim, and an ACP agent then started on the host instead of in its sandbox.
//!
//! The persisted `isolation` value is the requirement. The in-memory claim is
//! only the proof that a plugin satisfied it. So these tests keep the value on
//! disk, lose the claim, and then revive the session through the real doors:
//! the `session.send_message` RPC, the Lua `cru.session.resume`, and Lua that
//! runs while the plugin runtime is held.

use super::revive_cold::manager_over;
use super::{script, test_agent, StreamingMockAgent};
use crate::agent_manager::{AgentManager, AgentManagerParams};
use crate::background_manager::BackgroundJobManager;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::kiln_manager::KilnManager;
use crate::project_manager::ProjectManager;
use crate::protocol::{Request, RequestId, Response};
use crate::rpc::{RpcContext, RpcDispatcher};
use crate::session_bridge::DaemonSessionBridge;
use crate::subscription::ClientId;
use crate::test_support::kiln_name;
use crucible_core::session::{SessionState, SessionType};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

/// A plugin that claims isolation for every session that asked for it.
///
/// It reads the persisted `session.isolation`, as the `oci` plugin does, so a
/// session gets a claim back only when its start hooks fire again.
pub(super) const CLAIMS_REQUESTED_ISOLATION: &str = r#"
cru.on_session_start(function(session)
  if session.isolation then
    cru.isolation.require{ session = session.id, plugin = "sandbox" }
  end
end, { required = true })
return { name = "sandbox", version = "0.1.0", description = "test isolation claimer" }
"#;

/// A plugin that claims isolation for every session, as the `oci` plugin does
/// when the project configuration names an image.
///
/// The session asks for nothing. Only the plugin configuration isolates it, so
/// the persisted `isolation` value stays absent.
const CLAIMS_EVERY_SESSION: &str = r#"
cru.on_session_start(function(session)
  if session.isolation ~= false then
    cru.isolation.require{ session = session.id, plugin = "sandbox" }
  end
end, { required = true })
return { name = "sandbox", version = "0.1.0", description = "test configured isolation" }
"#;

/// A plugin that claims nothing: the plugin runtime is there, and no plugin in
/// it isolates a session.
const CLAIMS_NOTHING: &str = r#"
return { name = "sandbox", version = "0.1.0", description = "claims nothing" }
"#;

/// Write [`CLAIMS_REQUESTED_ISOLATION`] as a plugin under `dir` and return the
/// plugin root.
pub(super) fn sandbox_plugin(dir: &Path) -> std::path::PathBuf {
    sandbox_plugin_with(dir, CLAIMS_REQUESTED_ISOLATION)
}

/// Write `init` as the `sandbox` plugin under `dir` and return the plugin root.
pub(super) fn sandbox_plugin_with(dir: &Path, init: &str) -> std::path::PathBuf {
    let root = dir.join("plugins");
    let plugin = root.join("sandbox");
    std::fs::create_dir_all(&plugin).expect("plugin dir");
    std::fs::write(plugin.join("init.lua"), init).expect("init.lua");
    root
}

/// One daemon process, over storage that outlives it.
pub(super) struct Daemon {
    pub(super) ctx: Arc<RpcContext>,
    dispatcher: RpcDispatcher,
    /// The plugin VM, when the daemon has a plugin runtime.
    lua: Option<Arc<mlua::Lua>>,
}

impl Daemon {
    /// Boot a daemon over `data_home`.
    ///
    /// `plugins` is a plugin root to activate. `None` boots a daemon with no
    /// plugin runtime, which is a daemon where the isolating plugin is gone.
    /// `hook_lua` runs on the plugin VM before the runtime is shared.
    pub(super) async fn boot(
        data_home: &Path,
        kiln: &Path,
        plugins: Option<&Path>,
        hook_lua: Option<&str>,
    ) -> Self {
        let sm = manager_over(data_home, kiln);
        let (event_tx, _) = broadcast::channel(64);
        let plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>> = Arc::new(Mutex::new(None));
        let am = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: sm.clone(),
            background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: Some(plugin_loader.clone()),
            card_roots: Default::default(),
            review_snapshot_root: crate::test_support::scratch_snapshot_root(),
        }));
        let ctx = Arc::new(RpcContext::for_test_with_plugin_loader(
            Arc::new(KilnManager::new()),
            sm,
            am,
            Arc::new(ProjectManager::new(data_home.join("projects.json"))),
            event_tx,
            data_home.to_path_buf(),
            plugin_loader.clone(),
        ));

        let lua = match plugins {
            None => None,
            Some(root) => {
                let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("plugin loader");
                loader
                    .activate_discovered(&[(
                        root.to_path_buf(),
                        crucible_lua::PluginSource::EnvPath,
                    )])
                    .await
                    .expect("activate the sandbox plugin");
                loader
                    .upgrade_with_sessions(Arc::new(DaemonSessionBridge::new(ctx.clone())))
                    .expect("wire the session bridge into the plugin VM");
                if let Some(code) = hook_lua {
                    loader.eval(code).await.expect("register the test hook");
                }
                // The binds `Server::boot_plugins` makes. Without them a turn
                // falls back to the loader mutex, which production never does.
                let agents = &ctx.agents;
                agents.set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());
                agents.set_isolation(loader.isolation());
                agents.set_plugin_tool_registry(loader.plugin_registry());
                agents.set_publications(loader.publications());
                let lua = loader.plugin_lua();
                *plugin_loader.lock().await = Some(loader);
                Some(lua)
            }
        };

        Self {
            dispatcher: RpcDispatcher::new(ctx.clone()),
            ctx,
            lua,
        }
    }

    pub(super) fn lua(&self) -> &mlua::Lua {
        self.lua.as_ref().expect("this daemon has a plugin runtime")
    }

    /// A live session that asked for isolation and got a claim, made the way
    /// `session.create` makes one: stored, configured, then started.
    pub(super) async fn isolated_session(&self) -> String {
        self.started_session(Some(json!("sandbox"))).await
    }

    /// A live session with a claim, made the way `session.create` makes one:
    /// stored, configured, then started. `isolation` is the value the caller
    /// persisted. `None` leaves the decision to the plugin configuration.
    pub(super) async fn started_session(&self, isolation: Option<serde_json::Value>) -> String {
        let sm = &self.ctx.sessions;
        let session = sm
            .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
            .await
            .unwrap();
        let id = session.id.to_string();
        sm.modify_session(&id, |s| {
            s.isolation = isolation;
            true
        })
        .await
        .unwrap();
        self.ctx
            .agents
            .configure_agent(&id, test_agent())
            .await
            .unwrap();
        self.ctx
            .session_lifecycle
            .enforce_session_start(&id)
            .await
            .expect("the session starts");
        assert!(
            self.claimed(&id).await,
            "precondition: the session is sandboxed, or these tests assert nothing"
        );
        id
    }

    pub(super) async fn claimed(&self, session_id: &str) -> bool {
        self.ctx
            .session_lifecycle
            .isolation_claim(session_id)
            .await
            .is_some()
    }

    pub(super) fn state(&self, session_id: &str) -> Option<SessionState> {
        self.ctx.sessions.get_session(session_id).map(|s| s.state)
    }

    /// A scripted agent, so a turn that is allowed to run needs no provider.
    pub(super) fn inject_agent(&self, session_id: &str) {
        self.ctx.agents.install_agent_for_test(
            session_id.to_string(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                events: vec![script::text("revived"), script::done()],
            }) as _)),
        );
    }

    pub(super) async fn rpc(&self, method: &str, params: serde_json::Value) -> Response {
        self.dispatcher
            .dispatch(
                ClientId::new(),
                Request {
                    jsonrpc: "2.0".to_string(),
                    id: Some(RequestId::Number(1)),
                    method: method.to_string(),
                    params,
                },
            )
            .await
    }
}

/// Revive-on-send after a restart fires the start hooks again, so the plugin
/// claims the persisted isolation again.
///
/// Before the fix the send revived the session and ran the turn, and the new
/// daemon held no claim for it.
#[tokio::test]
async fn a_send_after_a_restart_gets_the_isolation_claim_back() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());

    let first = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    let id = first.isolated_session().await;
    drop(first);

    let second = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    assert!(
        second.state(&id).is_none() && !second.claimed(&id).await,
        "precondition: the restarted daemon holds neither the session nor its claim"
    );
    second.inject_agent(&id);

    let resp = second
        .rpc(
            "session.send_message",
            json!({ "session_id": id, "content": "still there?" }),
        )
        .await;

    assert!(
        resp.error.is_none(),
        "the plugin is still loaded, so the revive must succeed: {:?}",
        resp.error
    );
    assert!(
        second.claimed(&id).await,
        "session {id} persisted isolation \"sandbox\" and came back live with no \
         isolation claim: its tools run on the host, and an ACP agent starts there"
    );
}

/// When nothing can claim the persisted isolation after a restart, the send is
/// refused with an error that the caller sees, and the session is not left live.
#[tokio::test]
async fn a_send_after_a_restart_is_refused_when_no_plugin_can_claim_the_isolation() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());

    let first = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    let id = first.isolated_session().await;
    drop(first);

    // The isolating plugin is gone after the restart.
    let second = Daemon::boot(data_home.path(), kiln.path(), None, None).await;
    second.inject_agent(&id);

    let resp = second
        .rpc(
            "session.send_message",
            json!({ "session_id": id, "content": "still there?" }),
        )
        .await;

    let err = resp.error.unwrap_or_else(|| {
        panic!(
            "session {id} persisted isolation \"sandbox\", no plugin can claim it, \
             and the send still ran a turn on the host"
        )
    });
    assert!(
        err.message.contains("isolation") && err.message.contains("sandbox"),
        "the refusal must name the isolation that is missing: {}",
        err.message
    );
    assert_ne!(
        second.state(&id),
        Some(SessionState::Active),
        "a refused revive must not leave the session live"
    );
}

/// The Lua `cru.session.resume` runs the start checks, as the RPC
/// `session.resume` does.
///
/// An RPC `session.pause` fires the end hooks, which release the claim. Before
/// the fix a plugin then resumed the session with no claim.
#[tokio::test]
async fn a_lua_resume_gets_the_isolation_claim_back() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());
    let daemon = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    let id = daemon.isolated_session().await;

    let paused = daemon
        .rpc("session.pause", json!({ "session_id": id }))
        .await;
    assert!(paused.error.is_none(), "pause: {:?}", paused.error);
    assert!(
        !daemon.claimed(&id).await,
        "precondition: pausing released the claim with the end hooks"
    );

    daemon
        .lua()
        .load(format!(r#"_G.ok, _G.err = cru.session.resume("{id}")"#))
        .exec_async()
        .await
        .expect("run the Lua resume");

    let err: Option<String> = daemon.lua().globals().get("err").unwrap();
    assert_eq!(
        err, None,
        "the plugin is loaded, so the resume must succeed"
    );
    assert_eq!(daemon.state(&id), Some(SessionState::Active));
    assert!(
        daemon.claimed(&id).await,
        "the Lua resume made session {id} live with no isolation claim"
    );
}

/// Lua that runs inside a plugin session hook holds the plugin runtime, so the
/// start hooks of another session cannot run there. A resume from that place
/// is refused. It must not wait for the runtime that its own caller holds.
#[tokio::test]
async fn a_lua_resume_inside_a_session_hook_is_refused_and_does_not_hang() {
    const RESUME_FROM_END_HOOK: &str = r#"
cru.on_session_end(function(_session)
  if _G.resume_target then
    _G.resume_ok, _G.resume_err = cru.session.resume(_G.resume_target)
  end
end)
"#;
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());
    let daemon = Daemon::boot(
        data_home.path(),
        kiln.path(),
        Some(&plugins),
        Some(RESUME_FROM_END_HOOK),
    )
    .await;

    let target = daemon.isolated_session().await;
    daemon.ctx.sessions.pause_session(&target).await.unwrap();
    daemon
        .lua()
        .globals()
        .set("resume_target", target.clone())
        .unwrap();

    let ending = daemon
        .ctx
        .sessions
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .unwrap();

    tokio::time::timeout(
        Duration::from_secs(30),
        daemon
            .ctx
            .session_lifecycle
            .stop(&ending.id, crate::session_lifecycle::StopCause::End),
    )
    .await
    .expect("a resume inside a session hook waited for the plugin runtime its hook holds")
    .expect("the stop succeeds");

    let err: Option<String> = daemon.lua().globals().get("resume_err").unwrap();
    let err = err.expect("a resume that cannot run the start hooks must be refused");
    assert!(
        err.contains("plugin runtime"),
        "the refusal must say why the start checks cannot run: {err}"
    );
    assert_ne!(daemon.state(&target), Some(SessionState::Active));
}

/// `lua.eval` also holds the plugin runtime while its Lua runs. A send from
/// there that must revive a session is refused. It must not wait forever.
#[tokio::test]
async fn a_revive_from_lua_eval_is_refused_and_does_not_hang() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());
    let daemon = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;

    let id = daemon.isolated_session().await;
    daemon.ctx.sessions.end_session(&id).await.unwrap();
    daemon.inject_agent(&id);

    let resp = tokio::time::timeout(
        Duration::from_secs(30),
        daemon.rpc(
            "lua.eval",
            json!({
                "code": format!(
                    r#"local _, err = cru.session.send_message("{id}", "hi") return tostring(err)"#
                )
            }),
        ),
    )
    .await
    .expect("a revive inside lua.eval waited for the plugin runtime that eval holds");

    let result = resp.result.expect("lua.eval answers");
    let err = result["result"].as_str().unwrap_or_default();
    assert!(
        err.contains("plugin runtime"),
        "a send that must revive a session inside lua.eval must be refused: {err}"
    );
}

/// An agent manager with no session lifecycle has no plugin runtime, so no
/// plugin can claim isolation. A session that asked for isolation is refused
/// on send. It must not revive as if it never asked.
#[tokio::test]
async fn a_send_with_no_session_lifecycle_refuses_a_session_that_asked_for_isolation() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());

    let first = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    let id = first.isolated_session().await;
    drop(first);

    let (sm, am, tx) = super::revive_cold::cold_manager(data_home.path(), kiln.path(), None).await;
    am.install_agent_for_test(
        id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("revived"), script::done()],
        }) as _)),
    );

    let sent = am
        .send_message(&id, "still there?".to_string(), &tx, false, None)
        .await;

    let err = sent.expect_err("a session that asked for isolation revived with no way to claim it");
    assert!(
        matches!(err, crate::agent_manager::AgentError::SessionRefused(_)),
        "the refusal must be its own variant, so RPC answers it as a refusal: {err:?}"
    );
    assert!(err.to_string().contains("sandbox"), "{err}");
    drop(sm);
}

/// The claim itself is persisted. A session that the plugin configuration
/// isolated has no `isolation` value, so only the stored record of the claim
/// says that it must be sandboxed.
///
/// Before the fix a restart with the plugin gone revived the session with no
/// requirement at all, and its turn ran on the host.
#[tokio::test]
async fn a_configured_isolation_is_required_after_a_restart_without_the_plugin() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin_with(data_home.path(), CLAIMS_EVERY_SESSION);

    let first = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    let id = first.started_session(None).await;
    drop(first);

    // The isolating plugin is gone after the restart.
    let second = Daemon::boot(data_home.path(), kiln.path(), None, None).await;
    second.inject_agent(&id);

    let resp = second
        .rpc(
            "session.send_message",
            json!({ "session_id": id, "content": "still there?" }),
        )
        .await;

    let err = resp.error.unwrap_or_else(|| {
        panic!(
            "session {id} was sandboxed by plugin \"sandbox\" from the project \
             configuration, the plugin is gone, and the send still ran a turn on the host"
        )
    });
    assert!(
        err.message.contains("isolation") && err.message.contains("sandbox"),
        "the refusal must name the plugin that isolated the session: {}",
        err.message
    );
    assert_ne!(
        second.state(&id),
        Some(SessionState::Active),
        "a refused revive must not leave the session live"
    );
}

/// A session that no plugin ever isolated keeps reviving on a daemon with no
/// isolating plugin. The record is written only when a claim is made.
#[tokio::test]
async fn a_session_that_was_never_isolated_revives_without_the_plugin() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());

    let first = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    let sm = &first.ctx.sessions;
    let session = sm
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .unwrap();
    let id = session.id.to_string();
    first
        .ctx
        .agents
        .configure_agent(&id, test_agent())
        .await
        .unwrap();
    first
        .ctx
        .session_lifecycle
        .enforce_session_start(&id)
        .await
        .expect("the session starts");
    assert!(!first.claimed(&id).await, "precondition: no claim");
    drop(first);

    let second = Daemon::boot(data_home.path(), kiln.path(), None, None).await;
    second.inject_agent(&id);
    let resp = second
        .rpc(
            "session.send_message",
            json!({ "session_id": id, "content": "still there?" }),
        )
        .await;
    assert!(
        resp.error.is_none(),
        "a session that no plugin isolated must revive: {:?}",
        resp.error
    );
}

/// Each firing of the start hooks is paired with one firing of the end hooks.
///
/// An end releases the claim and the container. A revive fires the start
/// hooks again, and the plugin claims again. Before the fix the once-only end
/// marker stayed set after the first end, so the second end ran no end hooks:
/// the session kept its claim, and the plugin kept its container.
#[tokio::test]
async fn a_revived_session_releases_its_claim_again_when_it_ends_again() {
    const COUNTS_ENDS: &str = r#"
cru.on_session_end(function(_session)
  _G.ends = (_G.ends or 0) + 1
end)
"#;
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());
    let daemon = Daemon::boot(
        data_home.path(),
        kiln.path(),
        Some(&plugins),
        Some(COUNTS_ENDS),
    )
    .await;
    let id = daemon.isolated_session().await;

    let ended = daemon.rpc("session.end", json!({ "session_id": id })).await;
    assert!(ended.error.is_none(), "first end: {:?}", ended.error);
    assert!(
        !daemon.claimed(&id).await,
        "precondition: the first end released the claim"
    );

    daemon.inject_agent(&id);
    let sent = daemon
        .rpc(
            "session.send_message",
            json!({ "session_id": id, "content": "back again" }),
        )
        .await;
    assert!(sent.error.is_none(), "revive: {:?}", sent.error);
    assert!(
        daemon.claimed(&id).await,
        "precondition: the revive claimed isolation again"
    );

    let ended = daemon.rpc("session.end", json!({ "session_id": id })).await;
    assert!(ended.error.is_none(), "second end: {:?}", ended.error);
    assert!(
        !daemon.claimed(&id).await,
        "session {id} ended a second time and kept its isolation claim: the end \
         hooks did not run, so the plugin keeps its container"
    );
    let ends: i64 = daemon.lua().globals().get("ends").unwrap();
    assert_eq!(ends, 2, "each start is paired with one end");
}

/// A plugin create runs the start checks that an RPC create runs.
///
/// Before the fix `cru.session.create` skipped them. A session that asked for
/// isolation, with no plugin to claim it, was created live on the host.
#[tokio::test]
async fn a_plugin_create_that_asks_for_isolation_is_refused_when_no_plugin_claims_it() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin_with(data_home.path(), CLAIMS_NOTHING);
    let daemon = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;

    daemon
        .lua()
        .load(
            r#"_G.created, _G.err = cru.session.create({ type = "chat", isolation = "sandbox" })"#,
        )
        .exec_async()
        .await
        .expect("run the Lua create");

    let err: Option<String> = daemon.lua().globals().get("err").unwrap();
    let err = err.unwrap_or_else(|| {
        panic!(
            "a plugin created a session that asked for isolation \"sandbox\", no plugin \
             claimed it, and the session is live on the host"
        )
    });
    assert!(
        err.contains("isolation") && err.contains("sandbox"),
        "the refusal must name the isolation that is missing: {err}"
    );
    assert!(
        daemon
            .ctx
            .sessions
            .list_sessions()
            .iter()
            .all(|s| s.state != SessionState::Active),
        "a refused create must not leave a live session"
    );
}

/// A plugin create fires the start hooks, so a plugin claims the isolation
/// that the new session asked for. A plugin end fires the end hooks, so the
/// claim goes with the session.
#[tokio::test]
async fn a_plugin_create_gets_its_claim_and_a_plugin_end_releases_it() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());
    let daemon = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;

    daemon
        .lua()
        .load(
            r#"
            local created, err = cru.session.create({ type = "chat", isolation = "sandbox" })
            assert(created, err)
            _G.created_id = created.id
            "#,
        )
        .exec_async()
        .await
        .expect("the Lua create succeeds");
    let id: String = daemon.lua().globals().get("created_id").unwrap();
    assert!(
        daemon.claimed(&id).await,
        "a plugin created session {id} with no isolation claim"
    );

    daemon
        .lua()
        .load(format!(r#"assert(cru.session.end_session("{id}"))"#))
        .exec_async()
        .await
        .expect("the Lua end succeeds");
    assert!(
        !daemon.claimed(&id).await,
        "a plugin ended session {id}, and its isolation claim stayed: the end hooks \
         did not run, so the plugin keeps its container"
    );
}

/// Lua inside a session hook holds the plugin runtime. A plugin end from there
/// cannot wait for the runtime, so the end hooks run after the hook returns.
/// They must not be skipped, and they must not hang the daemon.
#[tokio::test]
async fn a_plugin_end_inside_a_session_hook_runs_the_end_hooks_later() {
    const END_FROM_END_HOOK: &str = r#"
cru.on_session_end(function(_session)
  if _G.end_target then
    local target = _G.end_target
    _G.end_target = nil
    _G.end_ok, _G.end_err = cru.session.end_session(target)
  end
end)
"#;
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());
    let daemon = Daemon::boot(
        data_home.path(),
        kiln.path(),
        Some(&plugins),
        Some(END_FROM_END_HOOK),
    )
    .await;
    let target = daemon.isolated_session().await;
    daemon
        .lua()
        .globals()
        .set("end_target", target.clone())
        .unwrap();

    let ending = daemon
        .ctx
        .sessions
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(30),
        daemon
            .ctx
            .session_lifecycle
            .stop(&ending.id, crate::session_lifecycle::StopCause::End),
    )
    .await
    .expect("a plugin end inside a session hook waited for the plugin runtime its hook holds")
    .expect("the stop succeeds");

    let err: Option<String> = daemon.lua().globals().get("end_err").unwrap();
    assert_eq!(err, None, "the end itself succeeds");
    // The whole stop runs after the hook returns, in its fixed order: the end
    // hooks release the claim, then the session ends.
    tokio::time::timeout(Duration::from_secs(30), async {
        while daemon.claimed(&target).await || daemon.state(&target) != Some(SessionState::Ended) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the stop of a session that a hook ended never ran: its claim stayed");
}

/// A plugin that loads runs its Lua with the plugin runtime held. A session
/// create from that Lua cannot run the start hooks, so it is refused. It must
/// not wait for the runtime that the reload holds.
#[tokio::test]
async fn a_plugin_create_while_the_plugin_loads_is_refused_and_does_not_hang() {
    const CREATES_WHEN_IT_LOADS: &str = r#"
if _G.create_on_load then
  _G.load_created, _G.load_err = cru.session.create({ type = "chat" })
end
return { name = "sandbox", version = "0.1.0", description = "creates a session when it loads" }
"#;
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin_with(data_home.path(), CREATES_WHEN_IT_LOADS);
    let daemon = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    daemon.lua().globals().set("create_on_load", true).unwrap();

    let resp = tokio::time::timeout(
        Duration::from_secs(30),
        daemon.rpc("plugin.reload", json!({ "name": "sandbox" })),
    )
    .await
    .expect("a create while the plugin loads waited for the plugin runtime that the reload holds");

    assert!(resp.error.is_none(), "reload: {:?}", resp.error);
    let err: Option<String> = daemon.lua().globals().get("load_err").unwrap();
    let err = err.expect("a create that cannot run the start checks must be refused");
    assert!(
        err.contains("plugin runtime"),
        "the refusal must say why the start checks cannot run: {err}"
    );
    assert!(
        daemon
            .ctx
            .sessions
            .list_sessions()
            .iter()
            .all(|s| s.state != SessionState::Active),
        "a refused create must not leave a live session"
    );
}

/// A history read loads the transcript of a session without making it live.
///
/// The web history page used `session.resume_from_storage`, which revived the
/// session and ran the start checks on every read: an ended session came back
/// `Active`, and the isolating plugin claimed it and could pull a container.
/// `session.history` reads storage and changes nothing.
#[tokio::test]
async fn a_history_read_does_not_revive_an_ended_session() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());
    let daemon = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    let id = daemon.isolated_session().await;
    let ended = daemon.rpc("session.end", json!({ "session_id": id })).await;
    assert!(ended.error.is_none(), "end: {:?}", ended.error);
    assert!(
        !daemon.claimed(&id).await,
        "precondition: the end released the claim"
    );

    let resp = daemon
        .rpc("session.history", json!({ "session_id": id, "limit": 10 }))
        .await;

    let result = resp
        .result
        .unwrap_or_else(|| panic!("session.history answers: {:?}", resp.error));
    assert_eq!(result["session_id"], id.as_str());
    assert_eq!(
        result["state"], "ended",
        "the reply reports the stored state"
    );
    assert!(result["history"].is_array(), "{result}");
    assert!(result["total_events"].is_u64(), "{result}");
    assert_eq!(
        daemon.state(&id),
        Some(SessionState::Ended),
        "a history read made the ended session live"
    );
    assert!(
        !daemon.claimed(&id).await,
        "a history read ran the start hooks, which claim isolation and can pull a container"
    );
}

/// A history read of a session that only storage holds leaves it out of
/// memory.
#[tokio::test]
async fn a_history_read_of_a_stored_session_leaves_it_stored() {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let plugins = sandbox_plugin(data_home.path());

    let first = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    let id = first.isolated_session().await;
    drop(first);

    let second = Daemon::boot(data_home.path(), kiln.path(), Some(&plugins), None).await;
    let resp = second
        .rpc("session.history", json!({ "session_id": id }))
        .await;
    assert!(resp.error.is_none(), "history: {:?}", resp.error);
    assert_eq!(
        second.state(&id),
        None,
        "the read loaded the session into memory"
    );
    assert!(!second.claimed(&id).await, "the read ran the start hooks");

    let missing = second
        .rpc(
            "session.history",
            json!({ "session_id": "chat-2020-01-01T0000-absent" }),
        )
        .await;
    assert!(missing.error.is_some(), "an unknown session is an error");
}
