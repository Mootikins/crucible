//! Provider factory for creating Rig clients from Crucible configuration.
//!
//! This module maps `LlmProviderConfig` from crucible-config to Rig provider clients.

use crate::agent::AgentComponents;
use crate::github_copilot::CopilotClient;
use crate::handle::RigAgentHandle;
use crate::kiln_tools::KilnContext;
use crate::mcp_proxy_tool::McpProxyTool;
use crate::workspace_tools::WorkspaceContext;
use crate::{build_agent_with_kiln_tools, build_agent_with_model_size, AgentConfig};
use crucible_config::llm::LlmProviderConfig;
use crucible_config::BackendType;
use crucible_core::prompts::ModelSize;
use crucible_core::traits::chat::AgentHandle;
use crucible_tools::mcp_gateway::McpGatewayManager;
use rig::client::Nothing;
use rig::providers::{anthropic, ollama, openai, openrouter};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Errors from Rig provider operations
#[derive(Debug, Error)]
pub enum RigError {
    /// Missing required API key
    #[error("Missing API key for provider {provider}: set {env_var} environment variable")]
    MissingApiKey {
        /// Provider name
        provider: String,
        /// Expected environment variable
        env_var: String,
    },

    /// Provider not supported
    #[error("Provider type not supported: {0:?}")]
    UnsupportedProvider(BackendType),

    /// Client creation failed
    #[error("Failed to create client: {0}")]
    ClientCreation(String),

    /// GitHub Copilot requires OAuth authentication
    #[error("GitHub Copilot requires OAuth authentication. Run device flow first or provide OAuth token via api_key.")]
    CopilotAuthRequired,
}

/// Result type for Rig operations
pub type RigResult<T> = Result<T, RigError>;

/// Enum wrapping different Rig client types
///
/// Since each provider has a different client type, we use an enum
/// to provide a unified interface.
#[derive(Debug, Clone)]
pub enum RigClient {
    /// Ollama client for local LLM inference
    Ollama(ollama::Client),
    /// OpenAI client (new responses API)
    OpenAI(openai::Client),
    /// OpenAI-compatible client (standard /chat/completions API)
    /// Use this for llama.cpp, vLLM, or other OpenAI-compatible servers
    OpenAICompat(openai::CompletionsClient),
    /// Anthropic client
    Anthropic(anthropic::Client),
    /// GitHub Copilot client (uses OAuth + Copilot API token exchange)
    GitHubCopilot(CopilotClient),
    /// OpenRouter client (meta-provider for multiple LLM APIs)
    OpenRouter(openrouter::Client),
}

impl RigClient {
    /// Get the provider name
    pub fn provider_name(&self) -> &'static str {
        match self {
            RigClient::Ollama(_) => "ollama",
            RigClient::OpenAI(_) => "openai",
            RigClient::OpenAICompat(_) => "openai-compat",
            RigClient::Anthropic(_) => "anthropic",
            RigClient::GitHubCopilot(_) => "github-copilot",
            RigClient::OpenRouter(_) => "openrouter",
        }
    }

    /// Get the inner GitHub Copilot client, if this is a GitHub Copilot client.
    pub fn as_github_copilot(&self) -> Option<&CopilotClient> {
        match self {
            RigClient::GitHubCopilot(c) => Some(c),
            _ => None,
        }
    }
}

/// Configuration knobs for [`RigClient::build_agent_handle`] and [`RigClient::build_agent_handle_with_kiln`].
pub struct HandleBuildOpts {
    /// Model name for display/streaming.
    /// Read by: both `build_agent_handle` (daemon) and `build_agent_handle_with_kiln` (CLI).
    pub model: String,
    /// Size classification for tool selection.
    /// Read by: both `build_agent_handle` (daemon) and `build_agent_handle_with_kiln` (CLI).
    pub model_size: ModelSize,
    /// Thinking budget: -1 = unlimited, 0 = disabled, >0 = max tokens.
    /// Read by: `build_agent_handle` (daemon) only — CLI manages thinking budget elsewhere.
    pub thinking_budget: Option<i64>,
    /// Ollama endpoint for custom streaming / model discovery.
    /// Read by: both methods, but **only applied for Ollama clients**.
    pub ollama_endpoint: Option<String>,
    /// MCP gateway for upstream tool injection.
    /// Read by: `build_agent_handle` (daemon) only — Ollama clients only.
    pub mcp_gateway: Option<Arc<RwLock<McpGatewayManager>>>,
    /// Initial mode (plan/normal/auto).
    /// Read by: `build_agent_handle_with_kiln` (CLI) only.
    pub initial_mode: Option<String>,
    /// OpenAI-compatible endpoint for `reasoning_content` extraction.
    /// Read by: `build_agent_handle_with_kiln` (CLI) only.
    pub reasoning_endpoint: Option<String>,
}

