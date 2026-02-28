//! Agent factory for daemon.
//!
//! Creates `AgentHandle` instances from `SessionAgent` configuration.
//! This is a simplified version of the CLI's agent factory since
//! `SessionAgent` contains fully-resolved configuration.

use crate::acp_handle::AcpAgentHandle;
use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};
use crate::protocol::SessionEventMessage;
use crate::provider::adapter_mapping::{build_genai_client, build_model_iden};
use crate::provider::genai_handle::GenaiAgentHandle;
use crucible_acp::client::PermissionRequestHandler;
use crucible_config::credentials::resolve_copilot_oauth_token;
use crucible_config::{BackendType, DataClassification, LlmProviderConfig};
use crucible_core::background::BackgroundSpawner;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::session::SessionAgent;
use crucible_core::traits::auth::AuthHeaders;
use crucible_core::traits::chat::AgentHandle;
use crucible_core::traits::llm::LlmToolDefinition;
use crucible_core::traits::mcp::McpToolInfo;
use crucible_core::traits::KnowledgeRepository;
use crucible_lua::auth_plugin::{fire_provider_auth_hooks, get_provider_auth_hooks};
use crucible_tools::mcp_server::CrucibleMcpServer;
use crucible_tools::DelegationContext;
use mlua::Lua;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Build a `DelegationContext` for the internal agent's MCP server.
///
/// Follows the exact pattern from `AcpAgentHandle::new()` — requires both
/// `parent_session_id` and `background_spawner` to be present.  When either
/// is `None` the result is `None` and the MCP server falls back to the
/// non-delegation constructor.
fn build_internal_delegation_context(
    agent_config: &SessionAgent,
    parent_session_id: Option<&str>,
    background_spawner: Option<Arc<dyn BackgroundSpawner>>,
    workspace: &Path,
    kiln_path: Option<&Path>,
) -> Option<DelegationContext> {
    parent_session_id.and_then(|session_id| {
        background_spawner.map(|spawner| {
            let delegation_config = agent_config.delegation_config.as_ref();
            DelegationContext {
                background_spawner: spawner,
                session_id: session_id.to_string(),
                targets: delegation_config
                    .and_then(|c| c.allowed_targets.clone())
                    .unwrap_or_default(),
                enabled: delegation_config.map(|c| c.enabled).unwrap_or(false),
                depth: 0,
                data_classification: kiln_path
                    .and_then(|kiln| {
                        crate::trust_resolution::resolve_kiln_classification(workspace, kiln)
                    })
                    .unwrap_or(DataClassification::Public),
            }
        })
    })
}

