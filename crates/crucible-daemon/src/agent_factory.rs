//! Agent factory for daemon.
//!
//! Creates `AgentHandle` instances from `SessionAgent` configuration.
//! This is a simplified version of the CLI's agent factory since
//! `SessionAgent` contains fully-resolved configuration.

use crate::acp::client::PermissionRequestHandler;
use crate::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};
use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};
use crate::provider::adapter_mapping::ChatClient;
use crate::provider::genai_handle::GenaiAgentHandle;
use crate::tools::mcp_server::CrucibleMcpServer;
use crate::tools::DelegationContext;
use crucible_core::background::BackgroundSpawner;
use crucible_core::config::credentials::resolve_copilot_oauth_token;
use crucible_core::config::{BackendType, DataClassification, LlmProviderConfig};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::session::SessionAgent;
use crucible_core::traits::auth::AuthHeaders;
use crucible_core::traits::chat::AgentHandle;
use crucible_core::traits::llm::LlmToolDefinition;
use crucible_core::traits::mcp::McpToolInfo;
use crucible_core::traits::KnowledgeRepository;
use crucible_lua::auth_plugin::{fire_provider_auth_hooks, get_provider_auth_hooks};
use mlua::Lua;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Parameters for creating internal MCP tool definitions.
pub struct CreateInternalMcpToolDefsParams<'a> {
    pub workspace: &'a Path,
    pub kiln_path: Option<&'a Path>,
    pub mcp_gateway: Option<Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>>,
    pub server_names: &'a [String],
    pub knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    pub embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    pub delegation_context: Option<DelegationContext>,
    pub mode: &'a str,
    pub gateway_all_tools_override: Option<&'a [McpToolInfo]>,
}

/// Parameters for creating an agent from session configuration.
pub struct CreateAgentFromSessionConfigParams<'a> {
    pub agent_config: &'a SessionAgent,
    pub lua: Option<&'a Lua>,
    pub workspace: &'a Path,
    pub kiln_path: Option<&'a Path>,
    pub connected_kilns: &'a [std::path::PathBuf],
    pub parent_session_id: Option<&'a str>,
    pub background_spawner: Option<Arc<dyn BackgroundSpawner>>,
    pub delegation_spawner: Option<Arc<dyn crate::delegation::DelegationSpawner>>,
    pub mcp_gateway: Option<Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>>,
    pub acp_permission_handler: Option<PermissionRequestHandler>,
    pub acp_config: Option<&'a crucible_core::config::components::acp::AcpConfig>,
    pub knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    pub embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
}

/// Build a `DelegationContext` for a session's MCP server.
///
/// This is the SINGLE builder used both when tool definitions are assembled
/// (here, via `create_agent_from_session_config`) and when the session's
/// dispatch-side MCP server is constructed (`get_or_create_session_dispatcher`)
/// — the two must agree or the LLM is advertised a `delegate_session` tool
/// the executor refuses. Requires the session id and both spawners; `None`
/// otherwise, which hides the delegation/job tools.
pub(crate) fn build_internal_delegation_context(
    agent_config: &SessionAgent,
    parent_session_id: Option<&str>,
    background_spawner: Option<Arc<dyn BackgroundSpawner>>,
    delegation_spawner: Option<Arc<dyn crate::delegation::DelegationSpawner>>,
    workspace: &Path,
    kiln_path: Option<&Path>,
) -> Option<DelegationContext> {
    let session_id = parent_session_id?;
    let background_spawner = background_spawner?;
    let delegation_spawner = delegation_spawner?;
    let delegation_config = agent_config.delegation_config.as_ref();
    Some(DelegationContext {
        background_spawner,
        delegation_spawner,
        session_id: session_id.to_string(),
        targets: delegation_config
            .and_then(|c| c.allowed_targets.clone())
            .unwrap_or_default(),
        enabled: delegation_config.map(|c| c.enabled).unwrap_or(false),
        depth: 0,
        result_max_bytes: delegation_config
            .map(|c| c.result_max_bytes)
            .unwrap_or(51200),
        timeout_secs: delegation_config.map(|c| c.timeout_secs).unwrap_or(300),
        data_classification: kiln_path
            .and_then(|kiln| crate::trust_resolution::resolve_kiln_classification(workspace, kiln))
            .unwrap_or(DataClassification::Public),
    })
}