impl RigClient {
    async fn copilot_compat_client(
        copilot_client: &CopilotClient,
    ) -> Result<openai::CompletionsClient, String> {
        let api_token = copilot_client
            .api_token()
            .await
            .map_err(|e| format!("Copilot auth: {}", e))?;
        let api_base = copilot_client
            .api_base()
            .await
            .map_err(|e| format!("Copilot base: {}", e))?;
        create_openai_compat_client(&api_token, &api_base).map_err(|e| e.to_string())
    }

    /// Daemon path: build handle from a pre-configured `WorkspaceContext`.
    pub async fn build_agent_handle(
        &self,
        rig_agent_config: &AgentConfig,
        ws_ctx: &WorkspaceContext,
        mcp_tools: Vec<McpProxyTool>,
        opts: HandleBuildOpts,
    ) -> Result<Box<dyn AgentHandle + Send + Sync>, String> {
        match self {
            RigClient::Ollama(ref ollama_client) => {
                let agent = build_agent_with_model_size(
                    rig_agent_config,
                    ollama_client,
                    ws_ctx,
                    opts.model_size,
                    mcp_tools,
                )
                .map_err(|e| e.to_string())?;
                let mut components =
                    AgentComponents::new(rig_agent_config.clone(), self.clone(), ws_ctx.clone())
                        .with_model_size(opts.model_size);
                if let Some(budget) = opts.thinking_budget {
                    components = components.with_thinking_budget(budget);
                }
                if let Some(ref endpoint) = opts.ollama_endpoint {
                    components = components.with_ollama_endpoint(endpoint.clone());
                }
                if let Some(gw) = opts.mcp_gateway {
                    components = components.with_mcp_gateway(gw);
                }
                let handle = RigAgentHandle::new(agent)
                    .with_ollama_components(components)
                    .with_model(opts.model.clone())
                    .with_thinking_budget(opts.thinking_budget);
                Ok(Box::new(handle))
            }
            RigClient::GitHubCopilot(copilot_client) => {
                let compat_client = Self::copilot_compat_client(copilot_client).await?;

                let agent = build_agent_with_model_size(
                    rig_agent_config,
                    &compat_client,
                    ws_ctx,
                    opts.model_size,
                    mcp_tools,
                )
                .map_err(|e| e.to_string())?;
                Ok(Box::new(
                    RigAgentHandle::new(agent)
                        .with_workspace_context(ws_ctx.clone())
                        .with_model(opts.model.clone())
                        .with_thinking_budget(opts.thinking_budget),
                ))
            }
            _ => self.build_standard_handle(rig_agent_config, ws_ctx, mcp_tools, &opts),
        }
    }

