//! Knob reads and writes bring an ACP session's agent up without a send.
//!
//! Everything a front end draws about an ACP session — its modes, its model
//! selector, its knob support — is answered from the surface the ACP
//! handshake fills, and the handshake is also the resume (`session/resume`
//! with the persisted agent session id). The handle used to come up only
//! with the first message, so a resumed session's dropdowns answered from
//! Crucible's fallbacks — the Lua-declared mode set, the configured
//! providers — until the user sent something, and every knob write refused
//! with "send a message first".
//!
//! The fake here is a stand-in for `AcpAgentHandle` post-handshake: it
//! declares modes and a model selector and answers `fetch_available_models`.
//! The real spawn/resume crossing is `acp_smoke`'s; what this pins is that
//! the MANAGER conjures the handle on a knob read and serves the surface.

use super::super::*;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use crucible_core::types::acp::schema::{
    SessionConfigOption, SessionMode, SessionModeId, SessionModeState,
};

const MODES: &[(&str, &str)] = &[("code", "Code"), ("ask", "Ask"), ("architect", "Architect")];

fn fake_modes() -> SessionModeState {
    SessionModeState::new(
        SessionModeId::new("code"),
        MODES
            .iter()
            .map(|(id, name)| SessionMode::new(SessionModeId::new(*id), name.to_string()))
            .collect(),
    )
}

/// The model selector the way the wire carries it — the shape
/// claude-agent-acp sends (`acp/session.rs`'s parser fixture pins it).
fn fake_config_options() -> Vec<SessionConfigOption> {
    serde_json::from_value(serde_json::json!([
        {
            "id": "model",
            "name": "Model",
            "category": "model",
            "type": "select",
            "currentValue": "mock-sonnet",
            "options": [
                {"value": "mock-sonnet", "name": "Mock Sonnet"},
                {"value": "mock-opus", "name": "Mock Opus"}
            ]
        }
    ]))
    .expect("the selector parses into the ACP schema")
}

struct FakeAcpAgent {
    modes: SessionModeState,
    options: Vec<SessionConfigOption>,
}

impl FakeAcpAgent {
    fn new() -> Self {
        Self {
            modes: fake_modes(),
            options: fake_config_options(),
        }
    }
}

crucible_core::impl_noop_agent!(FakeAcpAgent);

#[async_trait::async_trait]
impl AgentHandle for FakeAcpAgent {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }

    fn get_mode_id(&self) -> &str {
        "code"
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        Some(&self.modes)
    }

    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl SessionKnobs for FakeAcpAgent {
    fn get_system_prompt(&self) -> Option<String> {
        None
    }

    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        Ok(())
    }

    fn current_model(&self) -> Option<&str> {
        Some("mock-sonnet")
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        vec!["mock-sonnet".to_string(), "mock-opus".to_string()]
    }

    async fn fetch_available_modes(&mut self) -> Vec<String> {
        MODES.iter().map(|(id, _)| id.to_string()).collect()
    }

    async fn set_context_strategy(
        &mut self,
        _strategy: crucible_core::session::ContextStrategy,
    ) -> ChatResult<()> {
        Ok(())
    }

    fn get_context_strategy(&self) -> crucible_core::session::ContextStrategy {
        crucible_core::session::ContextStrategy::default()
    }

    async fn set_precognition(&mut self, _enabled: bool) -> ChatResult<()> {
        Ok(())
    }

    fn get_precognition(&self) -> bool {
        true
    }

    fn agent_config_options(&self) -> &[SessionConfigOption] {
        &self.options
    }
}

/// Agent-factory override returning the fake for every session.
fn acp_factory() -> AgentFactoryOverride {
    Box::new(|_agent_config: &SessionAgent, _workspace: &Path| {
        Box::pin(async { Ok(Box::new(FakeAcpAgent::new()) as Box<dyn AgentHandle + Send + Sync>) })
            as Pin<
                Box<dyn Future<Output = Result<Box<dyn AgentHandle + Send + Sync>, String>> + Send>,
            >
    })
}