/// Build the internal agent's tool definitions and the set of names eligible
/// for progressive disclosure. Core tools (kiln MCP + workspace) are never
/// deferrable; the deferrable set is exactly the gateway (user MCP) tool names.
async fn create_internal_mcp_tool_defs(
    params: CreateInternalMcpToolDefsParams<'_>,
) -> (Vec<LlmToolDefinition>, std::collections::HashSet<String>) {
    let CreateInternalMcpToolDefsParams {
        workspace,
        kiln_path,
        mcp_gateway,
        server_names,
        knowledge_repo,
        embedding_provider,
        delegation_context,
        mode,
        gateway_all_tools_override,
    } = params;
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

    // Add workspace tools (bash, read_file, edit_file, write_file, glob, grep)
    for tool in crate::tools::workspace::WorkspaceTools::tool_definitions() {
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

    let deferrable_names: std::collections::HashSet<String> = user_mcp_tools
        .iter()
        .map(|t| t.function.name.clone())
        .collect();
    tool_defs.extend(user_mcp_tools);
    (tool_defs, deferrable_names)
}

fn is_plan_mode_tool(name: &str) -> bool {
    crate::tools::tool_modes::PLAN_TOOL_NAMES.contains(&name)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
async fn create_internal_mcp_tool_names_for_tests(
    workspace: &Path,
    kiln_path: Option<&Path>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>>,
    server_names: &[String],
    knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    delegation_context: Option<DelegationContext>,
    mode: &str,
    gateway_all_tools_override: Option<&[McpToolInfo]>,
) -> Vec<String> {
    let (tools, _deferrable) = create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
        workspace,
        kiln_path,
        mcp_gateway,
        server_names,
        knowledge_repo,
        embedding_provider,
        delegation_context,
        mode,
        gateway_all_tools_override,
    })
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

/// Discover skills for the agent's workspace/kiln and render the tier-1 catalog
/// (name + description per skill) for the system prompt. The agent loads full
/// instructions on demand via the `skill_view` tool. Best-effort: discovery
/// failures yield an empty catalog rather than blocking agent creation.
fn discover_skills_catalog(workspace: &Path, kiln_path: Option<&Path>) -> String {
    let discovery = crate::skills::FolderDiscovery::with_default_paths(workspace, kiln_path);
    match discovery.discover() {
        Ok(skills) => crate::skills::format_skills_for_context(&skills),
        Err(e) => {
            warn!("Skill discovery failed; agent runs without a skills catalog: {e}");
            String::new()
        }
    }
}

fn build_enriched_prompt(
    workspace: &Path,
    kiln_path: Option<&Path>,
    connected_kilns: &[std::path::PathBuf],
    base_prompt: &str,
    skills_catalog: &str,
) -> String {
    let mut enriched_prompt = String::new();
    enriched_prompt.push_str(&format!("Workspace: {}\n", workspace.display()));
    if let Some(kiln) = kiln_path {
        enriched_prompt.push_str(&format!("Kiln: {}\n", kiln.display()));
    }

    // List knowledge bases by name
    let mut kb_names: Vec<String> = Vec::new();
    if let Some(primary) = kiln_path {
        if let Some(cfg) = crucible_core::config::read_kiln_config(primary) {
            kb_names.push(format!("{} (primary)", cfg.kiln.name));
        }
    }
    for kiln in connected_kilns {
        // Skip if same as primary kiln to avoid duplicate listing
        if kiln_path.is_some_and(|p| p == kiln.as_path()) {
            continue;
        }
        if let Some(cfg) = crucible_core::config::read_kiln_config(kiln) {
            kb_names.push(cfg.kiln.name.clone());
        }
    }
    if !kb_names.is_empty() {
        enriched_prompt.push_str("\nKnowledge bases:\n");
        for name in &kb_names {
            enriched_prompt.push_str(&format!("- {}\n", name));
        }
    }

    if !base_prompt.is_empty() {
        enriched_prompt.push('\n');
        enriched_prompt.push_str(base_prompt);
    }

    if !skills_catalog.is_empty() {
        enriched_prompt.push('\n');
        enriched_prompt.push_str(skills_catalog);
    }
    enriched_prompt
}

/// Build a bare genai chat client + model identity from a session's agent
/// config, resolving credentials the same way full agent construction does
/// (env var → Lua auth hooks → Copilot OAuth exchange). Shared by agent
/// creation and one-shot completions (e.g. session title generation) that
/// must not spin up tools or touch conversation history.
pub(crate) fn build_chat_client_for_agent(
    agent_config: &SessionAgent,
    lua: Option<&Lua>,
) -> Result<(genai::Client, genai::ModelIden), AgentFactoryError> {
    let provider_type = agent_config.provider;

    let mut llm_config = LlmProviderConfig::builder(provider_type);
    if let Some(endpoint) = agent_config.endpoint.clone() {
        llm_config = llm_config.endpoint(endpoint);
    }
    let mut llm_config = llm_config
        .model(agent_config.model.clone())
        .with_api_key_env_var_name()
        .build();

    // Resolve the env var value — with_api_key_env_var_name() stores the env var
    // NAME (e.g. "GLM_AUTH_TOKEN"), not the actual token. Look it up now so genai
    // sends the real credential in the Authorization header.
    if let Some(env_var_name) = &llm_config.api_key {
        match std::env::var(env_var_name) {
            Ok(resolved) => {
                if !resolved.is_empty() {
                    llm_config.api_key = Some(resolved);
                }
            }
            Err(e) => {
                warn!(
                    "Failed to resolve API key env var '{}': {} — clearing api_key",
                    env_var_name, e
                );
                llm_config.api_key = None;
            }
        }
    }

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

    if agent_config.provider == BackendType::GitHubCopilot {
        if let Some(oauth_token) = resolve_copilot_oauth_token(llm_config.api_key.as_deref()) {
            llm_config.api_key = Some(oauth_token);
        }
    }

    let chat_client = ChatClient::new(&llm_config);
    let model_iden = chat_client.model_iden(&agent_config.model).ok_or_else(|| {
        AgentFactoryError::ClientCreation(format!(
            "Unsupported provider for chat: {:?}",
            provider_type
        ))
    })?;
    let genai_client = chat_client.inner().clone();
    Ok((genai_client, model_iden))
}

/// Create an agent handle from session configuration.
///
/// This takes the fully-resolved `SessionAgent` and creates a ready-to-use
/// `Box<dyn AgentHandle>`. Unlike the CLI factory, this doesn't need to:
/// - Discover skills (already in system_prompt)
/// - Load rules files (already in system_prompt)
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
    params: CreateAgentFromSessionConfigParams<'_>,
) -> Result<Box<dyn AgentHandle + Send + Sync>, AgentFactoryError> {
    let CreateAgentFromSessionConfigParams {
        agent_config,
        lua,
        workspace,
        kiln_path,
        connected_kilns,
        parent_session_id,
        background_spawner,
        delegation_spawner,
        mcp_gateway,
        acp_permission_handler,
        acp_config,
        knowledge_repo,
        embedding_provider,
    } = params;
    if agent_config.agent_type == "acp" {
        let handle = AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config,
            workspace,
            kiln_path,
            knowledge_repo,
            embedding_provider,
            background_spawner,
            delegation_spawner,
            parent_session_id,
            delegation_config: agent_config.delegation_config.as_ref(),
            acp_config,
            permission_handler: acp_permission_handler,
        })
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
        delegation_spawner.clone(),
        workspace,
        kiln_path,
    );
    let (tool_defs, deferrable_tool_names) =
        create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            workspace,
            kiln_path,
            mcp_gateway: mcp_gateway.clone(),
            server_names: &agent_config.mcp_servers,
            knowledge_repo,
            embedding_provider,
            delegation_context,
            mode,
            gateway_all_tools_override: None,
        })
        .await;

    info!(
        provider = %agent_config.provider,
        model = %agent_config.model,
        workspace = %workspace.display(),
        "Creating agent from session config"
    );

    let (genai_client, model_iden) = build_chat_client_for_agent(agent_config, lua)?;

    // Skills are surfaced via the kiln-scoped `skill_view` tool, so only inject
    // the catalog when a kiln is present (keeps catalog and tool in lockstep).
    // Note: discovery can also find personal/cross-harness skills with no kiln,
    // but a kiln-less session has no `skill_view` to load them — so we skip the
    // catalog entirely rather than advertise skills the agent can't open.
    let skills_catalog = if kiln_path.is_some() {
        discover_skills_catalog(workspace, kiln_path)
    } else {
        String::new()
    };
    let enriched_prompt = build_enriched_prompt(
        workspace,
        kiln_path,
        connected_kilns,
        &agent_config.system_prompt,
        &skills_catalog,
    );

    let handle = GenaiAgentHandle::with_workspace(
        genai_client,
        model_iden,
        &enriched_prompt,
        tool_defs,
        agent_config.thinking_budget,
        workspace.to_path_buf(),
    )
    .with_deferrable_tools(deferrable_tool_names);

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
    use std::sync::Mutex;
    use tokio::sync::RwLock;

    static OPENAI_API_KEY_LOCK: Mutex<()> = Mutex::new(());

    async fn build_internal_tool_names_for_tests(
        workspace: &Path,
        kiln_path: Option<&Path>,
        knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
        embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
        mcp_gateway: Option<Arc<RwLock<crate::tools::mcp_gateway::McpGatewayManager>>>,
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
            mode: None,
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
            precognition_results: 5,
            max_iterations: None,
            execution_timeout_secs: None,
            context_budget: None,
            context_strategy: Default::default(),
            context_window: None,
            output_validation: Default::default(),
            validation_retries: 3,
            autocompact_threshold: None,
        }
    }

    #[test]
    fn enriched_prompt_prepends_workspace_and_kiln_context() {
        let ws = Path::new("/repo");
        let kiln = Path::new("/repo/docs");

        // With kiln + base prompt: both paths present, workspace before base
        let enriched = build_enriched_prompt(ws, Some(kiln), &[], "You are helpful.", "");
        assert!(enriched.contains("Workspace: /repo"));
        assert!(enriched.contains("Kiln: /repo/docs"));
        assert!(enriched.contains("You are helpful."));
        assert!(enriched.find("Workspace:").unwrap() < enriched.find("You are helpful.").unwrap());

        // Without kiln: no Kiln line
        let no_kiln = build_enriched_prompt(ws, None, &[], "Base.", "");
        assert!(no_kiln.contains("Workspace: /repo"));
        assert!(!no_kiln.contains("Kiln:"));
        assert!(no_kiln.contains("Base."));

        // Empty base prompt: just context lines, no double blank
        let empty_base = build_enriched_prompt(ws, None, &[], "", "");
        assert!(empty_base.contains("Workspace: /repo"));
        assert!(!empty_base.ends_with("\n\n"));

        // Skills catalog is appended after the base prompt when present
        let with_skills = build_enriched_prompt(
            ws,
            Some(kiln),
            &[],
            "Base.",
            "# Available Skills\n\n## commit\n",
        );
        assert!(with_skills.contains("# Available Skills"));
        assert!(
            with_skills.find("Base.").unwrap() < with_skills.find("# Available Skills").unwrap()
        );
    }

    #[test]
    fn build_enriched_prompt_includes_kiln_names() {
        let tmp = tempfile::TempDir::new().unwrap();
        let crucible_dir = tmp.path().join(".crucible");
        std::fs::create_dir_all(&crucible_dir).unwrap();
        std::fs::write(
            crucible_dir.join("kiln.toml"),
            "[kiln]\nname = \"My Kiln\"\n",
        )
        .unwrap();

        let result =
            build_enriched_prompt(Path::new("/workspace"), Some(tmp.path()), &[], "base", "");
        assert!(
            result.contains("Knowledge bases:"),
            "should have kb section"
        );
        assert!(
            result.contains("My Kiln (primary)"),
            "should list primary kiln"
        );
    }

    #[test]
    fn build_enriched_prompt_no_kiln_names_when_no_config() {
        let result = build_enriched_prompt(Path::new("/workspace"), None, &[], "base", "");
        assert!(
            !result.contains("Knowledge bases:"),
            "no kb section when no kiln"
        );
    }

    #[test]
    fn test_unsupported_agent_type() {
        let mut config = test_agent_config();
        config.agent_type = "unknown".to_string();

        let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
            create_agent_from_session_config(CreateAgentFromSessionConfigParams {
                agent_config: &config,
                lua: None,
                workspace: Path::new("/tmp"),
                kiln_path: None,
                connected_kilns: &[],
                parent_session_id: None,
                background_spawner: None,
                delegation_spawner: None,
                mcp_gateway: None,
                acp_permission_handler: None,
                acp_config: None,
                knowledge_repo: None,
                embedding_provider: None,
            })
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
            crate::tools::mcp_gateway::McpGatewayManager::new(),
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
        // delegate_session is filtered out when no delegation context is provided
        assert!(!names.iter().any(|name| name == "delegate_session"));
        assert!(names.iter().any(|name| name == "list_jobs"));
    }

    fn many_gateway_tools(n: usize) -> Vec<McpToolInfo> {
        (0..n)
            .map(|i| McpToolInfo {
                name: format!("tool_{i}"),
                prefixed_name: format!("gh_tool_{i}"),
                description: Some("a gateway tool with a description".to_string()),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {"q": {"type": "string", "description": "a query"}}
                }),
                upstream: "gh".to_string(),
            })
            .collect()
    }

    /// End-to-end through the factory: an over-budget agent attaches core tools
    /// plus the three bridge defs and drops every gateway def; the plan-mode
    /// variant attaches no gateway defs at all.
    #[tokio::test]
    async fn over_budget_agent_attaches_core_plus_bridge_and_plan_excludes_gateway() {
        use crucible_core::traits::chat::AgentHandle;

        let gateway_tools = many_gateway_tools(12);
        let gateway = Arc::new(RwLock::new(
            crate::tools::mcp_gateway::McpGatewayManager::new(),
        ));
        let (defs, deferrable) = create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            workspace: Path::new("/tmp"),
            kiln_path: Some(Path::new("/tmp")),
            mcp_gateway: Some(gateway),
            server_names: &["gh".to_string()],
            knowledge_repo: None,
            embedding_provider: None,
            delegation_context: None,
            mode: "auto",
            gateway_all_tools_override: Some(&gateway_tools),
        })
        .await;
        assert_eq!(deferrable.len(), 12, "all gateway tools are deferrable");

        let config = LlmProviderConfig::builder(BackendType::OpenAI)
            .model("gpt-4o-mini")
            .build();
        let chat_client = ChatClient::new(&config);
        let client = chat_client.inner().clone();
        let model = chat_client
            .model_iden("gpt-4o-mini")
            .expect("model iden for gpt-4o-mini");
        let mut handle = GenaiAgentHandle::with_workspace(
            client,
            model,
            "system",
            defs,
            None,
            std::path::PathBuf::new(),
        )
        .with_deferrable_tools(deferrable);
        // Tiny budget → the tool schemas exceed the 15% share.
        handle.set_context_budget(Some(1_000)).await.unwrap();

        let (names, deferred) = handle.visible_tool_names_for_test();
        assert_eq!(deferred, 12, "every gateway tool deferred");
        assert!(names.iter().any(|n| n == "discover_tools"));
        assert!(names.iter().any(|n| n == "get_tool_schema"));
        assert!(names.iter().any(|n| n == "invoke_tool"));
        assert!(
            !names.iter().any(|n| n.starts_with("gh_")),
            "no gateway defs attached natively: {names:?}"
        );
        // Core kiln + workspace tools remain attached.
        assert!(names.iter().any(|n| n == "semantic_search"));
        assert!(names.iter().any(|n| n == "read_file"));

        // Plan-mode variant: gateway defs excluded categorically.
        handle.set_mode_str("plan").await.unwrap();
        let (plan_names, _) = handle.visible_tool_names_for_test();
        assert!(
            !plan_names.iter().any(|n| n.starts_with("gh_")),
            "no gateway defs in plan mode: {plan_names:?}"
        );
    }

    #[tokio::test]
    async fn adapter_tools_come_before_user_mcp_tools() {
        let gateway = Arc::new(RwLock::new(
            crate::tools::mcp_gateway::McpGatewayManager::new(),
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
        let result = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
            agent_config: &config,
            lua: None,
            workspace: Path::new("/tmp"),
            kiln_path: None,
            connected_kilns: &[],
            parent_session_id: None,
            background_spawner: None,
            delegation_spawner: None,
            mcp_gateway: None,
            acp_permission_handler: None,
            acp_config: None,
            knowledge_repo: None,
            embedding_provider: None,
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn internal_agent_type_dispatches_to_internal_branch() {
        // Verify that agent_type == "internal" takes the internal creation path
        // (not the ACP path). This test validates the dispatch logic by checking
        // that the function successfully creates an agent handle for internal agents.
        let config = test_agent_config();
        assert_eq!(config.agent_type, "internal");

        let result = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
            agent_config: &config,
            lua: None,
            workspace: Path::new("/tmp"),
            kiln_path: None,
            connected_kilns: &[],
            parent_session_id: None,
            background_spawner: None,
            delegation_spawner: None,
            mcp_gateway: None,
            acp_permission_handler: None,
            acp_config: None,
            knowledge_repo: None,
            embedding_provider: None,
        })
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

        let result = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
            agent_config: &config,
            lua: None,
            workspace: Path::new("/tmp"),
            kiln_path: None,
            connected_kilns: &[],
            parent_session_id: None,
            background_spawner: None,
            delegation_spawner: None,
            mcp_gateway: None,
            acp_permission_handler: None,
            acp_config: None,
            knowledge_repo: None,
            embedding_provider: None,
        })
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
        let _env_lock = OPENAI_API_KEY_LOCK
            .lock()
            .expect("OPENAI_API_KEY_LOCK should not be poisoned");
        let _guard = crucible_core::test_support::EnvVarGuard::set(
            "OPENAI_API_KEY",
            "config-key".to_string(),
        );

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
    }

    #[test]
    fn lua_auth_none_keeps_config_fallback() {
        let _env_lock = OPENAI_API_KEY_LOCK
            .lock()
            .expect("OPENAI_API_KEY_LOCK should not be poisoned");
        let _guard = crucible_core::test_support::EnvVarGuard::set(
            "OPENAI_API_KEY",
            "config-key".to_string(),
        );

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
    }

    #[tokio::test]
    async fn test_tool_definitions_include_get_kiln_info() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let kiln_path = temp_dir.path();

        let knowledge_repo: Arc<dyn KnowledgeRepository> = Arc::new(EmptyKnowledgeRepository);
        let embedding_provider: Arc<dyn EmbeddingProvider> = Arc::new(EmptyEmbeddingProvider);

        let (tools, _deferrable) = create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            workspace: Path::new("/tmp"),
            kiln_path: Some(kiln_path),
            mcp_gateway: None,
            server_names: &[],
            knowledge_repo: Some(knowledge_repo),
            embedding_provider: Some(embedding_provider),
            delegation_context: None,
            mode: "auto",
            gateway_all_tools_override: None,
        })
        .await;

        let get_kiln_info_tool = tools
            .iter()
            .find(|t| t.function.name == "get_kiln_info")
            .expect("get_kiln_info tool should exist in in-process tools");
        assert!(!get_kiln_info_tool.function.description.is_empty());
    }

    #[tokio::test]
    async fn workspace_tools_in_agent_tool_defs() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let kiln_path = temp_dir.path();

        let knowledge_repo: Arc<dyn KnowledgeRepository> = Arc::new(EmptyKnowledgeRepository);
        let embedding_provider: Arc<dyn EmbeddingProvider> = Arc::new(EmptyEmbeddingProvider);

        let (tools, _deferrable) = create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            workspace: Path::new("/tmp"),
            kiln_path: Some(kiln_path),
            mcp_gateway: None,
            server_names: &[],
            knowledge_repo: Some(knowledge_repo),
            embedding_provider: Some(embedding_provider),
            delegation_context: None,
            mode: "auto",
            gateway_all_tools_override: None,
        })
        .await;

        let tool_names: Vec<String> = tools.iter().map(|t| t.function.name.clone()).collect();

        // These assertions FAIL because workspace tools are not yet included
        assert!(
            tool_names.iter().any(|name| name == "bash"),
            "bash tool should be in agent tool defs"
        );
        assert!(
            tool_names.iter().any(|name| name == "read_file"),
            "read_file tool should be in agent tool defs"
        );
        assert!(
            tool_names.iter().any(|name| name == "edit_file"),
            "edit_file tool should be in agent tool defs"
        );
        assert!(
            tool_names.iter().any(|name| name == "write_file"),
            "write_file tool should be in agent tool defs"
        );
        assert!(
            tool_names.iter().any(|name| name == "glob"),
            "glob tool should be in agent tool defs"
        );
        assert!(
            tool_names.iter().any(|name| name == "grep"),
            "grep tool should be in agent tool defs"
        );
    }

    #[test]
    fn is_safe_classifies_workspace_tools() {
        use crate::agent_manager::is_safe;

        // These assertions test the current state of is_safe()
        // Some may FAIL if is_safe() doesn't have these tool names yet
        assert!(
            !is_safe("bash"),
            "bash should be unsafe (runs arbitrary commands)"
        );
        assert!(is_safe("read_file"), "read_file should be safe (read-only)");
        assert!(
            !is_safe("write_file"),
            "write_file should be unsafe (modifies files)"
        );
        assert!(
            !is_safe("edit_file"),
            "edit_file should be unsafe (modifies files)"
        );
        assert!(is_safe("glob"), "glob should be safe (read-only)");
        assert!(is_safe("grep"), "grep should be safe (read-only)");
    }
}
