//! Agent factory for daemon.
//!
//! Creates `AgentHandle` instances from `SessionAgent` configuration.
//! This is a simplified version of the CLI's agent factory since
//! `SessionAgent` contains fully-resolved configuration.

use crate::acp_handle::AcpAgentHandle;
use crate::event_emitter::emit_event;
use crate::protocol::SessionEventMessage;
use crucible_acp::client::PermissionRequestHandler;
use crucible_config::credentials::resolve_copilot_oauth_token;
use crucible_config::{BackendType, DataClassification, LlmProviderConfig};
use crucible_core::background::BackgroundSpawner;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::interaction_registry::InteractionRegistry;
use crucible_core::prompts::ModelSize;
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use crucible_core::traits::mcp::McpToolInfo;
use crucible_core::traits::KnowledgeRepository;
use crucible_core::{EventPushCallback, InteractionContext};
use crucible_rig::{
    create_client, mcp_tools_from_gateway, AgentConfig, HandleBuildOpts, McpProxyTool,
    WorkspaceContext,
};
use crucible_tools::in_process_adapter::InProcessMcpAdapter;
use crucible_tools::mcp_server::CrucibleMcpServer;
use crucible_tools::DelegationContext;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{debug, info};

struct EmptyKnowledgeRepository;

#[async_trait::async_trait]
impl KnowledgeRepository for EmptyKnowledgeRepository {
    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }

    async fn list_notes(
        &self,
        _path: Option<&str>,
    ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
        Ok(vec![])
    }

    async fn search_vectors(
        &self,
        _vector: Vec<f32>,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        Ok(vec![])
    }
}

struct EmptyEmbeddingProvider;

#[async_trait::async_trait]
impl EmbeddingProvider for EmptyEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!("Embedding provider unavailable for in-process MCP adapter")
    }

    async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        anyhow::bail!("Embedding provider unavailable for in-process MCP adapter")
    }

    fn model_name(&self) -> &str {
        "unavailable"
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn provider_name(&self) -> &str {
        "none"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
}

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
                    .map(|kiln| crate::trust_resolution::resolve_kiln_classification(workspace, kiln))
                    .unwrap_or(DataClassification::Public),
            }
        })
    })
}

