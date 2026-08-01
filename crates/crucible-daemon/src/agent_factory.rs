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
    /// Lua-declared modes. `None` (tests, callers with no daemon registry)
    /// falls back to the shipped definitions, so a missing registry restricts
    /// plan mode rather than opening it.
    pub modes: Option<&'a crucible_lua::ModeRegistry>,
    pub gateway_all_tools_override: Option<&'a [McpToolInfo]>,
    /// Agent-card tool policy: tools marked Deny are never advertised.
    pub tool_policy: Option<&'a crucible_core::agent::ToolPolicyMap>,
    /// Spec-declared plugin tools. `None` when no plugin loader is attached.
    pub plugin_tools: Option<Arc<crate::plugin_tools::PluginRegistry>>,
}

/// Parameters for creating an agent from session configuration.
pub struct CreateAgentFromSessionConfigParams<'a> {
    /// Lua-declared modes, so a declared mode's `tools` selector filters the
    /// live tool set. `None` keeps the shipped plan-mode behaviour.
    pub modes: Option<crucible_lua::ModeRegistry>,
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
    /// `[context]` from the daemon's config, deciding which project rules
    /// files are loaded into the prompt. `None` uses the defaults.
    pub context_config: Option<&'a crucible_core::config::ContextConfig>,
    pub knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    pub embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    /// Spec-declared plugin tools, so the model can see what the dispatcher
    /// can already run. `None` when no plugin loader is attached.
    pub plugin_tools: Option<Arc<crate::plugin_tools::PluginRegistry>>,
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
            .and_then(|kiln| {
                crate::trust_resolution::resolve_session_classification(workspace, kiln)
            })
            .unwrap_or(DataClassification::Public),
    })
}

