//! Which API key a session's provider sends, proven at the HTTP boundary.
//!
//! The provider is a wiremock server that answers only a request with the
//! right bearer token. A session on the provider key `openrouter-work`
//! (backend `openrouter`) asks it for one completion through `complete_once`,
//! which builds its client the way a turn does.
//!
//! The user's provider was `zai-coding` (backend `zai`). The test does not use
//! `zai`: genai's z.ai adapter replaces every endpoint with the real z.ai
//! host, so a mock cannot stand in for it, and the test would reach the
//! network. The key lookup does not depend on the backend.

use super::*;
use crucible_core::config::credentials::SecretsFile;
use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
use crucible_core::session::SessionAgent;
use wiremock::matchers::{header, method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The provider key of the session, which is not the backend name.
const PROVIDER: &str = "openrouter-work";

/// A provider environment with no key in the process environment and an
/// empty credential store of its own.
struct Rig {
    // Dropped last: the environment stays clear while the rig lives.
    _env: Vec<EnvVarGuard>,
    _env_lock: std::sync::MutexGuard<'static, ()>,
    home: TempDir,
    provider: MockServer,
}

impl Rig {
    async fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let env_lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = TempDir::new().unwrap();
        let mut env = clear_provider_env();
        env.push(EnvVarGuard::set(
            "XDG_CONFIG_HOME",
            home.path().display().to_string(),
        ));
        let provider = MockServer::start().await;
        Self {
            _env: env,
            _env_lock: env_lock,
            home,
            provider,
        }
    }

    /// The provider answers `key`, and refuses everything else with a 401.
    async fn accept_only(&self, key: &str) {
        Mock::given(method("POST"))
            .and(path_regex("/chat/completions$"))
            .and(header("authorization", format!("Bearer {key}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "c1",
                "object": "chat.completion",
                "created": 0,
                "model": "glm-5.3",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "answered" },
                    "finish_reason": "stop",
                }],
                "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 },
            })))
            .with_priority(1)
            .mount(&self.provider)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": { "code": "401", "message": "token expired or incorrect" },
            })))
            .with_priority(2)
            .mount(&self.provider)
            .await;
    }

    /// The credential store that `cru auth login` writes, in this rig's home.
    fn store(&self) -> SecretsFile {
        SecretsFile::with_path(self.home.path().join("crucible").join("secrets.toml"))
    }

    /// `llm.providers.openrouter-work`, on the mock, with `api_key`.
    fn llm_config(&self, api_key: Option<&str>) -> LlmConfig {
        let mut providers = std::collections::BTreeMap::new();
        providers.insert(
            PROVIDER.to_string(),
            LlmProviderConfig {
                provider_type: BackendType::OpenRouter,
                endpoint: Some(self.provider.uri()),
                default_model: Some("glm-5.3".to_string()),
                api_key: api_key.map(str::to_string),
                available_models: None,
                trust_level: None,
                name: None,
            },
        );
        LlmConfig {
            default: Some(PROVIDER.to_string()),
            providers,
            models: Default::default(),
        }
    }

    /// A chat session on `openrouter-work`, and the manager that holds it.
    async fn session(&self, api_key: Option<&str>) -> (Arc<AgentManager>, String) {
        let sm = temp_session_manager();
        let am = create_test_agent_manager_with_llm_config(sm.clone(), self.llm_config(api_key));
        let session = sm
            .create_session(SessionType::Chat, vec![], None, None)
            .await
            .unwrap();
        am.configure_agent(&session.id, agent_on_the_mock(&self.provider.uri()))
            .await
            .unwrap();
        (am, session.id.to_string())
    }
}

fn agent_on_the_mock(endpoint: &str) -> SessionAgent {
    SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some(PROVIDER.to_string()),
        provider: BackendType::OpenRouter,
        model: "glm-5.3".to_string(),
        system_prompt: "You are helpful.".to_string(),
        max_context_tokens: None,
        endpoint: Some(endpoint.to_string()),
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        context_budget: None,
        context_strategy: Default::default(),
        tool_policy: None,
    }
}

fn prompt() -> crate::agent_manager::completion::OneShotParams {
    serde_json::from_value(serde_json::json!({ "prompt": "hello" })).unwrap()
}

/// The regression: `cru auth login --provider zai-coding` stored the key under
/// `zai-coding`, and the client looked only under the backend name `zai`, so
/// it sent no key.
#[tokio::test]
async fn a_key_stored_under_the_provider_key_reaches_the_provider() {
    let rig = Rig::new().await;
    rig.accept_only("stored-key").await;
    rig.store().set(PROVIDER, "stored-key").unwrap();
    let (am, id) = rig.session(None).await;

    let answer = am.complete_once(&id, prompt()).await;

    assert_eq!(answer.expect("the stored key must be sent"), "answered");
}

/// `llm.providers.<key>.api_key` is the key the user configured for this
/// provider. The client never read it.
#[tokio::test]
async fn a_configured_api_key_reaches_the_provider() {
    let rig = Rig::new().await;
    rig.accept_only("config-key").await;
    let (am, id) = rig.session(Some("config-key")).await;

    let answer = am.complete_once(&id, prompt()).await;

    assert_eq!(answer.expect("the configured key must be sent"), "answered");
}

/// Rule 7: a turn on a provider that needs a key, with no key anywhere, is
/// refused at once, and the refusal names the provider and the command that
/// stores a key. The turn used to start, send no key, and fail later as
/// "genai stream error: Web stream error for model …".
#[tokio::test]
async fn a_turn_without_a_key_is_refused_before_it_reaches_the_provider() {
    let rig = Rig::new().await;
    rig.accept_only("never-sent").await;
    let (am, id) = rig.session(None).await;
    let (tx, _rx) = tokio::sync::broadcast::channel(64);

    let refused = am
        .send_message(&id, "hello".to_string(), &tx, true, None)
        .await
        .expect_err("a turn with no key must not start");

    let message = refused.to_string();
    assert!(
        message.contains("No API key for provider 'openrouter-work'")
            && message.contains("OPENROUTER_API_KEY")
            && message.contains("cru auth login --provider openrouter-work"),
        "the refusal must name the provider and the fix: {message}"
    );
    assert!(
        rig.provider.received_requests().await.unwrap().is_empty(),
        "a refused turn must not reach the provider"
    );
}
