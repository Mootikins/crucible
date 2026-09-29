//! Shared daemon-level fixture for the shipped `reflection` and `auto-title`
//! plugins.
//!
//! Both plugins need the same shape underneath: a cleared provider
//! environment, a mock chat endpoint, a `SessionManager`, an `AgentManager`,
//! and a `DaemonPluginLoader` that has activated the plugin under test. Two
//! copies of that setup used to live here, one per plugin, and they drifted
//! on details neither test meant to test (a kiln registered or not, a mode
//! registry wired or not). [`Rig::reflection`] and [`Rig::auto_title`] build
//! the one struct now, so the shared fields stay one shape and each
//! constructor names only what its plugin needs.

use super::*;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::test_support::temp_session_manager_with_kilns;
use crucible_lua::manifest::PluginSource;
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The reviewer's note in every provider answer of the `reflection` fixture.
pub(super) const NOTE: &str = "# Remember\n\nUse the daemon as the only storage owner.\n";

/// What the `auto-title` provider answers when it is asked for a title.
pub(super) const AUTO_TITLE_ANSWER: &str = "\"Fixing the auth flow.\"";

/// The reviewer's chat route, and only it, in the `reflection` fixture. The
/// daemon also POSTs `/api/show` to probe the model's context window when it
/// creates the pass's session (the session-create setup task), and whether
/// that probe lands before the test tears the mock down is a race: an
/// endpoint that answered it made the "one tool call and one continuation"
/// count below three on a fresh runner. A chat request is the one that
/// reaches the chat route.
pub(super) fn is_chat_request(request: &wiremock::Request) -> bool {
    request.method.as_str() == "POST" && request.url.path().ends_with("/api/chat")
}

/// A daemon-level plugin fixture: a cleared provider environment, one mock
/// chat endpoint, a session and agent manager pair, and a plugin loader that
/// has activated the plugin under test.
///
/// [`Rig::reflection`] and [`Rig::auto_title`] build this over the shipped
/// plugin each names.
pub(super) struct Rig {
    // Dropped last: the provider environment stays clear while the rig lives.
    _env_guards: Vec<crucible_core::test_support::EnvVarGuard>,
    _env_lock: std::sync::MutexGuard<'static, ()>,
    pub(super) tmp: TempDir,
    /// A kiln directory the fixture created. `reflection` registers it as
    /// `knowledge`; `auto_title` has no use for one, but keeps the field one
    /// shape rather than an `Option` only one constructor would ever set.
    pub(super) kiln: TempDir,
    pub(super) provider: MockServer,
    pub(super) sessions: Arc<SessionManager>,
    pub(super) agents: Arc<AgentManager>,
    pub(super) ctx: Arc<RpcContext>,
    pub(super) bridge: Arc<DaemonSessionBridge>,
    pub(super) lua: Arc<mlua::Lua>,
    pub(super) loader: Arc<tokio::sync::Mutex<Option<DaemonPluginLoader>>>,
    pub(super) observed: broadcast::Receiver<crucible_core::protocol::SessionEventMessage>,
}

impl Rig {
    /// The shipped `reflection` plugin, active, with a provider fixture that
    /// proposes [`NOTE`], and the kiln `knowledge`.
    ///
    /// `hook_lua` runs on the plugin VM before the reflection plugin
    /// activates, so a test can put a stand-in plugin hook there.
    ///
    /// The environment lock is held across the awaits on purpose, for the
    /// life of the rig: nextest runs each test in its own process, so it
    /// waits for nothing, and it keeps the provider environment clear.
    #[allow(clippy::await_holding_lock)]
    pub(super) async fn reflection(hook_lua: Option<&str>) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        // The provider table is the one this test injects: the daemon's own
        // enumeration also reads the process environment's credentials, so a
        // developer's `GLM_AUTH_TOKEN` (or `OPENAI_API_KEY`, …) would put a
        // real endpoint on the setup path. nextest runs each test in its own
        // process, so the guard races with nothing.
        let env_lock = crate::agent_manager::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let env_guards = crate::agent_manager::clear_provider_env();

        let provider = MockServer::start().await;
        Mock::given(is_chat_request)
            .respond_with(move |request: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let has_result = body["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["role"] == "tool");
                let message = if has_result {
                    json!({"role": "assistant", "content": "Saved one lesson for review."})
                } else {
                    json!({"role": "assistant", "content": "", "tool_calls": [{"id": "lesson", "function": {
                        "name": "create_note", "arguments": {"path": "Remember.md", "content": NOTE}
                    }}]})
                };
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/x-ndjson")
                    .set_body_string(format!("{}\n{}\n", json!({"model": "reflection-fixture", "message": message, "done": false}), json!({"done": true, "done_reason": "stop"})))
            })
            .mount(&provider)
            .await;