    /// CLI path: build handle from workspace path + optional kiln context.
    pub async fn build_agent_handle_with_kiln(
        &self,
        rig_agent_config: &AgentConfig,
        workspace_root: &Path,
        kiln_ctx: Option<KilnContext>,
        mcp_tools: Vec<McpProxyTool>,
        opts: HandleBuildOpts,
    ) -> Result<Box<dyn AgentHandle + Send + Sync>, String> {
        let initial_mode = opts.initial_mode.as_deref().unwrap_or("normal");

        match self {
            RigClient::Ollama(ollama_client) => {
                let (agent, ws_ctx) = build_agent_with_kiln_tools(
                    rig_agent_config,
                    ollama_client,
                    workspace_root,
                    opts.model_size,
                    kiln_ctx.clone(),
                    mcp_tools.clone(),
                )
                .map_err(|e| e.to_string())?;

                let mut components =
                    AgentComponents::new(rig_agent_config.clone(), self.clone(), ws_ctx.clone())
                        .with_model_size(opts.model_size);

                if let Some(kc) = kiln_ctx {
                    components = components.with_kiln(kc);
                }
                if let Some(ref endpoint) = opts.ollama_endpoint {
                    components = components.with_ollama_endpoint(endpoint.clone());
                }

                let mut handle = RigAgentHandle::new(agent)
                    .with_ollama_components(components)
                    .with_initial_mode(initial_mode);

                if let Some(endpoint) = opts.reasoning_endpoint {
                    handle = handle.with_reasoning_endpoint(endpoint, opts.model.clone());
                }
                Ok(Box::new(handle))
            }
            RigClient::GitHubCopilot(copilot_client) => {
                let compat_client = Self::copilot_compat_client(copilot_client).await?;

                let (agent, ws_ctx) = build_agent_with_kiln_tools(
                    rig_agent_config,
                    &compat_client,
                    workspace_root,
                    opts.model_size,
                    kiln_ctx,
                    mcp_tools,
                )
                .map_err(|e| e.to_string())?;
                let mut handle = RigAgentHandle::new(agent)
                    .with_workspace_context(ws_ctx)
                    .with_initial_mode(initial_mode);
                if let Some(endpoint) = opts.reasoning_endpoint {
                    handle = handle.with_reasoning_endpoint(endpoint, opts.model.clone());
                }
                Ok(Box::new(handle))
            }
            _ => self.build_standard_kiln_handle(
                rig_agent_config,
                workspace_root,
                kiln_ctx,
                mcp_tools,
                &opts,
                initial_mode,
            ),
        }
    }

    fn build_standard_handle(
        &self,
        rig_agent_config: &AgentConfig,
        ws_ctx: &WorkspaceContext,
        mcp_tools: Vec<McpProxyTool>,
        opts: &HandleBuildOpts,
    ) -> Result<Box<dyn AgentHandle + Send + Sync>, String> {
        match self {
            RigClient::OpenAI(client) => {
                make_daemon_handle(client, rig_agent_config, ws_ctx, mcp_tools, opts)
            }
            RigClient::OpenAICompat(client) => {
                make_daemon_handle(client, rig_agent_config, ws_ctx, mcp_tools, opts)
            }
            RigClient::Anthropic(client) => {
                make_daemon_handle(client, rig_agent_config, ws_ctx, mcp_tools, opts)
            }
            RigClient::OpenRouter(client) => {
                make_daemon_handle(client, rig_agent_config, ws_ctx, mcp_tools, opts)
            }
            RigClient::Ollama(_) | RigClient::GitHubCopilot(_) => {
                Err("Ollama and Copilot should be handled before build_standard_handle".into())
            }
        }
    }

    fn build_standard_kiln_handle(
        &self,
        rig_agent_config: &AgentConfig,
        workspace_root: &Path,
        kiln_ctx: Option<KilnContext>,
        mcp_tools: Vec<McpProxyTool>,
        opts: &HandleBuildOpts,
        initial_mode: &str,
    ) -> Result<Box<dyn AgentHandle + Send + Sync>, String> {
        match self {
            RigClient::OpenAI(client) => make_kiln_handle(
                client,
                rig_agent_config,
                workspace_root,
                kiln_ctx,
                mcp_tools,
                opts,
                initial_mode,
            ),
            RigClient::OpenAICompat(client) => make_kiln_handle(
                client,
                rig_agent_config,
                workspace_root,
                kiln_ctx,
                mcp_tools,
                opts,
                initial_mode,
            ),
            RigClient::Anthropic(client) => make_kiln_handle(
                client,
                rig_agent_config,
                workspace_root,
                kiln_ctx,
                mcp_tools,
                opts,
                initial_mode,
            ),
            RigClient::OpenRouter(client) => make_kiln_handle(
                client,
                rig_agent_config,
                workspace_root,
                kiln_ctx,
                mcp_tools,
                opts,
                initial_mode,
            ),
            RigClient::Ollama(_) | RigClient::GitHubCopilot(_) => {
                Err("Ollama and Copilot should be handled before build_standard_kiln_handle".into())
            }
        }
    }
}

