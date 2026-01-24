//! Agent builder for creating Rig agents from Crucible AgentCards.
//!
//! This module provides utilities for building Rig agents from Crucible's
//! AgentCard configuration.
//!
//! ## Agent Components Pattern
//!
//! For runtime model switching, agents are built from reusable [`AgentComponents`]:
//!
//! ```rust,ignore
//! use crucible_rig::agent::{AgentComponents, build_agent_from_components_generic};
//!
//! // Create components once
//! let components = AgentComponents::new(config, client, workspace_ctx)
//!     .with_kiln(kiln_ctx)
//!     .with_model_size(ModelSize::Medium);
//!
//! // Build initial agent
//! let handle = rebuild_agent_handle(&components, "llama3.2")?;
//!
//! // Later: switch model by rebuilding with same components
//! let new_handle = rebuild_agent_handle(&components, "qwen3-8b")?;
//! ```

use crate::kiln_tools::{KilnContext, ListNotesTool, ReadNoteTool, SemanticSearchTool};
use crate::providers::RigClient;
use crate::workspace_tools::{
    BashTool, CancelTaskTool, EditFileTool, GetTaskResultTool, GlobTool, GrepTool,
    ListBackgroundTasksTool, ReadFileTool, SpawnSubagentTool, WorkspaceContext, WriteFileTool,
};
use crucible_core::agent::AgentCard;
use crucible_core::prompts::ModelSize;
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

fn configure_builder<C>(client: &C, config: &AgentConfig) -> AgentBuilder<C::CompletionModel>
where
    C: CompletionClient,
    C::CompletionModel: CompletionModel<Client = C>,
{
    let mut builder = client.agent(&config.model);
    builder = builder.preamble(&config.system_prompt);

    if let Some(temp) = config.temperature {
        builder = builder.temperature(temp);
    }

    if let Some(ref params) = config.additional_params {
        builder = builder.additional_params(params.clone());
    }

    builder
}

fn is_read_only_mode(mode_id: &str) -> bool {
    mode_id == "plan"
}