        let tmp = TempDir::new().unwrap();
        let kiln = TempDir::new().unwrap();
        let sessions = temp_session_manager_with_kilns(&[("knowledge", kiln.path())]);
        let shared_loader = Arc::new(tokio::sync::Mutex::new(None));
        let (events, observed) = crate::EventBus::channel(128);
        let kilns = Arc::new(KilnManager::with_event_tx(
            events.clone(),
            Some(crucible_core::config::EmbeddingProviderConfig::mock(Some(
                384,
            ))),
            crucible_core::config::default_max_precognition_chars(),
        ));
        let mut llm = bridge_llm_config();
        llm.providers.get_mut("ollama").unwrap().endpoint = Some(provider.uri());
        let mut loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        let lua = loader.plugin_lua();
        let previous = crucible_lua::set_source(&lua, crucible_lua::LuaSource::Builtin);
        lua.load(crucible_lua::BUILTIN_INIT_LUA).exec().unwrap();
        crucible_lua::set_source(&lua, previous);
        let agents = Arc::new(
            AgentManager::new(AgentManagerParams {
                kiln_manager: kilns.clone(),
                session_manager: sessions.clone(),
                background_manager: Arc::new(BackgroundJobManager::new(events.clone())),
                mcp_gateway: None,
                llm_config: Some(llm),
                acp_config: None,
                context_config: None,
                permission_config: None,
                plugin_loader: Some(shared_loader.clone()),
                source_roots: Default::default(),
                review_snapshot_root: tmp.path().join("snapshots"),
            })
            .with_modes(Some(loader.mode_registry())),
        );
        let ctx = Arc::new(RpcContext::for_test_with_plugin_loader(
            kilns,
            sessions.clone(),
            agents.clone(),
            Arc::new(crate::project_manager::ProjectManager::new(
                tmp.path().join("projects.json"),
            )),
            events,
            tmp.path().into(),
            shared_loader.clone(),
        ));
        let bridge = Arc::new(DaemonSessionBridge::new(ctx.clone()));
        loader.upgrade_with_sessions(bridge.clone()).unwrap();
        loader
            .upgrade_with_tools(Arc::new(
                crate::tools_bridge::DaemonToolsBridge::new(
                    Arc::new(crate::tools::workspace::WorkspaceTools::new(tmp.path())),
                    None,
                )
                .with_active_tools(agents.active_tools(), sessions.clone()),
            ))
            .unwrap();
        agents.set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());
        agents.set_plugin_tool_registry(loader.plugin_registry());
        agents.set_isolation(loader.isolation());
        if let Some(code) = hook_lua {
            loader.eval(code).await.unwrap();
        }
        loader
            .add_plugin_paths(&[(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/plugins"),
                PluginSource::Runtime,
            )])
            .unwrap();
        loader.activate_plugin("reflection").await.unwrap();
        loader.eval(r#"require("reflection").setup({ model = "reflection-fixture", min_turns = 1, timeout = 5 })"#).await.unwrap();
        *shared_loader.lock().await = Some(loader);

        Self {
            _env_guards: env_guards,
            _env_lock: env_lock,
            tmp,
            kiln,
            provider,
            sessions,
            agents,
            ctx,
            bridge,
            lua,
            loader: shared_loader,
            observed,
        }
    }

    /// The shipped `auto-title` plugin, active, with a provider fixture that
    /// answers [`AUTO_TITLE_ANSWER`] for every completion.
    #[allow(clippy::await_holding_lock)]
    pub(super) async fn auto_title() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let env_lock = crate::agent_manager::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let env_guards = crate::agent_manager::clear_provider_env();

        let provider = MockServer::start().await;
        Mock::given(|request: &wiremock::Request| {
            request.method.as_str() == "POST" && request.url.path().ends_with("/api/chat")
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "llama3.2",
            "message": { "role": "assistant", "content": AUTO_TITLE_ANSWER },
            "done": true,
            "done_reason": "stop",
        })))
        .mount(&provider)
        .await;

        let tmp = TempDir::new().unwrap();
        // `auto-title` registers no kiln of its own; the directory is kept
        // only so the field is the one shape [`Rig::reflection`] uses.
        let kiln = TempDir::new().unwrap();
        let sessions = temp_session_manager();
        let shared_loader = Arc::new(tokio::sync::Mutex::new(None));
        let (events, observed) = crate::EventBus::channel(128);
        let kilns = Arc::new(KilnManager::new());
        // A configured provider is what admits a loopback endpoint.
        let mut llm = bridge_llm_config();
        llm.providers.get_mut("ollama").unwrap().endpoint = Some(provider.uri());
        let agents = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: kilns.clone(),
            session_manager: sessions.clone(),
            background_manager: Arc::new(BackgroundJobManager::new(events.clone())),
            mcp_gateway: None,
            llm_config: Some(llm),
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: Some(shared_loader.clone()),
            source_roots: Default::default(),
            review_snapshot_root: tmp.path().join("snapshots"),
        }));
        let ctx = Arc::new(RpcContext::for_test_with_plugin_loader(
            kilns,
            sessions.clone(),
            agents.clone(),
            Arc::new(crate::project_manager::ProjectManager::new(
                tmp.path().join("projects.json"),
            )),
            events,
            tmp.path().into(),
            shared_loader.clone(),
        ));
        let bridge = Arc::new(DaemonSessionBridge::new(ctx.clone()));
        let mut loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        let lua = loader.plugin_lua();
        loader.upgrade_with_sessions(bridge.clone()).unwrap();
        loader
            .add_plugin_paths(&[(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/plugins"),
                PluginSource::Runtime,
            )])
            .unwrap();
        loader.activate_plugin("auto-title").await.unwrap();
        agents.set_plugin_tool_registry(loader.plugin_registry());
        *shared_loader.lock().await = Some(loader);

        Self {
            _env_guards: env_guards,
            _env_lock: env_lock,
            tmp,
            kiln,
            provider,
            sessions,
            agents,
            ctx,
            bridge,
            lua,
            loader: shared_loader,
            observed,
        }
    }
}