fn make_daemon_handle<C>(
    client: &C,
    rig_agent_config: &AgentConfig,
    ws_ctx: &WorkspaceContext,
    mcp_tools: Vec<McpProxyTool>,
    opts: &HandleBuildOpts,
) -> Result<Box<dyn AgentHandle + Send + Sync>, String>
where
    C: rig::client::CompletionClient,
    C::CompletionModel: rig::completion::CompletionModel<Client = C> + 'static,
{
    let agent =
        build_agent_with_model_size(rig_agent_config, client, ws_ctx, opts.model_size, mcp_tools)
            .map_err(|e| e.to_string())?;
    let handle = RigAgentHandle::new(agent)
        .with_workspace_context(ws_ctx.clone())
        .with_model(opts.model.clone())
        .with_thinking_budget(opts.thinking_budget);
    Ok(Box::new(handle))
}

fn make_kiln_handle<C>(
    client: &C,
    rig_agent_config: &AgentConfig,
    workspace_root: &Path,
    kiln_ctx: Option<KilnContext>,
    mcp_tools: Vec<McpProxyTool>,
    opts: &HandleBuildOpts,
    initial_mode: &str,
) -> Result<Box<dyn AgentHandle + Send + Sync>, String>
where
    C: rig::client::CompletionClient,
    C::CompletionModel: rig::completion::CompletionModel<Client = C> + 'static,
{
    let (agent, ws_ctx) = build_agent_with_kiln_tools(
        rig_agent_config,
        client,
        workspace_root,
        opts.model_size,
        kiln_ctx,
        mcp_tools,
    )
    .map_err(|e| e.to_string())?;
    let mut handle = RigAgentHandle::new(agent)
        .with_workspace_context(ws_ctx)
        .with_initial_mode(initial_mode)
        .with_model(opts.model.clone());
    if let Some(ref endpoint) = opts.reasoning_endpoint {
        handle = handle.with_reasoning_endpoint(endpoint.clone(), opts.model.clone());
    }
    Ok(Box::new(handle))
}

/// Create a Rig client from Crucible LLM provider configuration.
///
/// # Arguments
///
/// * `config` - The LLM provider configuration from crucible-config
///
/// # Returns
///
/// A `RigClient` enum wrapping the appropriate provider client.
///
/// # Errors
///
/// Returns an error if:
/// - Required API key is missing for OpenAI/Anthropic
/// - Provider type is not supported
///
/// # Example
///
/// ```rust,ignore
/// use crucible_config::components::llm::LlmProviderConfig;
/// use crucible_config::components::backend::BackendType;
/// use crucible_rig::providers::create_client;
///
/// let config = LlmProviderConfig {
///     provider_type: BackendType::Ollama,
///     endpoint: Some("http://localhost:11434".into()),
///     default_model: Some("llama3.2".into()),
///     ..Default::default()
/// };
///
/// let client = create_client(&config)?;
/// ```
pub fn create_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    match config.provider_type {
        BackendType::Ollama => create_ollama_client(config),
        // OpenAI and ZAI both use the OpenAI-compatible client
        BackendType::OpenAI | BackendType::ZAI => create_openai_client(config),
        BackendType::Anthropic => create_anthropic_client(config),
        BackendType::GitHubCopilot => create_github_copilot_client(config),
        BackendType::OpenRouter => create_openrouter_client(config),
        BackendType::Cohere
        | BackendType::VertexAI
        | BackendType::FastEmbed
        | BackendType::Burn
        | BackendType::Custom
        | BackendType::Mock => Err(RigError::UnsupportedProvider(config.provider_type)),
    }
}