async fn create_internal_mcp_tools(
    workspace: &Path,
    kiln_path: Option<&Path>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
    server_names: &[String],
    knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    delegation_context: Option<DelegationContext>,
    mode: &str,
    gateway_all_tools_override: Option<&[McpToolInfo]>,
) -> Vec<McpProxyTool> {
    let mut combined_tools = Vec::new();

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
        let adapter = InProcessMcpAdapter::new(Arc::new(server));
        let adapter_gateway = Arc::new(tokio::sync::RwLock::new(
            crucible_tools::mcp_gateway::McpGatewayManager::new(),
        ));

        for tool in adapter.create_rig_tools(mode) {
            let definition = tool.definition(String::new()).await;
            let info = McpToolInfo {
                name: definition.name.clone(),
                prefixed_name: definition.name,
                description: Some(definition.description),
                input_schema: definition.parameters,
                upstream: "crucible".to_string(),
            };
            combined_tools.push(McpProxyTool::new(&info, adapter_gateway.clone()));
        }

        info!(
            count = combined_tools.len(),
            kiln = %kiln_path.display(),
            mode,
            "Resolved in-process Crucible MCP adapter tools"
        );
    } else {
        debug!(
            workspace = %workspace.display(),
            "Skipping in-process Crucible MCP adapter tools because kiln path is unavailable"
        );
    }

    let user_mcp_tools: Vec<McpProxyTool> = if let Some(ref gw) = mcp_gateway {
        let gw_read = gw.read().await;
        debug!(
            tool_count = gw_read.tool_count(),
            mcp_servers = ?server_names,
            "MCP gateway available for agent"
        );

        if !server_names.is_empty() {
            let all_tools_owned;
            let all_tools: &[McpToolInfo] = if let Some(override_tools) = gateway_all_tools_override {
                override_tools
            } else {
                all_tools_owned = gw_read.all_tools();
                &all_tools_owned
            };
            drop(gw_read);
            let tools = mcp_tools_from_gateway(gw, server_names, all_tools);
            info!(count = tools.len(), servers = ?server_names, "Resolved MCP proxy tools");
            tools
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    combined_tools.extend(user_mcp_tools);
    combined_tools
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
    let mut names = Vec::new();

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
        let adapter = InProcessMcpAdapter::new(Arc::new(server));
        for tool in adapter.create_rig_tools(mode) {
            names.push(tool.definition(String::new()).await.name);
        }
    } else {
        debug!(
            workspace = %workspace.display(),
            "Skipping in-process Crucible MCP adapter tools because kiln path is unavailable"
        );
    }

    if let Some(ref gw) = mcp_gateway {
        let gw_read = gw.read().await;
        if !server_names.is_empty() {
            let all_tools_owned;
            let all_tools: &[McpToolInfo] = if let Some(override_tools) = gateway_all_tools_override {
                override_tools
            } else {
                all_tools_owned = gw_read.all_tools();
                &all_tools_owned
            };
            names.extend(
                all_tools
                    .iter()
                    .filter(|tool| server_names.contains(&tool.upstream))
                    .map(|tool| tool.prefixed_name.clone()),
            );
        }
    }

    names
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
pub async fn create_agent_from_session_config(
    agent_config: &SessionAgent,
    workspace: &Path,
    kiln_path: Option<&Path>,
    parent_session_id: Option<&str>,
    background_spawner: Option<Arc<dyn BackgroundSpawner>>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
    acp_permission_handler: Option<PermissionRequestHandler>,
    _knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    _embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
) -> Result<Box<dyn AgentHandle + Send + Sync>, AgentFactoryError> {
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
            None,
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

    let mode = "auto";
    let delegation_context = build_internal_delegation_context(
        agent_config,
        parent_session_id,
        background_spawner.clone(),
        workspace,
        kiln_path,
    );
    let mcp_tools = create_internal_mcp_tools(
        workspace,
        kiln_path,
        mcp_gateway.clone(),
        &agent_config.mcp_servers,
        _knowledge_repo,
        _embedding_provider,
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

    let provider_type = agent_config.provider;

    let mut llm_config = LlmProviderConfig::builder(provider_type);
    if let Some(endpoint) = agent_config.endpoint.clone() {
        llm_config = llm_config.endpoint(endpoint);
    }
    let mut llm_config = llm_config
        .model(agent_config.model.clone())
        .with_api_key_env_var_name()
        .build();

    if agent_config.provider == BackendType::GitHubCopilot {
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
        let _ = emit_event(
            &event_tx_clone,
            SessionEventMessage::new("session", "interaction_event", serde_json::json!({})),
        );
    });
    let interaction_ctx = Arc::new(InteractionContext::new(registry, push_event));

    let delegation_targets = agent_config
        .delegation_config
        .as_ref()
        .and_then(|config| config.allowed_targets.clone())
        .unwrap_or_default();

    let mut ws_ctx = WorkspaceContext::new(workspace)
        .with_delegation_targets(delegation_targets)
        .with_interaction_context(interaction_ctx);
    if let Some(ref spawner) = background_spawner {
        ws_ctx = ws_ctx.with_background_spawner(spawner.clone());
    }

    let opts = HandleBuildOpts {
        model: agent_config.model.clone(),
        model_size,
        thinking_budget,
        ollama_endpoint,
        mcp_gateway,
        initial_mode: None,
        reasoning_endpoint: None,
    };

    let handle = client
        .build_agent_handle(&rig_agent_config, &ws_ctx, mcp_tools, opts)
        .await
        .map_err(AgentFactoryError::AgentBuild)?;

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
                Path::new("/tmp"),
                None,
                None,
                None,
                &event_tx,
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
            Path::new("/tmp"),
            None,
            None,
            None,
            &event_tx,
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
        assert_eq!(ctx.targets, vec!["cursor".to_string(), "opencode".to_string()]);
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
        assert!(!ctx.enabled, "should be disabled when delegation_config is None");
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
}