/// An ACP-typed session with the fake factory installed. No handle exists
/// yet: nothing sent a message.
async fn acp_session() -> (Arc<AgentManager>, Arc<SessionManager>, String) {
    let session_manager = temp_session_manager();
    let agent_manager = Arc::new(create_test_agent_manager(session_manager.clone()));

    let agent = SessionAgent {
        mode: None,
        agent_type: "acp".to_string(),
        agent_name: Some("fake-acp".to_string()),
        provider_key: None,
        provider: crucible_core::config::BackendType::Mock,
        model: "mock-model".to_string(),
        system_prompt: String::new(),
        max_context_tokens: None,
        endpoint: None,
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        context_budget: None,
        context_strategy: Default::default(),
        tool_policy: None,
    };

    let mut session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("session");
    session.agent = Some(agent);
    session_manager
        .update_session(&session)
        .await
        .expect("persist agent");

    agent_manager.set_agent_factory_override(acp_factory());

    let id = session.id.to_string();
    (agent_manager, session_manager, id)
}

#[tokio::test]
async fn listing_models_brings_the_acp_agent_up_and_answers_its_own_models() {
    let (agent_manager, _sm, session_id) = acp_session().await;

    let models = agent_manager.list_models(&session_id, None).await.unwrap();

    assert_eq!(
        models,
        vec!["mock-sonnet".to_string(), "mock-opus".to_string()],
        "the session's model list must be the agent's advertised selector, \
         not the daemon's configured providers"
    );
}

#[tokio::test]
async fn session_modes_bring_the_acp_agent_up_and_answer_its_own_modes() {
    let (agent_manager, _sm, session_id) = acp_session().await;
    let (event_tx, _rx) = tokio::sync::broadcast::channel::<SessionEventMessage>(64);

    let modes = agent_manager
        .live_session_modes(&session_id, Some(&event_tx))
        .await;

    assert_eq!(
        modes.current_mode_id.0.as_ref(),
        "code",
        "the agent's declared current mode must reach the front end, not the \
         Lua registry's stand-in; got {:?}",
        modes.available_modes
    );
    assert!(
        !modes
            .available_modes
            .iter()
            .any(|m| m.id.0.as_ref() == "plan"),
        "Crucible's internal mode set must not be offered for an ACP session \
         that declares its own; got {:?}",
        modes.available_modes
    );
}

#[tokio::test]
async fn session_knobs_report_the_advertised_model_selector_without_a_send() {
    let (agent_manager, _sm, session_id) = acp_session().await;
    let (event_tx, _rx) = tokio::sync::broadcast::channel::<SessionEventMessage>(64);

    let knobs = agent_manager
        .live_session_knobs(&session_id, Some(&event_tx))
        .await;
    let model_knob = knobs
        .iter()
        .find(|(knob, _)| knob.id() == "model")
        .expect("the model knob is always listed");

    assert!(
        model_knob.1,
        "the agent advertised a model selector at its handshake, so the knob \
         must read supported; supported knobs: {:?}",
        knobs
    );
}

/// The write that used to refuse with "send a message first" now brings the
/// agent up itself: a model switch on a fresh session lands on the live
/// agent, and the agent's selector reports the new current value.
#[tokio::test]
async fn switching_model_on_a_fresh_acp_session_bring_the_agent_up_first() {
    let (agent_manager, _sm, session_id) = acp_session().await;
    let (event_tx, _rx) = tokio::sync::broadcast::channel::<SessionEventMessage>(64);

    agent_manager
        .switch_model(&session_id, "mock-opus", Some(&event_tx))
        .await
        .expect("the switch must ensure the handle, not refuse");

    let models = agent_manager.list_models(&session_id, None).await.unwrap();
    assert_eq!(
        models,
        vec!["mock-sonnet".to_string(), "mock-opus".to_string()]
    );
}