#[allow(clippy::too_many_arguments)]
async fn create_internal_mcp_tool_defs(
    workspace: &Path,
    kiln_path: Option<&Path>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
    server_names: &[String],
    knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    delegation_context: Option<DelegationContext>,
    mode: &str,
    gateway_all_tools_override: Option<&[McpToolInfo]>,
) -> Vec<LlmToolDefinition> {
    let mut tool_defs = Vec::new();

    if let Some(kiln_path) = kiln_path {
        let knowledge_repo: Arc<dyn KnowledgeRepository> =
            knowledge_repo.unwrap_or_else(|| Arc::new(EmptyKnowledgeRepository));
        let embedding_provider: Arc<dyn EmbeddingProvider> =
            embedding_provider.unwrap_or_else(|| Arc::new(EmptyEmbeddingProvider));
        let server = CrucibleMcpServer::new_with_delegation(
            kiln_path.display().to_string(),
            knowledge_repo,
            embedding_provider,
            delegation_context,
        );
        for tool in server.list_tools() {
            let tool_name = tool.name.to_string();
            if mode == "plan" && !is_plan_mode_tool(&tool_name) {
                continue;
            }
            tool_defs.push(LlmToolDefinition::new(
                tool_name,
                tool.description.map(|d| d.to_string()).unwrap_or_default(),
                serde_json::Value::Object((*tool.input_schema).clone()),
            ));
        }

        info!(
            count = tool_defs.len(),
            kiln = %kiln_path.display(),
            mode,
            "Resolved in-process Crucible MCP tool definitions"
        );
    } else {
        debug!(
            workspace = %workspace.display(),
            "Skipping in-process Crucible MCP adapter tools because kiln path is unavailable"
        );
    }

    let user_mcp_tools: Vec<LlmToolDefinition> = if let Some(ref gw) = mcp_gateway {
        let gw_read = gw.read().await;
        debug!(
            tool_count = gw_read.tool_count(),
            mcp_servers = ?server_names,
            "MCP gateway available for agent"
        );

        if !server_names.is_empty() {
            let all_tools_owned;
            let all_tools: &[McpToolInfo] = if let Some(override_tools) = gateway_all_tools_override
            {
                override_tools
            } else {
                all_tools_owned = gw_read.all_tools();
                &all_tools_owned
            };
            drop(gw_read);
            let tools = all_tools
                .iter()
                .filter(|tool| server_names.contains(&tool.upstream))
                .map(|tool| {
                    LlmToolDefinition::new(
                        tool.prefixed_name.clone(),
                        tool.description.clone().unwrap_or_default(),
                        tool.input_schema.clone(),
                    )
                })
                .collect::<Vec<_>>();
            info!(count = tools.len(), servers = ?server_names, "Resolved MCP proxy tools");
            tools
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    tool_defs.extend(user_mcp_tools);
    tool_defs
}

fn is_plan_mode_tool(name: &str) -> bool {
    crucible_tools::tool_modes::PLAN_TOOL_NAMES.contains(&name)
}

#[cfg(test)]
async fn create_internal_mcp_tool_names_for_tests(
    workspace: &Path,
    kiln_path: Option<&Path>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
    server_names: &[String],
    knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    delegation_context: Option<DelegationContext>,
    mode: &str,
    gateway_all_tools_override: Option<&[McpToolInfo]>,
) -> Vec<String> {
    let tools = create_internal_mcp_tool_defs(
        workspace,
        kiln_path,
        mcp_gateway,
        server_names,
        knowledge_repo,
        embedding_provider,
        delegation_context,
        mode,
        gateway_all_tools_override,
    )
    .await;
    tools.into_iter().map(|t| t.function.name).collect()
}

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
/// * `background_spawner` - Optional spawner for background tasks (subagents, long bash)
/// * `event_tx` - Broadcast sender for session events (used for InteractionContext)
/// * `knowledge_repo` - Optional knowledge repository for search tools (used by CrucibleMcpServer)
/// * `embedding_provider` - Optional embedding provider for semantic search (used by CrucibleMcpServer)
/// # Returns
///
/// A boxed `AgentHandle` ready for streaming messages.
#[allow(clippy::too_many_arguments)]
pub async fn create_agent_from_session_config(
    agent_config: &SessionAgent,
    lua: Option<&Lua>,
    workspace: &Path,
    kiln_path: Option<&Path>,
    parent_session_id: Option<&str>,
    background_spawner: Option<Arc<dyn BackgroundSpawner>>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
    acp_permission_handler: Option<PermissionRequestHandler>,
    acp_config: Option<&crucible_config::components::acp::AcpConfig>,
    knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
) -> Result<Box<dyn AgentHandle + Send + Sync>, AgentFactoryError> {
    // TODO: Wire event_tx for real-time session event broadcasting (streaming progress, tool call notifications)
    let _ = event_tx;
    if agent_config.agent_type == "acp" {
        let handle = AcpAgentHandle::new(
            agent_config,
            workspace,
            kiln_path,
            None,
            None,
            background_spawner,
            parent_session_id,
            agent_config.delegation_config.as_ref(),
            acp_config,
            acp_permission_handler,
        )
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

    let provider_type = agent_config.provider;

    let mut llm_config = LlmProviderConfig::builder(provider_type);
    if let Some(endpoint) = agent_config.endpoint.clone() {
        llm_config = llm_config.endpoint(endpoint);
    }
    let mut llm_config = llm_config
        .model(agent_config.model.clone())
        .with_api_key_env_var_name()
        .build();

    if let Some(lua) = lua {
        match get_provider_auth_hooks(lua) {
            Ok(hooks) if !hooks.is_empty() => {
                let provider_name = match provider_type {
                    BackendType::GitHubCopilot => "github-copilot".to_string(),
                    _ => format!("{provider_type:?}").to_lowercase(),
                };

                match fire_provider_auth_hooks(lua, &hooks, &provider_name, &agent_config.model) {
                    Ok(Some(auth_headers)) => {
                        let auth_headers: AuthHeaders = auth_headers;
                        if let Some(auth_value) = auth_headers.get("Authorization") {
                            let api_key = auth_value.strip_prefix("Bearer ").unwrap_or(auth_value);
                            llm_config.api_key = Some(api_key.to_string());
                            debug!(
                                provider = %provider_name,
                                model = %agent_config.model,
                                "Lua auth hook provided API key"
                            );
                        } else {
                            debug!(
                                provider = %provider_name,
                                model = %agent_config.model,
                                "Lua auth hook returned headers without Authorization; using config API key fallback"
                            );
                        }
                    }
                    Ok(None) => {
                        debug!(
                            provider = %provider_name,
                            model = %agent_config.model,
                            "No Lua auth hook matched provider; using config API key fallback"
                        );
                    }
                    Err(e) => {
                        warn!(
                            provider = %provider_name,
                            model = %agent_config.model,
                            "Lua auth hook error: {e}; using config API key fallback"
                        );
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                warn!(
                    provider = %agent_config.provider,
                    model = %agent_config.model,
                    "Failed to get Lua auth hooks: {e}; using config API key fallback"
                );
            }
        }
    }

    let mode = "auto";
    let delegation_context = build_internal_delegation_context(
        agent_config,
        parent_session_id,
        background_spawner.clone(),
        workspace,
        kiln_path,
    );
    let tool_defs = create_internal_mcp_tool_defs(
        workspace,
        kiln_path,
        mcp_gateway.clone(),
        &agent_config.mcp_servers,
        knowledge_repo,
        embedding_provider,
        delegation_context,
        mode,
        None,
    )
    .await;

    info!(
        provider = %agent_config.provider,
        model = %agent_config.model,
        workspace = %workspace.display(),
        "Creating agent from session config"
    );

    if agent_config.provider == BackendType::GitHubCopilot {
        if let Some(oauth_token) = resolve_copilot_oauth_token(llm_config.api_key.as_deref()) {
            llm_config.api_key = Some(oauth_token);
        }
    }

    let model_iden = build_model_iden(&provider_type, &agent_config.model).ok_or_else(|| {
        AgentFactoryError::ClientCreation(format!(
            "Unsupported provider for chat: {:?}",
            provider_type
        ))
    })?;
    let genai_client = build_genai_client(&llm_config);
    let handle = GenaiAgentHandle::new(
        genai_client,
        model_iden,
        &agent_config.system_prompt,
        tool_defs,
        agent_config.thinking_budget,
    );

    info!(
        provider = %agent_config.provider,
        model = %agent_config.model,
        "Agent created successfully"
    );

    Ok(Box::new(handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use tokio::sync::RwLock;

    async fn build_internal_tool_names_for_tests(
        workspace: &Path,
        kiln_path: Option<&Path>,
        knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
        embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
        mcp_gateway: Option<Arc<RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
        user_tools: &[McpToolInfo],
        mode: &str,
    ) -> Vec<String> {
        create_internal_mcp_tool_names_for_tests(
            workspace,
            kiln_path,
            mcp_gateway,
            &["gh".to_string()],
            knowledge_repo,
            embedding_provider,
            None,
            mode,
            Some(user_tools),
        )
        .await
    }

    fn test_agent_config() -> SessionAgent {
        SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some("ollama".to_string()),
            provider: BackendType::Ollama,
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
            precognition_enabled: false,
        }
    }

    #[test]
    fn test_unsupported_agent_type() {
        let mut config = test_agent_config();
        config.agent_type = "unknown".to_string();

        let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (event_tx, _) = broadcast::channel(16);
            create_agent_from_session_config(
                &config,
                None,
                Path::new("/tmp"),
                None,
                None,
                None,
                &event_tx,
                None,
                None,
                None,
                None,
                None,
            )
            .await
        });

        assert!(matches!(
            result,
            Err(AgentFactoryError::UnsupportedAgentType(_))
        ));
    }

    #[tokio::test]
    async fn internal_tools_include_adapter_tools() {
        let gateway = Arc::new(RwLock::new(
            crucible_tools::mcp_gateway::McpGatewayManager::new(),
        ));

        let names = build_internal_tool_names_for_tests(
            Path::new("/tmp"),
            Some(Path::new("/tmp")),
            None,
            None,
            Some(gateway),
            &[],
            "auto",
        )
        .await;

        assert!(names.iter().any(|name| name == "semantic_search"));
        assert!(names.iter().any(|name| name == "delegate_session"));
        assert!(names.iter().any(|name| name == "list_jobs"));
    }

    #[tokio::test]
    async fn adapter_tools_come_before_user_mcp_tools() {
        let gateway = Arc::new(RwLock::new(
            crucible_tools::mcp_gateway::McpGatewayManager::new(),
        ));

        let user_tools = vec![McpToolInfo {
            name: "search_repos".to_string(),
            prefixed_name: "gh_search_repos".to_string(),
            description: Some("Search repos".to_string()),
            input_schema: serde_json::json!({"type": "object"}),
            upstream: "gh".to_string(),
        }];

        let names = build_internal_tool_names_for_tests(
            Path::new("/tmp"),
            Some(Path::new("/tmp")),
            None,
            None,
            Some(gateway),
            &user_tools,
            "auto",
        )
        .await;

        let adapter_idx = names
            .iter()
            .position(|name| name == "semantic_search")
            .expect("semantic_search tool missing");
        let user_idx = names
            .iter()
            .position(|name| name == "gh_search_repos")
            .expect("user MCP tool missing");

        assert!(adapter_idx < user_idx);
    }

    #[tokio::test]
    #[ignore = "Requires Ollama to be running"]
    async fn test_create_ollama_agent() {
        let config = test_agent_config();
        let (event_tx, _) = broadcast::channel(16);
        let result = create_agent_from_session_config(
            &config,
            None,
            Path::new("/tmp"),
            None,
            None,
            None,
            &event_tx,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(result.is_ok());
    }

    // --- Delegation context wiring tests ---

    use crucible_config::DelegationConfig;
    use crucible_core::background::{JobError, JobId, JobInfo, JobResult};
    use std::path::PathBuf;
    use std::time::Duration;

    struct MockSpawner;

    #[async_trait::async_trait]
    impl BackgroundSpawner for MockSpawner {
        async fn spawn_bash(
            &self,
            _session_id: &str,
            _command: String,
            _workdir: Option<PathBuf>,
            _timeout: Option<Duration>,
        ) -> Result<JobId, JobError> {
            unimplemented!()
        }

        async fn spawn_subagent(
            &self,
            _session_id: &str,
            _prompt: String,
            _context: Option<String>,
        ) -> Result<JobId, JobError> {
            unimplemented!()
        }

        fn list_jobs(&self, _session_id: &str) -> Vec<JobInfo> {
            vec![]
        }

        fn get_job_result(&self, _job_id: &JobId) -> Option<JobResult> {
            None
        }

        async fn cancel_job(&self, _job_id: &JobId) -> bool {
            false
        }
    }

    #[test]
    fn delegation_context_built_when_config_present() {
        let mut config = test_agent_config();
        config.delegation_config = Some(DelegationConfig {
            enabled: true,
            max_depth: 2,
            allowed_targets: Some(vec!["cursor".to_string(), "opencode".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        });

        let spawner: Arc<dyn BackgroundSpawner> = Arc::new(MockSpawner);
        let ctx = build_internal_delegation_context(
            &config,
            Some("session-123"),
            Some(spawner),
            Path::new("/tmp"),
            Some(Path::new("/tmp/kiln")),
        );

        let ctx = ctx.expect("delegation context should be Some");
        assert!(ctx.enabled);
        assert_eq!(ctx.session_id, "session-123");
        assert_eq!(
            ctx.targets,
            vec!["cursor".to_string(), "opencode".to_string()]
        );
        assert_eq!(ctx.depth, 0);
    }

    #[test]
    fn delegation_context_disabled_without_delegation_config() {
        // delegation_config = None -> context exists but enabled = false
        let config = test_agent_config();
        let spawner: Arc<dyn BackgroundSpawner> = Arc::new(MockSpawner);
        let ctx = build_internal_delegation_context(
            &config,
            Some("session-123"),
            Some(spawner),
            Path::new("/tmp"),
            Some(Path::new("/tmp/kiln")),
        );

        let ctx = ctx.expect("context present when spawner + session_id exist");
        assert!(
            !ctx.enabled,
            "should be disabled when delegation_config is None"
        );
        assert!(ctx.targets.is_empty());
    }

    #[test]
    fn delegation_context_none_without_spawner() {
        let mut config = test_agent_config();
        config.delegation_config = Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        });

        let ctx = build_internal_delegation_context(
            &config,
            Some("session-123"),
            None,
            Path::new("/tmp"),
            Some(Path::new("/tmp/kiln")),
        );

        assert!(ctx.is_none(), "should be None without background_spawner");
    }

    #[test]
    fn delegation_context_none_without_session_id() {
        let mut config = test_agent_config();
        config.delegation_config = Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        });

        let spawner: Arc<dyn BackgroundSpawner> = Arc::new(MockSpawner);
        let ctx = build_internal_delegation_context(
            &config,
            None,
            Some(spawner),
            Path::new("/tmp"),
            Some(Path::new("/tmp/kiln")),
        );

        assert!(ctx.is_none(), "should be None without parent_session_id");
    }

    #[tokio::test]
    async fn internal_agent_type_dispatches_to_internal_branch() {
        // Verify that agent_type == "internal" takes the internal creation path
        // (not the ACP path). This test validates the dispatch logic by checking
        // that the function successfully creates an agent handle for internal agents.
        let config = test_agent_config();
        assert_eq!(config.agent_type, "internal");

        let (event_tx, _) = broadcast::channel(16);
        let result = create_agent_from_session_config(
            &config,
            None,
            Path::new("/tmp"),
            None,
            None,
            None,
            &event_tx,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        // The internal branch should succeed in creating an agent handle.
        // (Ollama client creation doesn't validate connectivity, just creates the object.)
        assert!(result.is_ok(), "Internal agent creation should succeed");
    }

    #[tokio::test]
    async fn acp_agent_type_dispatches_to_acp_branch() {
        // Verify that agent_type == "acp" takes the ACP creation path
        // (not the internal path). This test validates the dispatch logic.
        let mut config = test_agent_config();
        config.agent_type = "acp".to_string();

        let (event_tx, _) = broadcast::channel(16);
        let result = create_agent_from_session_config(
            &config,
            None,
            Path::new("/tmp"),
            None,
            None,
            None,
            &event_tx,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        // The result will be an error because ACP agent creation requires
        // proper ACP config and spawner setup, but it should be an AgentBuild error
        // (from the ACP branch), not an UnsupportedAgentType error.
        match result {
            Err(AgentFactoryError::AgentBuild(_)) => {
                // Expected: ACP branch was taken and failed during ACP agent creation
            }
            Err(AgentFactoryError::UnsupportedAgentType(_)) => {
                panic!("Should not reach UnsupportedAgentType for 'acp' agent type");
            }
            Ok(_) => {
                panic!("Should fail without proper ACP config");
            }
            Err(AgentFactoryError::ClientCreation(_)) => {
                panic!("Should not reach ClientCreation for ACP agent type");
            }
        }
    }

    #[test]
    fn lua_auth_headers_override_config_when_authorization_present() {
        std::env::set_var("OPENAI_API_KEY", "config-key");

        let lua = Lua::new();
        let globals = lua.globals();
        let crucible = lua.create_table().unwrap();
        globals.set("crucible", crucible.clone()).unwrap();
        crucible_lua::auth_plugin::register_auth_module(&lua, &crucible).unwrap();
        lua.load(
            r#"
            crucible.on_provider_auth(function(ctx)
                if ctx.provider == "openai" then
                    return {
                        headers = {
                            ["Authorization"] = "Bearer lua-key"
                        }
                    }
                end
                return nil
            end)
            "#,
        )
        .exec()
        .unwrap();

        let hooks = get_provider_auth_hooks(&lua).unwrap();
        let auth_headers = fire_provider_auth_hooks(&lua, &hooks, "openai", "gpt-4o")
            .unwrap()
            .unwrap();
        let from_lua = auth_headers.get("Authorization").unwrap();
        let selected = from_lua.strip_prefix("Bearer ").unwrap_or(from_lua);

        assert_eq!(selected, "lua-key");

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn lua_auth_none_keeps_config_fallback() {
        std::env::set_var("OPENAI_API_KEY", "config-key");

        let lua = Lua::new();
        let globals = lua.globals();
        let crucible = lua.create_table().unwrap();
        globals.set("crucible", crucible.clone()).unwrap();
        crucible_lua::auth_plugin::register_auth_module(&lua, &crucible).unwrap();
        lua.load(
            r#"
            crucible.on_provider_auth(function(_ctx)
                return nil
            end)
            "#,
        )
        .exec()
        .unwrap();

        let hooks = get_provider_auth_hooks(&lua).unwrap();
        let auth_headers = fire_provider_auth_hooks(&lua, &hooks, "openai", "gpt-4o").unwrap();

        assert!(auth_headers.is_none());
        let fallback_key = std::env::var("OPENAI_API_KEY").unwrap();
        assert_eq!(fallback_key, "config-key");

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[tokio::test]
    async fn test_tool_definitions_include_get_kiln_info() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let kiln_path = temp_dir.path();

        let knowledge_repo: Arc<dyn KnowledgeRepository> = Arc::new(EmptyKnowledgeRepository);
        let embedding_provider: Arc<dyn EmbeddingProvider> = Arc::new(EmptyEmbeddingProvider);

        let tools = create_internal_mcp_tool_defs(
            Path::new("/tmp"),
            Some(kiln_path),
            None,
            &[],
            Some(knowledge_repo),
            Some(embedding_provider),
            None,
            "auto",
            None,
        )
        .await;

        let get_kiln_info_tool = tools
            .iter()
            .find(|t| t.function.name == "get_kiln_info")
            .expect("get_kiln_info tool should exist in in-process tools");
        assert!(!get_kiln_info_tool.function.description.is_empty());
    }
}