fn attach_tools<M: CompletionModel>(
    builder: AgentBuilder<M>,
    ctx: &WorkspaceContext,
    kiln_ctx: Option<&KilnContext>,
    mode_id: &str,
) -> Agent<M> {
    let read_only = is_read_only_mode(mode_id);
    let has_background = ctx.has_background_spawner();

    match (read_only, kiln_ctx, has_background) {
        (true, None, _) => builder
            .tool(ReadFileTool::new(ctx.clone()))
            .tool(GlobTool::new(ctx.clone()))
            .tool(GrepTool::new(ctx.clone()))
            .build(),
        (true, Some(kiln), _) => builder
            .tool(ReadFileTool::new(ctx.clone()))
            .tool(GlobTool::new(ctx.clone()))
            .tool(GrepTool::new(ctx.clone()))
            .tool(SemanticSearchTool::new(kiln.clone()))
            .tool(ReadNoteTool::new(kiln.clone()))
            .tool(ListNotesTool::new(kiln.clone()))
            .build(),
        (false, None, false) => builder
            .tool(ReadFileTool::new(ctx.clone()))
            .tool(EditFileTool::new(ctx.clone()))
            .tool(WriteFileTool::new(ctx.clone()))
            .tool(BashTool::new(ctx.clone()))
            .tool(GlobTool::new(ctx.clone()))
            .tool(GrepTool::new(ctx.clone()))
            .build(),
        (false, None, true) => builder
            .tool(ReadFileTool::new(ctx.clone()))
            .tool(EditFileTool::new(ctx.clone()))
            .tool(WriteFileTool::new(ctx.clone()))
            .tool(BashTool::new(ctx.clone()))
            .tool(GlobTool::new(ctx.clone()))
            .tool(GrepTool::new(ctx.clone()))
            .tool(ListBackgroundTasksTool::new(ctx.clone()))
            .tool(GetTaskResultTool::new(ctx.clone()))
            .tool(CancelTaskTool::new(ctx.clone()))
            .tool(SpawnSubagentTool::new(ctx.clone()))
            .build(),
        (false, Some(kiln), false) => builder
            .tool(ReadFileTool::new(ctx.clone()))
            .tool(EditFileTool::new(ctx.clone()))
            .tool(WriteFileTool::new(ctx.clone()))
            .tool(BashTool::new(ctx.clone()))
            .tool(GlobTool::new(ctx.clone()))
            .tool(GrepTool::new(ctx.clone()))
            .tool(SemanticSearchTool::new(kiln.clone()))
            .tool(ReadNoteTool::new(kiln.clone()))
            .tool(ListNotesTool::new(kiln.clone()))
            .build(),
        (false, Some(kiln), true) => builder
            .tool(ReadFileTool::new(ctx.clone()))
            .tool(EditFileTool::new(ctx.clone()))
            .tool(WriteFileTool::new(ctx.clone()))
            .tool(BashTool::new(ctx.clone()))
            .tool(GlobTool::new(ctx.clone()))
            .tool(GrepTool::new(ctx.clone()))
            .tool(ListBackgroundTasksTool::new(ctx.clone()))
            .tool(GetTaskResultTool::new(ctx.clone()))
            .tool(CancelTaskTool::new(ctx.clone()))
            .tool(SpawnSubagentTool::new(ctx.clone()))
            .tool(SemanticSearchTool::new(kiln.clone()))
            .tool(ReadNoteTool::new(kiln.clone()))
            .tool(ListNotesTool::new(kiln.clone()))
            .build(),
    }
}

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

    /// Additional provider-specific parameters (e.g., parallel_tool_calls for OpenAI)
    ///
    /// Note: `parallel_tool_calls` tells OpenAI to return multiple tool calls in a single
    /// response. However, Rig currently executes these sequentially. True parallel execution
    /// would require changes to Rig's streaming implementation.
    pub additional_params: Option<serde_json::Value>,
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
            additional_params: None,
        })
    }

    /// Create agent config with explicit values (useful for testing)
    pub fn new(model: impl Into<String>, system_prompt: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system_prompt: system_prompt.into(),
            temperature: None,
            max_tokens: None,
            additional_params: None,
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

    /// Set additional provider-specific parameters
    pub fn with_additional_params(mut self, params: serde_json::Value) -> Self {
        self.additional_params = Some(params);
        self
    }

    /// Create a copy with a different model name
    pub fn with_model(&self, model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system_prompt: self.system_prompt.clone(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            additional_params: self.additional_params.clone(),
        }
    }
}

/// Components needed to build and rebuild Rig agents.
///
/// Holds tool contexts and configuration separately from the agent itself,
/// enabling runtime model switching by rebuilding the agent with the same
/// tools but a different model.
#[derive(Clone)]
pub struct AgentComponents {
    /// Agent configuration (model, system prompt, etc.)
    pub config: AgentConfig,
    /// LLM client for API calls
    pub client: RigClient,
    /// Workspace context for tool execution
    pub workspace_ctx: WorkspaceContext,
    /// Optional kiln context for knowledge base tools
    pub kiln_ctx: Option<KilnContext>,
    /// Model size classification for tool selection
    pub model_size: ModelSize,
    /// Current mode (plan/normal/auto) for tool selection
    pub mode_id: String,
    /// Ollama endpoint for custom streaming (enables model switching)
    pub ollama_endpoint: Option<String>,
    /// Thinking budget for reasoning models
    pub thinking_budget: Option<i64>,
}

impl AgentComponents {
    /// Create new agent components with required fields.
    pub fn new(config: AgentConfig, client: RigClient, workspace_ctx: WorkspaceContext) -> Self {
        Self {
            config,
            client,
            workspace_ctx,
            kiln_ctx: None,
            model_size: ModelSize::Medium,
            mode_id: "normal".to_string(),
            ollama_endpoint: None,
            thinking_budget: None,
        }
    }

