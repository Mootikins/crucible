//! Agent factory for daemon.
//!
//! Creates `AgentHandle` instances from `SessionAgent` configuration.
//! This is a simplified version of the CLI's agent factory since
//! `SessionAgent` contains fully-resolved configuration.

use crucible_config::{LlmProviderConfig, LlmProviderType};
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use crucible_rig::{create_client, AgentConfig, RigAgentHandle, RigClient};
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use tracing::{debug, info};

#[derive(Error, Debug)]
pub enum AgentFactoryError {
    #[error("Failed to create LLM client: {0}")]
    ClientCreation(String),

    #[error("Failed to build agent: {0}")]
    AgentBuild(String),

    #[error("Unsupported agent type: {0}")]
    UnsupportedAgentType(String),
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
///
/// # Returns
///
/// A boxed `AgentHandle` ready for streaming messages.
pub async fn create_agent_from_session_config(
    agent_config: &SessionAgent,
    workspace: &Path,
) -> Result<Box<dyn AgentHandle + Send + Sync>, AgentFactoryError> {
    if agent_config.agent_type != "internal" {
        return Err(AgentFactoryError::UnsupportedAgentType(format!(
            "Daemon only supports 'internal' agents, got '{}'",
            agent_config.agent_type
        )));
    }

    info!(
        provider = %agent_config.provider,
        model = %agent_config.model,
        workspace = %workspace.display(),
        "Creating agent from session config"
    );

    let provider_type = LlmProviderType::from_str(&agent_config.provider)
        .map_err(|e| AgentFactoryError::ClientCreation(e))?;

    let llm_config = LlmProviderConfig::builder(provider_type.clone())
        .maybe_endpoint(agent_config.endpoint.clone())
        .model(agent_config.model.clone())
        .api_key_from_env()
        .build();

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

    let handle: Box<dyn AgentHandle + Send + Sync> = match client {
        RigClient::Ollama(ollama_client) => {
            let agent = crucible_rig::build_agent_from_config(&rig_agent_config, &ollama_client)
                .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            let mut handle = RigAgentHandle::new(agent)
                .with_model(agent_config.model.clone())
                .with_thinking_budget(thinking_budget);
            if let Some(endpoint) = &ollama_endpoint {
                handle = handle.with_ollama_endpoint(endpoint.clone());
            }
            Box::new(handle)
        }
        RigClient::OpenAI(openai_client) => {
            let agent = crucible_rig::build_agent_from_config(&rig_agent_config, &openai_client)
                .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            let mut handle = RigAgentHandle::new(agent)
                .with_model(agent_config.model.clone())
                .with_thinking_budget(thinking_budget);
            if let Some(endpoint) = &ollama_endpoint {
                handle = handle.with_ollama_endpoint(endpoint.clone());
            }
            Box::new(handle)
        }
        RigClient::OpenAICompat(compat_client) => {
            let agent = crucible_rig::build_agent_from_config(&rig_agent_config, &compat_client)
                .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            let mut handle = RigAgentHandle::new(agent)
                .with_model(agent_config.model.clone())
                .with_thinking_budget(thinking_budget);
            if let Some(endpoint) = &ollama_endpoint {
                handle = handle.with_ollama_endpoint(endpoint.clone());
            }
            Box::new(handle)
        }
        RigClient::Anthropic(anthropic_client) => {
            let agent = crucible_rig::build_agent_from_config(&rig_agent_config, &anthropic_client)
                .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            Box::new(
                RigAgentHandle::new(agent)
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

            let agent = crucible_rig::build_agent_from_config(&rig_agent_config, &compat_client)
                .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            Box::new(
                RigAgentHandle::new(agent)
                    .with_model(agent_config.model.clone())
                    .with_thinking_budget(thinking_budget),
            )
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
        }
    }

    #[test]
    fn test_unsupported_agent_type() {
        let mut config = test_agent_config();
        config.agent_type = "acp".to_string();

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(create_agent_from_session_config(&config, Path::new("/tmp")));

        assert!(matches!(
            result,
            Err(AgentFactoryError::UnsupportedAgentType(_))
        ));
    }

    #[tokio::test]
    #[ignore = "Requires Ollama to be running"]
    async fn test_create_ollama_agent() {
        let config = test_agent_config();
        let result = create_agent_from_session_config(&config, Path::new("/tmp")).await;
        assert!(result.is_ok());
    }
}
