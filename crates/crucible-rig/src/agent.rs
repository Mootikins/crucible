//! Agent builder for creating Rig agents from Crucible AgentCards.
//!
//! This module provides utilities for building Rig agents from Crucible's
//! AgentCard configuration.

use crate::workspace_tools::{
    BashTool, EditFileTool, GlobTool, GrepTool, ReadFileTool, WorkspaceContext, WriteFileTool,
};
use crucible_core::agent::AgentCard;
use rig::agent::{Agent, AgentBuilder};
use rig::client::CompletionClient;
use rig::completion::CompletionModel;
use std::path::Path;
use thiserror::Error;

/// Errors from agent building operations
#[derive(Debug, Error)]
pub enum AgentBuildError {
    /// Missing required configuration
    #[error("Missing required configuration: {field}")]
    MissingConfig {
        /// The missing field name
        field: String,
    },

    /// Invalid configuration value
    #[error("Invalid configuration value for {field}: {reason}")]
    InvalidConfig {
        /// The field with invalid value
        field: String,
        /// Why the value is invalid
        reason: String,
    },
}

/// Result type for agent building operations
pub type AgentBuildResult<T> = Result<T, AgentBuildError>;

/// Configuration extracted from an AgentCard for building agents
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Model name to use (e.g., "llama3.2", "gpt-4o", "claude-3-5-sonnet")
    pub model: String,

    /// System prompt / preamble
    pub system_prompt: String,

    /// Optional temperature setting (0.0 - 1.0)
    pub temperature: Option<f64>,

    /// Optional max tokens
    pub max_tokens: Option<u32>,
}

impl AgentConfig {
    /// Extract agent configuration from an AgentCard
    ///
    /// The model is expected to be in `card.config["model"]`.
    /// Temperature and max_tokens are optional in config.
    pub fn from_card(card: &AgentCard) -> AgentBuildResult<Self> {
        let model = card
            .config
            .get("model")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| AgentBuildError::MissingConfig {
                field: "model".into(),
            })?;

        let temperature = card.config.get("temperature").and_then(|v| v.as_f64());

        let max_tokens = card
            .config
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        Ok(Self {
            model,
            system_prompt: card.system_prompt.clone(),
            temperature,
            max_tokens,
        })
    }

    /// Create agent config with explicit values (useful for testing)
    pub fn new(model: impl Into<String>, system_prompt: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system_prompt: system_prompt.into(),
            temperature: None,
            max_tokens: None,
        }
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f64) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }
}

/// Build a Rig agent from an AgentCard and client.
///
/// This function creates a configured Rig Agent using the system prompt
/// and configuration from the AgentCard.
///
/// # Arguments
///
/// * `card` - The AgentCard containing agent configuration
/// * `client` - A Rig client implementing CompletionClient
///
/// # Returns
///
/// A configured Rig Agent ready for prompting.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_rig::agent::build_agent;
/// use crucible_core::agent::AgentCard;
///
/// let card = load_agent_card("agents/assistant.md")?;
/// let client = ollama::Client::new();
/// let agent = build_agent(&card, &client)?;
///
/// let response = agent.prompt("Hello!").await?;
/// ```
pub fn build_agent<C>(card: &AgentCard, client: &C) -> AgentBuildResult<Agent<C::CompletionModel>>
where
    C: CompletionClient,
    C::CompletionModel: CompletionModel<Client = C>,
{
    let config = AgentConfig::from_card(card)?;
    build_agent_from_config(&config, client)
}

/// Build a Rig agent from explicit configuration and client.
///
/// This is useful when you have configuration from sources other than
/// an AgentCard, or for testing.
pub fn build_agent_from_config<C>(
    config: &AgentConfig,
    client: &C,
) -> AgentBuildResult<Agent<C::CompletionModel>>
where
    C: CompletionClient,
    C::CompletionModel: CompletionModel<Client = C>,
{
    let mut builder: AgentBuilder<C::CompletionModel> = client.agent(&config.model);

    // Set preamble (system prompt)
    builder = builder.preamble(&config.system_prompt);

    // Set temperature if specified
    if let Some(temp) = config.temperature {
        builder = builder.temperature(temp);
    }

    // Note: max_tokens would be set on individual requests, not on the agent builder
    // The Rig AgentBuilder doesn't have a max_tokens method

    Ok(builder.build())
}