/// Build the internal agent's tool definitions plus two name sets: the tools
/// eligible for progressive disclosure (exactly the gateway/user-MCP names —
/// core kiln + workspace tools are never deferrable), and the plugin-declared
/// tool names (for the per-request plan-mode filter and dispatch guard).
async fn create_internal_mcp_tool_defs(
    params: CreateInternalMcpToolDefsParams<'_>,
) -> (
    Vec<LlmToolDefinition>,
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
) {
    let CreateInternalMcpToolDefsParams {
        modes,
        workspace,
        kiln_path,
        mcp_gateway,
        server_names,
        knowledge_repo,
        embedding_provider,
        delegation_context,
        mode,
        gateway_all_tools_override,
        tool_policy,
        plugin_tools,
    } = params;
    let denied = |name: &str| {
        tool_policy
            .and_then(|m| m.get(name))
            .is_some_and(|p| *p == crucible_core::agent::ToolPolicy::Deny)
    };
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
            if !mode_exposes_tool(modes, mode, &tool_name) {
                continue;
            }
            if denied(&tool_name) {
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
        if !mode_exposes_tool(modes, mode, &tool_name) {
            continue;
        }
        if denied(&tool_name) {
            continue;
        }
        tool_defs.push(LlmToolDefinition::new(
            tool_name,
            tool.description.map(|d| d.to_string()).unwrap_or_default(),
            serde_json::Value::Object((*tool.input_schema).clone()),
        ));
    }

    // Plugin-declared tools. Never deferrable: there are few of them and the
    // progressive-disclosure bridge exists to keep large gateway catalogs out
    // of the prompt, not to hide the handful a user installed on purpose.
    //
    // Attached in every mode; plan-mode exclusion happens per-request in
    // `visible_tools()` (and at dispatch), keyed by the names returned here.
    // Excluding them at creation looked equivalent but wasn't: mode is
    // captured when the agent is built, so a session switched to plan mid-run
    // kept its plugin tools advertised and dispatchable — plan mode is an
    // allowlist, and a plugin tool's side effects are unknown to us.
    let mut plugin_tool_names = std::collections::HashSet::new();
    if let Some(registry) = plugin_tools {
        for def in registry.tool_definitions() {
            if denied(&def.name) {
                continue;
            }
            plugin_tool_names.insert(def.name.clone());
            tool_defs.push(LlmToolDefinition::new(
                def.name,
                def.description,
                def.parameters.unwrap_or_else(|| serde_json::json!({})),
            ));
        }
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
                .filter(|tool| !denied(&tool.prefixed_name))
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
    (tool_defs, deferrable_names, plugin_tool_names)
}

/// Whether `mode` exposes `name`.
///
/// Consults the Lua registry when the mode is declared there, so a
/// user-defined mode filters tools without any Rust change. Falls back to the
/// shipped read-only list for plan, and to "everything" otherwise — the
/// fallback matters: a daemon whose Lua failed to load must still restrict
/// plan mode rather than silently opening it up.
fn mode_exposes_tool(modes: Option<&crucible_lua::ModeRegistry>, mode: &str, name: &str) -> bool {
    if let Some(definition) = modes.and_then(|m| m.get(mode)) {
        return definition.tools.matches(name);
    }
    if mode == "plan" {
        return crate::tools::tool_modes::PLAN_TOOL_NAMES.contains(&name);
    }
    // A mode the daemon ships needs no declaration to be legitimate. This is
    // the ordinary un-configured state: defaults not yet loaded, failed to
    // parse, or shadowed by a runtimepath entry. Keying the branch below on
    // `is_some()` alone caught it and crippled every agent — `PLAN_TOOL_NAMES`
    // has no workspace tools at all, so `read_file`, `grep` and `bash` all
    // silently vanished.
    if crate::tools::tool_modes::is_builtin_mode(mode) {
        return true;
    }
    // A mode that is neither declared nor built in has gone away. Fail CLOSED
    // to the read-only set rather than advertising everything: an unknown mode
    // used to be the most permissive one, which turned deleting a restrictive
    // declaration into a privilege escalation.
    if modes.is_some() {
        return crate::tools::tool_modes::PLAN_TOOL_NAMES.contains(&name);
    }
    true
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
    let (tools, _deferrable, _plugin_names) =
        create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            modes: None,
            workspace,
            kiln_path,
            mcp_gateway,
            server_names,
            knowledge_repo,
            embedding_provider,
            delegation_context,
            mode,
            gateway_all_tools_override,
            tool_policy: None,
            plugin_tools: None,
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

/// The system prompt, split at its caching boundary.
///
/// Prompt caching matches on a token *prefix*, so anything that varies per
/// session poisons everything behind it. The workspace path used to be the very
/// first line of the prompt, which meant two sessions in different projects
/// diverged at byte zero and shared nothing — not the persona, not `AGENTS.md`,
/// not the skills catalog. Splitting the two halves lets the stable one carry
/// its own cache breakpoint.
pub struct EnrichedPrompt {
    /// Identical across every session that shares this agent configuration:
    /// persona, project rules, skills catalog. Cacheable across projects.
    pub stable: String,
    /// This session's surroundings: workspace, kiln, knowledge-base names.
    /// Changes per session, so it goes last and gets its own breakpoint.
    pub volatile: String,
}

impl EnrichedPrompt {
    /// The whole prompt as one string, in the order the model reads it.
    ///
    /// For callers that need the text rather than the cache structure —
    /// `session.get_system_prompt`, tests, anything logging it.
    pub fn combined(&self) -> String {
        match (self.stable.is_empty(), self.volatile.is_empty()) {
            (_, true) => self.stable.clone(),
            (true, _) => self.volatile.clone(),
            _ => format!("{}\n\n{}", self.stable, self.volatile),
        }
    }
}

fn build_enriched_prompt(
    workspace: &Path,
    kiln_path: Option<&Path>,
    connected_kilns: &[std::path::PathBuf],
    base_prompt: &str,
    rules: &str,
    skills_catalog: &str,
) -> EnrichedPrompt {
    // Most stable first. The persona is identical for every session on this
    // config; project rules change per project; the skills catalog per kiln.
    let mut stable = String::new();
    if !base_prompt.is_empty() {
        stable.push_str(base_prompt);
    }
    // After the agent card: project rules refine who the agent is, they do not
    // define it. Before the skills catalog, which is a tool listing.
    if !rules.is_empty() {
        if !stable.is_empty() {
            stable.push('\n');
        }
        stable.push_str(rules);
    }
    if !skills_catalog.is_empty() {
        if !stable.is_empty() {
            stable.push('\n');
        }
        stable.push_str(skills_catalog);
    }

    // Least stable last, so it never sits inside the cached prefix.
    let mut volatile = String::new();
    volatile.push_str(&format!("Workspace: {}\n", workspace.display()));
    if let Some(kiln) = kiln_path {
        volatile.push_str(&format!("Kiln: {}\n", kiln.display()));
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
        volatile.push_str("\nKnowledge bases:\n");
        for name in &kb_names {
            volatile.push_str(&format!("- {}\n", name));
        }
    }

    EnrichedPrompt {
        stable,
        volatile: volatile.trim_end().to_string(),
    }
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
/// - Discover skills (done here, from the workspace and kiln)
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
        modes,
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
        context_config,
        knowledge_repo,
        embedding_provider,
        plugin_tools,
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
    let (tool_defs, deferrable_tool_names, plugin_tool_names) =
        create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            modes: modes.as_ref(),
            workspace,
            kiln_path,
            mcp_gateway: mcp_gateway.clone(),
            server_names: &agent_config.mcp_servers,
            knowledge_repo,
            embedding_provider,
            delegation_context,
            mode,
            gateway_all_tools_override: None,
            tool_policy: agent_config.tool_policy.as_ref(),
            plugin_tools,
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
    let default_context_config;
    let context_config = match context_config {
        Some(c) => c,
        None => {
            default_context_config = crucible_core::config::ContextConfig::default();
            &default_context_config
        }
    };
    let rules = crate::rules_files::load_rules_files(workspace, &context_config.rules_files);

    let enriched_prompt = build_enriched_prompt(
        workspace,
        kiln_path,
        connected_kilns,
        &agent_config.system_prompt,
        &rules,
        &skills_catalog,
    );

    let handle = GenaiAgentHandle::with_workspace(
        genai_client,
        model_iden,
        &enriched_prompt.stable,
        tool_defs,
        agent_config.thinking_budget,
        workspace.to_path_buf(),
    )
    // Separate from the prompt above so it can carry its own cache breakpoint;
    // it is the half that changes per session.
    .with_session_context(enriched_prompt.volatile)
    .with_deferrable_tools(deferrable_tool_names)
    .with_plugin_tools(plugin_tool_names)
    // Everything the session decided about generation and context. This is
    // the only hop where it can arrive: every setter that writes these to
    // `SessionAgent` — RPC, `cru.defaults`, an agent card, `[llm]` config —
    // invalidates the agent cache, so the handle is always rebuilt here.
    // Omitting them left `context_budget` permanently `None` (so every
    // context strategy was dead and tool-schema deferral guessed at the
    // window) and dropped `temperature`/`max_tokens` before they ever
    // reached a request.
    .with_generation_settings(agent_config.temperature, agent_config.max_tokens)
    .with_context_settings(
        agent_config.context_budget,
        agent_config.context_strategy.clone(),
        agent_config.context_window,
    );
    let handle = match modes.clone() {
        Some(registry) => handle.with_modes(registry),
        None => handle,
    };

    info!(
        provider = %agent_config.provider,
        model = %agent_config.model,
        "Agent created successfully"
    );

    Ok(Box::new(handle))
}

#[cfg(test)]
mod tests;
