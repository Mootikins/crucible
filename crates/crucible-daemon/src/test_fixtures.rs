//! Fixtures shared by unit tests in more than one module.
//!
//! Distinct from [`crate::test_support`], which is a `pub` module of mock trait
//! implementations that integration tests under `tests/` also use. These are
//! `#[cfg(test)]` builders for the daemon's own managers and config: they live
//! here rather than in whichever `tests` module needed them first, because the
//! alternative — the copy that `server::tests` and `session_bridge::tests` and
//! `agent_manager::tests` each keep — is how three "identical" LLM configs end
//! up disagreeing about which provider is the default.

use crate::agent_manager::{AgentManager, AgentManagerParams};
use crate::kiln_manager::KilnManager;
use crate::protocol::SessionEventMessage;
use crate::session_manager::SessionManager;
use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig, TrustLevel};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// An [`LlmConfig`] whose single provider is the default, at its backend's
/// built-in trust level.
pub(crate) fn build_llm_config(default_key: &str, provider_type: BackendType) -> LlmConfig {
    build_llm_config_with_trust(default_key, provider_type, None)
}

/// As [`build_llm_config`], with an explicit `trust_level` override — the knob
/// every kiln-classification test turns.
pub(crate) fn build_llm_config_with_trust(
    default_key: &str,
    provider_type: BackendType,
    trust_level: Option<TrustLevel>,
) -> LlmConfig {
    let mut providers = HashMap::new();
    providers.insert(
        default_key.to_string(),
        LlmProviderConfig {
            provider_type,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level,
            name: None,
        },
    );
    LlmConfig {
        default: Some(default_key.to_string()),
        providers,
        models: Default::default(),
    }
}

/// Build an `AgentManager` suitable for tests that don't actually drive an
/// agent — they just need a value to pass to a create path so the setup task
/// has a handle for `list_providers`. The returned manager has no MCP gateway,
/// no ACP config, no plugin loader.
pub(crate) fn test_agent_manager(
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    llm_config: Option<LlmConfig>,
) -> Arc<AgentManager> {
    let background_manager = Arc::new(crate::background_manager::BackgroundJobManager::new(
        event_tx,
    ));
    Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager,
        session_manager,
        background_manager,
        mcp_gateway: None,
        llm_config,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
    }))
}
