//! A user runs the shipped `auto-title` command from a session, through the
//! `plugin.run_command` handler, as the TUI does for `/generate`.
//!
//! Nothing here is scripted except the provider: the real loader activates the
//! real plugin, the real bridge answers `cru.session.messages` and
//! `cru.session.set_title`, and `cru.session.complete` reaches a wiremock
//! provider through the daemon's one-shot completion.

use super::*;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::observe::LogEvent;
use crate::protocol::{Request, RequestId};
use crucible_lua::manifest::PluginSource;
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// What the provider answers when it is asked for a title.
const ANSWER: &str = "\"Fixing the auth flow.\"";

struct Rig {
    // Dropped last: the provider environment stays clear while the rig lives.
    _env_guards: Vec<crucible_core::test_support::EnvVarGuard>,
    _env_lock: std::sync::MutexGuard<'static, ()>,
    _tmp: TempDir,
    provider: MockServer,
    sessions: Arc<SessionManager>,
    agents: Arc<AgentManager>,
    loader: Arc<tokio::sync::Mutex<Option<DaemonPluginLoader>>>,
}

impl Rig {
    #[allow(clippy::await_holding_lock)]
    async fn new() -> Self {
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
            "message": { "role": "assistant", "content": ANSWER },
            "done": true,
            "done_reason": "stop",
        })))
        .mount(&provider)
        .await;

        let tmp = TempDir::new().unwrap();
        let sessions = temp_session_manager();
        let shared_loader = Arc::new(tokio::sync::Mutex::new(None));
        let (events, _) = crate::EventBus::channel(128);
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
        let mut loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        loader
            .upgrade_with_sessions(Arc::new(DaemonSessionBridge::new(ctx)))
            .unwrap();
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
            _tmp: tmp,
            provider,
            sessions,
            agents,
            loader: shared_loader,
        }
    }

    /// A chat session on the fixture provider, with two exchanges.
    async fn session_with_two_exchanges(&self) -> String {
        let session = self
            .sessions
            .create_session(SessionType::Chat, vec![], None, None)
            .await
            .unwrap();
        let mut agent = make_test_agent(None);
        agent.endpoint = Some(self.provider.uri());
        self.agents
            .configure_agent(&session.id, agent)
            .await
            .unwrap();
        for event in [
            LogEvent::user("what is the weather"),
            LogEvent::assistant("sunny"),
            LogEvent::user("now help me fix the auth flow"),
            LogEvent::assistant("it is fixed"),
        ] {
            self.sessions
                .storage()
                .append_event(&session, &event.to_jsonl().unwrap())
                .await
                .unwrap();
        }
        session.id.to_string()
    }

    /// `plugin.run_command` as the TUI sends it for a bare `/generate`.
    async fn run(&self, session_id: Option<&str>) -> crate::protocol::Response {
        let mut params = json!({ "name": "generate" });
        if let Some(id) = session_id {
            params["session_id"] = json!(id);
        }
        crate::server::plugins::handle_plugin_run_command(
            Request {
                jsonrpc: "2.0".to_string(),
                id: Some(RequestId::Number(1)),
                method: "plugin.run_command".to_string(),
                params,
            },
            &self.loader,
        )
        .await
    }
}

/// The regression: `/generate` in the TUI raised "attempt to index userdata
/// with 'user'". It now titles the session from its latest exchange.
#[tokio::test]
async fn generate_run_from_a_session_titles_it_after_the_latest_exchange() {
    let rig = Rig::new().await;
    let id = rig.session_with_two_exchanges().await;

    let response = rig.run(Some(&id)).await;

    assert!(response.error.is_none(), "{:?}", response.error);
    assert_eq!(
        response.result.expect("a result")["result"],
        json!("Session titled: Fixing the auth flow")
    );
    assert_eq!(
        rig.sessions.get_session(&id).unwrap().title.as_deref(),
        Some("Fixing the auth flow")
    );
    let asked = rig.provider.received_requests().await.unwrap();
    let prompt = String::from_utf8_lossy(&asked.last().expect("a completion").body).to_string();
    assert!(
        prompt.contains("now help me fix the auth flow") && prompt.contains("it is fixed"),
        "the prompt must hold the latest exchange: {prompt}"
    );
    assert!(
        !prompt.contains("what is the weather"),
        "the prompt must not hold an earlier exchange: {prompt}"
    );
}

/// Without a session the command cannot know what to title, and says so.
#[tokio::test]
async fn generate_run_without_a_session_names_the_reason() {
    let rig = Rig::new().await;

    let response = rig.run(None).await;

    let error = response.error.expect("no session to title").message;
    assert!(
        error.contains("run /generate in a chat session"),
        "the error must name the reason: {error}"
    );
}