/// Build a Rig agent with workspace tools from configuration and client.
///
/// This creates an agent that has access to core workspace tools:
/// - `read_file`: Read file contents with optional line range
/// - `edit_file`: Edit file via search/replace
/// - `write_file`: Write content to file
/// - `bash`: Execute shell commands
/// - `glob`: Find files by pattern
/// - `grep`: Search file contents with regex
///
/// # Arguments
///
/// * `config` - Agent configuration (model, system prompt, etc.)
/// * `client` - A Rig client implementing CompletionClient
/// * `workspace_root` - Root directory for workspace operations
///
/// # Example
///
/// ```rust,ignore
/// use crucible_rig::agent::{build_agent_with_tools, AgentConfig};
/// use rig::providers::ollama;
///
/// let config = AgentConfig::new("llama3.2", "You are a helpful coding assistant.");
/// let client = ollama::Client::new();
/// let agent = build_agent_with_tools(&config, &client, "/path/to/project")?;
/// ```
pub fn build_agent_with_tools<C>(
    config: &AgentConfig,
    client: &C,
    workspace_root: impl AsRef<Path>,
) -> AgentBuildResult<Agent<C::CompletionModel>>
where
    C: CompletionClient,
    C::CompletionModel: CompletionModel<Client = C>,
{
    let ctx = WorkspaceContext::new(workspace_root.as_ref());

    let mut builder: AgentBuilder<C::CompletionModel> = client.agent(&config.model);

    // Set preamble (system prompt)
    builder = builder.preamble(&config.system_prompt);

    // Set temperature if specified
    if let Some(temp) = config.temperature {
        builder = builder.temperature(temp);
    }

    // Add workspace tools
    let agent = builder
        .tool(ReadFileTool::new(ctx.clone()))
        .tool(EditFileTool::new(ctx.clone()))
        .tool(WriteFileTool::new(ctx.clone()))
        .tool(BashTool::new(ctx.clone()))
        .tool(GlobTool::new(ctx.clone()))
        .tool(GrepTool::new(ctx))
        .build();

    Ok(agent)
}

