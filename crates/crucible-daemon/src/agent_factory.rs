//! Agent factory for daemon.
//!
//! Creates `AgentHandle` instances from `SessionAgent` configuration.
//! This is a simplified version of the CLI's agent factory since
//! `SessionAgent` contains fully-resolved configuration.

use crate::acp_handle::AcpAgentHandle;
use crate::protocol::SessionEventMessage;
use crucible_config::{LlmProviderConfig, LlmProviderType};
use crucible_core::background::BackgroundSpawner;
use crucible_core::interaction_registry::InteractionRegistry;
use crucible_core::prompts::ModelSize;
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use crucible_core::{EventPushCallback, InteractionContext};
use crucible_rig::{
    build_agent_with_model_size, create_client, mcp_tools_from_gateway, AgentComponents,
    AgentConfig, McpProxyTool, RigAgentHandle, RigClient, WorkspaceContext,
};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{debug, info};
use crucible_config::credentials::SecretsFile;

#[derive(Error, Debug)]
pub enum AgentFactoryError {
    #[error("Failed to create LLM client: {0}")]
    ClientCreation(String),

    #[error("Failed to build agent: {0}")]
    AgentBuild(String),

    #[error("Unsupported agent type: {0}")]
    UnsupportedAgentType(String),
}

/// Resolve OAuth token for GitHub Copilot from credential store
///
/// Resolution order:
/// 1. Environment variable (GITHUB_COPILOT_OAUTH_TOKEN)
/// 2. Credential store (secrets.toml)
/// 3. Config api_key (fallback)
fn resolve_copilot_oauth_token(config_api_key: Option<&str>) -> Option<String> {
    // Check environment variable first
    if let Ok(token) = std::env::var("GITHUB_COPILOT_OAUTH_TOKEN") {
        if !token.is_empty() {
            debug!("Using GitHub Copilot OAuth token from environment variable");
            return Some(token);
        }
    }

    // Check credential store
    let secrets = SecretsFile::new();
    if let Ok(Some(token)) = secrets.get_oauth_token("github-copilot") {
        debug!("Using GitHub Copilot OAuth token from credential store");
        return Some(token);
    }

    // Fall back to config api_key
    if let Some(key) = config_api_key {
        if !key.is_empty() {
            debug!("Using GitHub Copilot OAuth token from config");
            return Some(key.to_string());
        }
    }

    None
}