/// Create an Ollama client
fn create_ollama_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let endpoint = config.endpoint();

    tracing::debug!(endpoint = %endpoint, "Creating Ollama client");

    // Ollama uses builder pattern with Nothing as API key
    let client = if endpoint != "http://localhost:11434" {
        // Custom endpoint
        ollama::Client::builder()
            .api_key(Nothing)
            .base_url(&endpoint)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?
    } else {
        // Default endpoint
        ollama::Client::builder()
            .api_key(Nothing)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?
    };

    Ok(RigClient::Ollama(client))
}

/// Create an OpenAI client
///
/// For custom endpoints (llama.cpp, vLLM, etc.), this returns an OpenAICompat
/// client using the standard `/chat/completions` API. For the real OpenAI API,
/// it returns the standard OpenAI client.
fn create_openai_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let endpoint = config.endpoint();
    let is_real_openai = endpoint == "https://api.openai.com/v1";

    tracing::debug!(endpoint = %endpoint, is_real_openai, "Creating OpenAI client");

    if is_real_openai {
        // Real OpenAI - requires API key, uses responses API
        let api_key = config.api_key().ok_or_else(|| RigError::MissingApiKey {
            provider: "OpenAI".into(),
            env_var: config
                .api_key
                .clone()
                .unwrap_or_else(|| "OPENAI_API_KEY".into()),
        })?;

        let client = openai::Client::builder()
            .api_key(&api_key)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?;

        Ok(RigClient::OpenAI(client))
    } else {
        // OpenAI-compatible endpoint (llama.cpp, vLLM, etc.)
        // Use CompletionsClient for standard /chat/completions API
        // API key is optional for local servers
        let api_key = config.api_key().unwrap_or_else(|| "not-needed".to_string());

        let client = openai::CompletionsClient::builder()
            .api_key(&api_key)
            .base_url(&endpoint)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?;

        Ok(RigClient::OpenAICompat(client))
    }
}

/// Create an Anthropic client
///
/// Supports custom endpoints (e.g., Anthropic-compatible APIs) via the `endpoint` config field.
/// If no endpoint is specified, uses the default Anthropic API.
fn create_anthropic_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let api_key = config.api_key().ok_or_else(|| RigError::MissingApiKey {
        provider: "Anthropic".into(),
        env_var: config
            .api_key
            .clone()
            .unwrap_or_else(|| "ANTHROPIC_API_KEY".into()),
    })?;

    let endpoint = config.endpoint();
    let is_default_endpoint = endpoint == "https://api.anthropic.com/v1";

    tracing::debug!(endpoint = %endpoint, is_default_endpoint, "Creating Anthropic client");

    let client = if is_default_endpoint {
        // Default Anthropic API
        anthropic::Client::builder()
            .api_key(api_key)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?
    } else {
        // Custom endpoint (Anthropic-compatible API)
        anthropic::Client::builder()
            .api_key(api_key)
            .base_url(&endpoint)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?
    };

    Ok(RigClient::Anthropic(client))
}

/// Create a GitHub Copilot client
///
/// GitHub Copilot requires an OAuth token (obtained via device flow authentication).
/// The token should be stored in the config's `api_key` field.
///
/// To obtain an OAuth token, use [`crate::github_copilot::CopilotAuth`]:
///
/// ```rust,ignore
/// use crucible_rig::github_copilot::CopilotAuth;
///
/// let auth = CopilotAuth::new();
/// let oauth_token = auth.complete_device_flow(|code, uri| {
///     println!("Visit {} and enter code: {}", uri, code);
/// }).await?;
///
/// // Save oauth_token.access_token to config
/// ```
fn create_github_copilot_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let oauth_token = config.api_key().ok_or(RigError::CopilotAuthRequired)?;

    tracing::debug!("Creating GitHub Copilot client");

    let client = CopilotClient::new(oauth_token);

    Ok(RigClient::GitHubCopilot(client))
}

fn create_openrouter_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let api_key = config.api_key().ok_or_else(|| RigError::MissingApiKey {
        provider: "OpenRouter".into(),
        env_var: "OPENROUTER_API_KEY".into(),
    })?;

    tracing::debug!("Creating OpenRouter client");

    let client =
        openrouter::Client::new(&api_key).map_err(|e| RigError::ClientCreation(e.to_string()))?;

    Ok(RigClient::OpenRouter(client))
}