    /// Set the kiln context for knowledge base tools.
    pub fn with_kiln(mut self, kiln_ctx: KilnContext) -> Self {
        self.kiln_ctx = Some(kiln_ctx);
        self
    }

    /// Set the model size for tool selection.
    pub fn with_model_size(mut self, size: ModelSize) -> Self {
        self.model_size = size;
        self
    }

    /// Set the current mode for tool selection.
    pub fn with_mode(mut self, mode_id: impl Into<String>) -> Self {
        self.mode_id = mode_id.into();
        self
    }

    /// Set the Ollama endpoint for custom streaming.
    pub fn with_ollama_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.ollama_endpoint = Some(endpoint.into());
        self
    }

    /// Set the thinking budget for reasoning models.
    pub fn with_thinking_budget(mut self, budget: i64) -> Self {
        self.thinking_budget = Some(budget);
        self
    }

    /// Get a config with the specified model name
    pub fn config_for_model(&self, model: &str) -> AgentConfig {
        self.config.with_model(model)
    }
}

/// Build result containing the agent handle and workspace context for mode sync.
pub struct BuiltAgent<M>
where
    M: CompletionModel + 'static,
{
    /// The built Rig agent.
    pub agent: Agent<M>,
    /// Workspace context for tool execution.
    pub workspace_ctx: WorkspaceContext,
    /// Optional kiln context for knowledge base tools.
    pub kiln_ctx: Option<KilnContext>,
}

/// Build an agent from components with a specific model.
///
/// This is the core function for the rebuild pattern - given components and a model name,
/// it constructs a new agent with the appropriate tools based on model size.
pub fn build_agent_from_components_generic<C>(
    components: &AgentComponents,
    model: &str,
    client: &C,
) -> AgentBuildResult<BuiltAgent<C::CompletionModel>>
where
    C: CompletionClient,
    C::CompletionModel: CompletionModel<Client = C>,
{
    let config = components.config_for_model(model);
    let ctx = components.workspace_ctx.clone();
    let kiln_ctx = components.kiln_ctx.clone();

    let builder = configure_builder(client, &config);
    let agent = attach_tools(builder, &ctx, kiln_ctx.as_ref(), &components.mode_id);

    Ok(BuiltAgent {
        agent,
        workspace_ctx: ctx,
        kiln_ctx,
    })
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
    Ok(configure_builder(client, config).build())
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
) -> AgentBuildResult<(Agent<C::CompletionModel>, WorkspaceContext)>
where
    C: CompletionClient,
    C::CompletionModel: CompletionModel<Client = C>,
{
    let ctx = WorkspaceContext::new(workspace_root.as_ref());
    let builder = configure_builder(client, config);
    let agent = attach_tools(builder, &ctx, None, "normal");
    Ok((agent, ctx))
}

/// Build a Rig agent with workspace tools plus optional kiln tools.
///
/// Returns (agent, workspace_context) - caller should use context to sync mode state.
/// The `model_size` parameter is accepted for backward compatibility but ignored.
#[allow(unused_variables)]
pub fn build_agent_with_kiln_tools<C>(
    config: &AgentConfig,
    client: &C,
    workspace_root: impl AsRef<Path>,
    model_size: crucible_core::prompts::ModelSize,
    kiln_ctx: Option<KilnContext>,
) -> AgentBuildResult<(Agent<C::CompletionModel>, WorkspaceContext)>
where
    C: CompletionClient,
    C::CompletionModel: CompletionModel<Client = C>,
{
    let ctx = WorkspaceContext::new(workspace_root.as_ref());
    let builder = configure_builder(client, config);
    let agent = attach_tools(builder, &ctx, kiln_ctx.as_ref(), "normal");
    Ok((agent, ctx))
}

/// Build a Rig agent with workspace tools.
/// Backward-compatible alias that ignores model_size parameter.
///
/// Tool availability is now controlled only by mode, not model size.
#[allow(unused_variables)]
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
    let builder = configure_builder(client, config);
    Ok(attach_tools(builder, &ctx, None, "normal"))
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