/// Create an agent handle from session configuration.
///
/// This takes the fully-resolved `SessionAgent` and creates a ready-to-use
/// `Box<dyn AgentHandle>`. Unlike the CLI factory, this doesn't need to:
/// - Discover skills (already in system_prompt)
/// - Load rules files (already in system_prompt)
/// - Apply size-aware prompts (client chose the prompt)
///
/// # Arguments
///
/// * `agent_config` - The session agent configuration
/// * `workspace` - Working directory for the agent (for workspace tools)
/// * `background_spawner` - Optional spawner for background tasks (subagents, long bash)
/// * `event_tx` - Broadcast sender for session events (used for InteractionContext)
///
/// # Returns
///
/// A boxed `AgentHandle` ready for streaming messages.
pub async fn create_agent_from_session_config(
    agent_config: &SessionAgent,
    workspace: &Path,
    background_spawner: Option<Arc<dyn BackgroundSpawner>>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
) -> Result<Box<dyn AgentHandle + Send + Sync>, AgentFactoryError> {
    if agent_config.agent_type == "acp" {
        let handle = AcpAgentHandle::new(agent_config, workspace, None, None, None, None)
            .await
            .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
        return Ok(Box::new(handle));
    }

    if agent_config.agent_type != "internal" {
        return Err(AgentFactoryError::UnsupportedAgentType(format!(
            "Daemon only supports 'internal' and 'acp' agents, got '{}'",
            agent_config.agent_type
        )));
    }

    let mcp_tools: Vec<McpProxyTool> = if let Some(ref gw) = mcp_gateway {
        let gw_read = gw.read().await;
        debug!(
            tool_count = gw_read.tool_count(),
            mcp_servers = ?agent_config.mcp_servers,
            "MCP gateway available for agent"
        );
        if !agent_config.mcp_servers.is_empty() {
            let all_tools = gw_read.all_tools();
            drop(gw_read);
            let tools = mcp_tools_from_gateway(gw, &agent_config.mcp_servers, &all_tools);
            info!(count = tools.len(), servers = ?agent_config.mcp_servers, "Resolved MCP proxy tools");
            tools
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    info!(
        provider = %agent_config.provider,
        model = %agent_config.model,
        workspace = %workspace.display(),
        "Creating agent from session config"
    );

    let provider_type = LlmProviderType::from_str(&agent_config.provider)
        .map_err(AgentFactoryError::ClientCreation)?;

    let mut llm_config = LlmProviderConfig::builder(provider_type)
        .maybe_endpoint(agent_config.endpoint.clone())
        .model(agent_config.model.clone())
        .api_key_from_env()
        .build();

    // For GitHub Copilot, resolve OAuth token from credential store
    if provider_type == LlmProviderType::GitHubCopilot {
        if let Some(oauth_token) = resolve_copilot_oauth_token(llm_config.api_key.as_deref()) {
            llm_config.api_key = Some(oauth_token);
        }
    }

    let client =
        create_client(&llm_config).map_err(|e| AgentFactoryError::ClientCreation(e.to_string()))?;

    let mut rig_agent_config = AgentConfig::new(&agent_config.model, &agent_config.system_prompt);
    if let Some(temp) = agent_config.temperature {
        rig_agent_config = rig_agent_config.with_temperature(temp);
    }
    if let Some(tokens) = agent_config.max_tokens {
        rig_agent_config = rig_agent_config.with_max_tokens(tokens);
    }

    debug!(
        model = %agent_config.model,
        prompt_len = agent_config.system_prompt.len(),
        "Building Rig agent"
    );

    let ollama_endpoint = agent_config.endpoint.clone();
    let thinking_budget = agent_config.thinking_budget;
    let model_size = ModelSize::from_model_name(&agent_config.model);

    // Create InteractionContext for ask_user tool support
    let registry = Arc::new(tokio::sync::Mutex::new(InteractionRegistry::new()));
    let event_tx_clone = event_tx.clone();
    let push_event: EventPushCallback = Arc::new(move |_event| {
        // TODO: Convert SessionEvent to SessionEventMessage and send
        // For now, events are handled through the agent's event stream
        let _ = event_tx_clone.send(SessionEventMessage::new(
            "session",
            "interaction_event",
            serde_json::json!({}),
        ));
    });
    let interaction_ctx = Arc::new(InteractionContext::new(registry, push_event));

    let mut ws_ctx = WorkspaceContext::new(workspace).with_interaction_context(interaction_ctx);
    if let Some(ref spawner) = background_spawner {
        ws_ctx = ws_ctx.with_background_spawner(spawner.clone());
    }

    let handle: Box<dyn AgentHandle + Send + Sync> = match client {
        RigClient::Ollama(ref ollama_client) => {
            let agent = build_agent_with_model_size(
                &rig_agent_config,
                ollama_client,
                &ws_ctx,
                model_size,
                mcp_tools,
            )
            .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            let mut components =
                AgentComponents::new(rig_agent_config.clone(), client, ws_ctx.clone())
                    .with_model_size(model_size);
            if let Some(budget) = thinking_budget {
                components = components.with_thinking_budget(budget);
            }
            if let Some(ref endpoint) = ollama_endpoint {
                components = components.with_ollama_endpoint(endpoint.clone());
            }
            if let Some(gw) = mcp_gateway {
                components = components.with_mcp_gateway(gw);
            }
            let handle = RigAgentHandle::new(agent)
                .with_ollama_components(components)
                .with_model(agent_config.model.clone())
                .with_thinking_budget(thinking_budget);
            Box::new(handle)
        }
        RigClient::OpenAI(openai_client) => {
            let agent = build_agent_with_model_size(
                &rig_agent_config,
                &openai_client,
                &ws_ctx,
                model_size,
                mcp_tools,
            )
            .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            let mut handle = RigAgentHandle::new(agent)
                .with_workspace_context(ws_ctx)
                .with_model(agent_config.model.clone())
                .with_thinking_budget(thinking_budget);
            if let Some(endpoint) = &ollama_endpoint {
                handle = handle.with_ollama_endpoint(endpoint.clone());
            }
            Box::new(handle)
        }
        RigClient::OpenAICompat(compat_client) => {
            let agent = build_agent_with_model_size(
                &rig_agent_config,
                &compat_client,
                &ws_ctx,
                model_size,
                mcp_tools,
            )
            .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            let mut handle = RigAgentHandle::new(agent)
                .with_workspace_context(ws_ctx)
                .with_model(agent_config.model.clone())
                .with_thinking_budget(thinking_budget);
            if let Some(endpoint) = &ollama_endpoint {
                handle = handle.with_ollama_endpoint(endpoint.clone());
            }
            Box::new(handle)
        }
        RigClient::Anthropic(anthropic_client) => {
            let agent = build_agent_with_model_size(
                &rig_agent_config,
                &anthropic_client,
                &ws_ctx,
                model_size,
                mcp_tools,
            )
            .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            Box::new(
                RigAgentHandle::new(agent)
                    .with_workspace_context(ws_ctx)
                    .with_model(agent_config.model.clone())
                    .with_thinking_budget(thinking_budget),
            )
        }
        RigClient::GitHubCopilot(copilot_client) => {
            let api_token = copilot_client
                .api_token()
                .await
                .map_err(|e| AgentFactoryError::ClientCreation(format!("Copilot auth: {}", e)))?;
            let api_base = copilot_client
                .api_base()
                .await
                .map_err(|e| AgentFactoryError::ClientCreation(format!("Copilot base: {}", e)))?;

            let compat_client = crucible_rig::create_openai_compat_client(&api_token, &api_base)
                .map_err(|e| AgentFactoryError::ClientCreation(e.to_string()))?;

            let agent = build_agent_with_model_size(
                &rig_agent_config,
                &compat_client,
                &ws_ctx,
                model_size,
                mcp_tools,
            )
            .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            Box::new(
                RigAgentHandle::new(agent)
                    .with_workspace_context(ws_ctx)
                    .with_model(agent_config.model.clone())
                    .with_thinking_budget(thinking_budget),
            )
        }
        RigClient::OpenRouter(openrouter_client) => {
            let agent = build_agent_with_model_size(
                &rig_agent_config,
                &openrouter_client,
                &ws_ctx,
                model_size,
                mcp_tools,
            )
            .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            let mut handle = RigAgentHandle::new(agent)
                .with_workspace_context(ws_ctx)
                .with_model(agent_config.model.clone())
                .with_thinking_budget(thinking_budget);
            if let Some(endpoint) = &ollama_endpoint {
                handle = handle.with_ollama_endpoint(endpoint.clone());
            }
            Box::new(handle)
        }
    };

    info!(
        provider = %agent_config.provider,
        model = %agent_config.model,
        "Agent created successfully"
    );

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_agent_config() -> SessionAgent {
        SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some("ollama".to_string()),
            provider: "ollama".to_string(),
            model: "llama3.2".to_string(),
            system_prompt: "You are a helpful assistant.".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: None,
            env_overrides: HashMap::new(),
            mcp_servers: Vec::new(),
            agent_card_name: None,
            capabilities: None,
            agent_description: None,
            delegation_config: None,
        }
    }

    #[test]
    fn test_unsupported_agent_type() {
        let mut config = test_agent_config();
        config.agent_type = "unknown".to_string();

        let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (event_tx, _) = broadcast::channel(16);
            create_agent_from_session_config(&config, Path::new("/tmp"), None, &event_tx, None)
                .await
        });

        assert!(matches!(
            result,
            Err(AgentFactoryError::UnsupportedAgentType(_))
        ));
    }

    #[tokio::test]
    #[ignore = "Requires Ollama to be running"]
    async fn test_create_ollama_agent() {
        let config = test_agent_config();
        let (event_tx, _) = broadcast::channel(16);
        let result =
            create_agent_from_session_config(&config, Path::new("/tmp"), None, &event_tx, None)
                .await;
        assert!(result.is_ok());
    }
}