/// Create an OpenAI-compatible client with explicit credentials.
///
/// This is useful for creating clients from dynamically-obtained tokens,
/// such as GitHub Copilot's API tokens.
pub fn create_openai_compat_client(
    api_key: &str,
    base_url: &str,
) -> RigResult<openai::CompletionsClient> {
    openai::CompletionsClient::builder()
        .api_key(api_key)
        .base_url(base_url)
        .build()
        .map_err(|e| RigError::ClientCreation(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ollama_config() -> LlmProviderConfig {
        LlmProviderConfig {
                    provider_type: BackendType::Ollama,
                    endpoint: None,
                    default_model: Some("llama3.2".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                }
    }

    fn ollama_config_custom_endpoint() -> LlmProviderConfig {
        LlmProviderConfig {
                    provider_type: BackendType::Ollama,
                    endpoint: Some("http://192.168.1.100:11434".into()),
                    default_model: Some("llama3.2".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                }
    }

    fn openai_config_with_key() -> LlmProviderConfig {
        LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: Some("gpt-4o".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: Some("TEST_OPENAI_KEY".into()),
                    available_models: None,
                    trust_level: None,
                }
    }

    fn anthropic_config_with_key() -> LlmProviderConfig {
        LlmProviderConfig {
                    provider_type: BackendType::Anthropic,
                    endpoint: None,
                    default_model: Some("claude-3-5-sonnet-20241022".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: Some("TEST_ANTHROPIC_KEY".into()),
                    available_models: None,
                    trust_level: None,
                }
    }

    fn copilot_config_with_token() -> LlmProviderConfig {
        LlmProviderConfig {
                    provider_type: BackendType::GitHubCopilot,
                    endpoint: None,
                    default_model: Some("gpt-4o".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: Some("gho_test_oauth_token".into()),
                    available_models: None,
                    trust_level: None,
                }
    }

    fn copilot_config_no_token() -> LlmProviderConfig {
        LlmProviderConfig {
                    provider_type: BackendType::GitHubCopilot,
                    endpoint: None,
                    default_model: Some("gpt-4o".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                }
    }

    #[test]
    fn test_create_ollama_client_default_endpoint() {
        let config = ollama_config();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "ollama");
    }

    #[test]
    fn test_create_ollama_client_custom_endpoint() {
        let config = ollama_config_custom_endpoint();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "ollama");
    }

    #[test]
    fn test_create_openai_client_with_api_key() {
        // Set test API key
        std::env::set_var("TEST_OPENAI_KEY", "test-key-12345");

        let config = openai_config_with_key();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "openai");

        std::env::remove_var("TEST_OPENAI_KEY");
    }

    #[test]
    fn test_create_openai_client_missing_api_key() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: Some("gpt-4o".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                };

        let client = create_client(&config);

        assert!(client.is_err());
        let err = client.unwrap_err();
        assert!(matches!(err, RigError::MissingApiKey { .. }));
    }

    #[test]
    fn test_create_anthropic_client_with_api_key() {
        // Set test API key
        std::env::set_var("TEST_ANTHROPIC_KEY", "test-key-67890");

        let config = anthropic_config_with_key();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "anthropic");

        std::env::remove_var("TEST_ANTHROPIC_KEY");
    }

    #[test]
    fn test_create_anthropic_client_missing_api_key() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::Anthropic,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                };

        let client = create_client(&config);

        assert!(client.is_err());
        let err = client.unwrap_err();
        assert!(matches!(err, RigError::MissingApiKey { .. }));
    }

    #[test]
    fn test_rig_client_provider_names() {
        // Ollama (no API key needed)
        let ollama = create_client(&ollama_config()).unwrap();
        assert_eq!(ollama.provider_name(), "ollama");

        // OpenAI
        std::env::set_var("TEST_OPENAI_KEY", "test");
        let openai = create_client(&openai_config_with_key()).unwrap();
        assert_eq!(openai.provider_name(), "openai");
        std::env::remove_var("TEST_OPENAI_KEY");

        // Anthropic
        std::env::set_var("TEST_ANTHROPIC_KEY", "test");
        let anthropic = create_client(&anthropic_config_with_key()).unwrap();
        assert_eq!(anthropic.provider_name(), "anthropic");
        std::env::remove_var("TEST_ANTHROPIC_KEY");
    }

    #[test]
    fn test_create_openai_compat_client_with_custom_endpoint() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: Some("https://llama.example.com/v1".into()),
                    default_model: Some("qwen3-8b".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                };

        let client = create_client(&config);
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "openai-compat");
    }

    #[test]
    fn test_create_openai_compat_no_api_key_required() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: Some("http://localhost:8080/v1".into()),
                    default_model: Some("local-model".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: Some("NONEXISTENT_API_KEY".into()),
                    available_models: None,
                    trust_level: None,
                };

        let client = create_client(&config);
        assert!(client.is_ok());
        assert_eq!(client.unwrap().provider_name(), "openai-compat");
    }

    #[test]
    fn test_real_openai_requires_api_key() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: Some("gpt-4o".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                };

        let client = create_client(&config);
        assert!(client.is_err());
        assert!(matches!(
            client.unwrap_err(),
            RigError::MissingApiKey { .. }
        ));
    }

    #[test]
    fn test_create_github_copilot_client_with_token() {
        let config = copilot_config_with_token();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "github-copilot");
        assert!(client.as_github_copilot().is_some());
    }

    #[test]
    fn test_create_github_copilot_client_missing_token() {
        // GitHub Copilot requires an OAuth token
        let config = copilot_config_no_token();
        let client = create_client(&config);

        assert!(client.is_err());
        let err = client.unwrap_err();
        assert!(matches!(err, RigError::CopilotAuthRequired));
    }

    #[test]
    fn test_github_copilot_oauth_token_preserved() {
        let config = copilot_config_with_token();
        let client = create_client(&config).unwrap();
        let copilot = client.as_github_copilot().unwrap();

        // OAuth token should be preserved
        assert_eq!(copilot.oauth_token(), "gho_test_oauth_token");
    }

    fn openrouter_config_with_key() -> LlmProviderConfig {
        LlmProviderConfig {
                    provider_type: BackendType::OpenRouter,
                    endpoint: None,
                    default_model: Some("openai/gpt-4o".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: Some("sk-or-test-key".into()),
                    available_models: None,
                    trust_level: None,
                }
    }

    fn openrouter_config_no_key() -> LlmProviderConfig {
        LlmProviderConfig {
                    provider_type: BackendType::OpenRouter,
                    endpoint: None,
                    default_model: Some("openai/gpt-4o".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                }
    }

    #[test]
    fn test_create_openrouter_client_with_api_key() {
        let config = openrouter_config_with_key();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "openrouter");
    }

    #[test]
    fn test_create_openrouter_client_missing_api_key() {
        let config = openrouter_config_no_key();
        let client = create_client(&config);

        assert!(client.is_err());
        let err = client.unwrap_err();
        assert!(matches!(err, RigError::MissingApiKey { .. }));
    }

    #[test]
    fn test_create_anthropic_client_custom_endpoint() {
        std::env::set_var("TEST_ANTHROPIC_KEY", "test-key-custom");

        let config = LlmProviderConfig {
                    provider_type: BackendType::Anthropic,
                    endpoint: Some("https://api.z.ai/api/anthropic".into()),
                    default_model: Some("glm-4-flash".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: Some("TEST_ANTHROPIC_KEY".into()),
                    available_models: None,
                    trust_level: None,
                };

        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "anthropic");

        std::env::remove_var("TEST_ANTHROPIC_KEY");
    }

    #[test]
    fn test_create_zai_client() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::ZAI,
                    endpoint: Some("https://api.z.ai/api/coding/paas/v4".into()),
                    default_model: Some("glm-4-flash".into()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: Some("test-key".into()),
                    available_models: None,
                    trust_level: None,
                };

        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        // ZAI routes through OpenAI-compat client
        assert!(client.provider_name() == "openai-compat" || client.provider_name() == "openai");
    }
}
