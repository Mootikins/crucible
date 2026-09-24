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
const CLAIMS_REQUESTED_ISOLATION: &str = r#"
cru.on_session_start(function(session)
  if session.isolation then
    cru.isolation.require{ session = session.id, plugin = "sandbox" }
  end
end, { required = true })
return { name = "sandbox", version = "0.1.0", description = "test isolation claimer" }
"#;

/// Write [`CLAIMS_REQUESTED_ISOLATION`] as a plugin under `dir` and return the
/// plugin root.
fn sandbox_plugin(dir: &Path) -> std::path::PathBuf {
    let root = dir.join("plugins");
    let plugin = root.join("sandbox");
    std::fs::create_dir_all(&plugin).expect("plugin dir");
    std::fs::write(plugin.join("init.lua"), CLAIMS_REQUESTED_ISOLATION).expect("init.lua");
    root
}

/// One daemon process, over storage that outlives it.
struct Daemon {
    ctx: Arc<RpcContext>,
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
    async fn boot(
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
                agents.set_daemon_permissions(loader.permission_registry());
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

    fn lua(&self) -> &mlua::Lua {
        self.lua.as_ref().expect("this daemon has a plugin runtime")
    }

    /// A live session that asked for isolation and got a claim, made the way
    /// `session.create` makes one: stored, configured, then started.
    async fn isolated_session(&self) -> String {
        let sm = &self.ctx.sessions;
        let session = sm
            .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
            .await
            .unwrap();
        let id = session.id.to_string();
        sm.modify_session(&id, |s| {
            s.isolation = Some(json!("sandbox"));
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

    async fn claimed(&self, session_id: &str) -> bool {
        self.ctx
            .session_lifecycle
            .isolation_claim(session_id)
            .await
            .is_some()
    }

    fn state(&self, session_id: &str) -> Option<SessionState> {
        self.ctx.sessions.get_session(session_id).map(|s| s.state)
    }

    /// A scripted agent, so a turn that is allowed to run needs no provider.
    fn inject_agent(&self, session_id: &str) {
        self.ctx.agents.install_agent_for_test(
            session_id.to_string(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                events: vec![script::text("revived"), script::done()],
            }) as _)),
        );
    }

    async fn rpc(&self, method: &str, params: serde_json::Value) -> Response {
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
        daemon.ctx.session_lifecycle.fire_session_end(&ending.id),
    )
    .await
    .expect("a resume inside a session hook waited for the plugin runtime its hook holds");

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