/// Build a Rig agent with size-appropriate tools.
///
/// This creates an agent with tools selected based on model size:
/// - Small models (< 4B): read-only tools (read_file, glob, grep)
/// - Medium/Large models: all tools including write operations
///
/// # Arguments
///
/// * `config` - Agent configuration (model, system prompt, etc.)
/// * `client` - A Rig client implementing CompletionClient
/// * `workspace_root` - Root directory for workspace operations
/// * `model_size` - Model size category for tool selection
///
/// # Example
///
/// ```rust,ignore
/// use crucible_rig::agent::{build_agent_with_model_size, AgentConfig};
/// use crucible_core::prompts::ModelSize;
/// use rig::providers::ollama;
///
/// let config = AgentConfig::new("granite-3b", "You are a helpful assistant.");
/// let client = ollama::Client::new();
/// let agent = build_agent_with_model_size(&config, &client, "/path/to/project", ModelSize::Small)?;
/// ```
pub fn build_agent_with_model_size<C>(
    config: &AgentConfig,
    client: &C,
    workspace_root: impl AsRef<Path>,
    model_size: crucible_core::prompts::ModelSize,
) -> AgentBuildResult<Agent<C::CompletionModel>>
where
    C: CompletionClient,
    C::CompletionModel: CompletionModel<Client = C>,
{
    let ctx = WorkspaceContext::new(workspace_root.as_ref());

    let mut builder: AgentBuilder<C::CompletionModel> = client.agent(&config.model);

    // Set preamble (system prompt)
    builder = builder.preamble(&config.system_prompt);

    // Set temperature if specified
    if let Some(temp) = config.temperature {
        builder = builder.temperature(temp);
    }

    // Add tools based on model size
    // Note: Rig's AgentBuilder requires static tool types, so we conditionally add them
    if model_size.is_read_only() {
        // Small models: read-only tools only
        let agent = builder
            .tool(ReadFileTool::new(ctx.clone()))
            .tool(GlobTool::new(ctx.clone()))
            .tool(GrepTool::new(ctx))
            .build();
        Ok(agent)
    } else {
        // Medium/Large models: all tools
        let agent = builder
            .tool(ReadFileTool::new(ctx.clone()))
            .tool(EditFileTool::new(ctx.clone()))
            .tool(WriteFileTool::new(ctx.clone()))
            .tool(BashTool::new(ctx.clone()))
            .tool(GlobTool::new(ctx.clone()))
            .tool(GrepTool::new(ctx))
            .build();
        Ok(agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rig::client::Nothing;
    use rig::providers::ollama;
    use std::collections::HashMap;
    use uuid::Uuid;

    // Helper to create a test Ollama client (with explicit type)
    fn test_ollama_client() -> ollama::Client {
        ollama::Client::builder().api_key(Nothing).build().unwrap()
    }

    fn make_test_card(model: &str, system_prompt: &str) -> AgentCard {
        let mut config = HashMap::new();
        config.insert("model".to_string(), serde_json::json!(model));

        AgentCard {
            id: Uuid::new_v4(),
            name: "test-agent".into(),
            version: "1.0.0".into(),
            description: "A test agent".into(),
            tags: vec!["test".into()],
            system_prompt: system_prompt.into(),
            mcp_servers: vec![],
            config,
            loaded_at: Utc::now(),
        }
    }

    fn make_test_card_with_temperature(model: &str, system_prompt: &str, temp: f64) -> AgentCard {
        let mut card = make_test_card(model, system_prompt);
        card.config
            .insert("temperature".to_string(), serde_json::json!(temp));
        card
    }

    #[test]
    fn test_agent_config_from_card() {
        let card = make_test_card("llama3.2", "You are a helpful assistant.");

        let config = AgentConfig::from_card(&card).unwrap();

        assert_eq!(config.model, "llama3.2");
        assert_eq!(config.system_prompt, "You are a helpful assistant.");
        assert!(config.temperature.is_none());
        assert!(config.max_tokens.is_none());
    }

    #[test]
    fn test_agent_config_with_temperature() {
        let card = make_test_card_with_temperature("llama3.2", "You are helpful.", 0.7);

        let config = AgentConfig::from_card(&card).unwrap();

        assert_eq!(config.temperature, Some(0.7));
    }

    #[test]
    fn test_agent_config_missing_model() {
        let card = AgentCard {
            id: Uuid::new_v4(),
            name: "test".into(),
            version: "1.0.0".into(),
            description: "test".into(),
            tags: vec![],
            system_prompt: "test".into(),
            mcp_servers: vec![],
            config: HashMap::new(), // No model!
            loaded_at: Utc::now(),
        };

        let result = AgentConfig::from_card(&card);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AgentBuildError::MissingConfig { field } if field == "model"
        ));
    }

    #[tokio::test]
    async fn test_build_agent_from_card() {
        // Create a test card
        let card = make_test_card("llama3.2", "You are a helpful assistant.");

        // Create an Ollama client (doesn't require network for building)
        let client = test_ollama_client();

        // Build the agent - this should succeed without network
        let result = build_agent(&card, &client);

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_build_agent_with_temperature() {
        let card = make_test_card_with_temperature("llama3.2", "You are helpful.", 0.5);

        let client = test_ollama_client();

        let result = build_agent(&card, &client);

        assert!(result.is_ok());
        // The agent is built with temperature 0.5
        // We can't easily verify the temperature was set without making a call
    }

    #[tokio::test]
    async fn test_build_agent_from_config() {
        let config =
            AgentConfig::new("llama3.2", "You are a test assistant.").with_temperature(0.8);

        let client = test_ollama_client();

        let result = build_agent_from_config(&config, &client);

        assert!(result.is_ok());
    }

    #[test]
    fn test_agent_config_builder_pattern() {
        let config = AgentConfig::new("gpt-4o", "You are helpful.")
            .with_temperature(0.7)
            .with_max_tokens(1000);

        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.system_prompt, "You are helpful.");
        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.max_tokens, Some(1000));
    }
}
