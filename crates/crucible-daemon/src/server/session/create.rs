use super::super::*;
use crate::optional_param;

use super::spawn_setup_task;
use crucible_core::config::McpConfig;
use crucible_core::session::SessionType;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_session_create(
    req: Request,
    sm: &Arc<SessionManager>,
    pm: &Arc<ProjectManager>,
    data_home: &std::path::Path,
    llm_config: &Option<LlmConfig>,
    km: &Arc<KilnManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    am: &Arc<AgentManager>,
    mcp_config: Option<&McpConfig>,
) -> Response {
    let session_type_str = optional_param!(req, "type", as_str).unwrap_or("chat");
    let session_type: SessionType = match session_type_str.parse() {
        Ok(st) => st,
        Err(_) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid session type: {}", session_type_str),
            );
        }
    };

    // Kiln-less create falls back to the server's resolved data root, not the
    // process-global crucible_home() — in production they're the same path,
    // but tests inject an isolated data_home that must win here too.
    let kiln = optional_param!(req, "kiln", as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| data_home.to_path_buf());

    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);

    let provider_trust_level = resolve_provider_trust_level_for_create(&req, llm_config);
    let classification = resolve_kiln_classification_for_create(&kiln, workspace.as_ref());
    if let Some(classification) = classification {
        if let Err(message) = validate_trust_level(provider_trust_level, classification) {
            return Response::error(req.id, INVALID_PARAMS, message);
        }
    }

    let connected_kilns: Vec<PathBuf> = req
        .params
        .get("connect_kilns")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default();

    let recording_mode = optional_param!(req, "recording_mode", as_str)
        .and_then(|s| s.parse::<RecordingMode>().ok());
    let custom_recording_path = optional_param!(req, "recording_path", as_str).map(PathBuf::from);

    // Read locally — drives ACP vs internal branching in the setup task
    // below. `resolve_provider_trust_level_for_create` above already reads
    // this field for trust resolution.
    let agent_type = optional_param!(req, "agent_type", as_str)
        .unwrap_or("internal")
        .to_string();

    // Resolve the agent BEFORE creating the session. `configure_agent` is the
    // caller's opt-in to have the daemon own default-agent resolution (ACP
    // profile or config-derived internal defaults) instead of each client
    // building its own copy. Absent/false ⇒ today's behavior exactly: the
    // session is created agent-less and configured later via
    // `session.configure_agent`. Resolving first means an unknown ACP profile
    // (or an unparseable provider override) fails without orphaning a session.
    let configure_agent = optional_param!(req, "configure_agent", as_bool).unwrap_or(false);
    let resolved_agent = if configure_agent {
        match resolve_create_agent(
            &req,
            &agent_type,
            am,
            llm_config,
            mcp_config,
            workspace.as_deref().unwrap_or(&kiln),
            &kiln,
        ) {
            Ok(agent) => Some(agent),
            Err(message) => return Response::error(req.id, INVALID_PARAMS, message),
        }
    } else {
        None
    };

    // Only a real workspace registers as a project. Falling back to the
    // kiln here used to register kiln/config dirs (e.g. ~/.crucible) as
    // "projects" — a kiln is where knowledge goes, not where work happens.
    if let Some(project_path) = workspace.as_ref() {
        if let Err(e) = pm.register_if_missing(project_path) {
            tracing::warn!(path = %project_path.display(), error = %e, "Failed to auto-register project");
        }
    }

    match sm
        .create_session(
            session_type,
            kiln,
            workspace,
            connected_kilns,
            recording_mode,
        )
        .await
    {
        Ok(mut session) => {
            // Configure the resolved agent as part of create so the session is
            // usable immediately (no follow-up `session.configure_agent`
            // round-trip) and the setup task's `session_initialized` event can
            // carry the real model/endpoint. Mutating the local `session` here
            // mirrors what `configure_agent` persists to the manager.
            if let Some(agent) = resolved_agent {
                if let Err(e) = am.configure_agent(&session.id, agent.clone()).await {
                    return internal_error(req.id, e);
                }
                session.agent = Some(agent);
            }

            // Open the kiln in KilnManager so it's discoverable by session.list()
            if let Err(e) = km.open(&session.kiln).await {
                tracing::warn!(kiln = %session.kiln.display(), error = %e, "Failed to open kiln in manager");
            }

            if session.recording_mode == Some(RecordingMode::Granular) {
                let recording_path = match custom_recording_path {
                    Some(ref p) => p.clone(),
                    None => {
                        let session_dir = FileSessionStorage::session_dir_for(&session);
                        session_dir.join("recording.jsonl")
                    }
                };
                let (writer, tx) = RecordingWriter::new(
                    recording_path,
                    session.id.clone(),
                    RecordingMode::Granular,
                    None,
                );
                sm.set_recording_sender(&session.id, tx);
                let _handle = writer.start();
            }

            // Spawn the setup task. Must not be awaited here — the session
            // must be usable the moment `session.create` returns, even while
            // the task is still indexing / listing providers in the
            // background. Any failures inside the task are logged but never
            // reach the caller.
            spawn_setup_task(
                &session,
                agent_type,
                event_tx.clone(),
                am.clone(),
                mcp_config.cloned(),
            );

            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "type": session.session_type.as_prefix(),
                    "kiln": session.kiln,
                    "workspace": session.workspace,
                    "state": format!("{}", session.state),
                    // Present only when the daemon configured the agent as part
                    // of create; lets callers render the model without a
                    // separate session.get. Null/absent otherwise.
                    "agent_model": session.agent.as_ref().map(|a| a.model.clone()),
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// Resolve the [`SessionAgent`] to configure at create time from the request's
/// agent spec.
///
/// ACP profiles are looked up in the same table `agents.resolve_profile` uses
/// (`AgentManager::build_available_agents`); an unknown name is an `Err`, which
/// the caller turns into `INVALID_PARAMS` — the session is never created, so an
/// unknown agent can't orphan an agent-less row. Internal agents get
/// config-derived defaults (see [`build_default_internal_agent`]).
#[allow(clippy::too_many_arguments)]
fn resolve_create_agent(
    req: &Request,
    agent_type: &str,
    am: &Arc<AgentManager>,
    llm_config: &Option<LlmConfig>,
    mcp_config: Option<&McpConfig>,
    workspace: &std::path::Path,
    kiln: &std::path::Path,
) -> Result<crucible_core::session::SessionAgent, String> {
    if agent_type == "acp" {
        let name = optional_param!(req, "agent_name", as_str).unwrap_or("");
        if name.is_empty() {
            return Err("agent_name is required when agent_type is \"acp\"".to_string());
        }
        let profiles = am.build_available_agents();
        match profiles.get(name) {
            Some(profile) => Ok(crucible_core::session::SessionAgent::from_profile(
                profile, name,
            )),
            None => Err(format!("Unknown ACP agent profile: {name}")),
        }
    } else {
        let base = build_default_internal_agent(req, llm_config, mcp_config)?;
        // An internal `agent_name` selects an agent card (specialized
        // internal agent): card prompt/model/tools layered over the
        // config-derived defaults. Unknown card = error before the session
        // exists, mirroring the ACP branch.
        match optional_param!(req, "agent_name", as_str) {
            Some(name) if !name.is_empty() => {
                let cards = crate::agent_cards::discover_agent_cards(workspace, Some(kiln));
                match cards.get(name) {
                    Some(card) => Ok(crucible_core::session::SessionAgent::from_card(
                        card,
                        &base,
                        llm_config.as_ref().map(|c| &c.models),
                    )),
                    None => {
                        let mut names: Vec<_> = cards.keys().cloned().collect();
                        names.sort();
                        Err(format!(
                            "Unknown agent card: {name}. Available cards: {}",
                            if names.is_empty() {
                                "(none)".to_string()
                            } else {
                                names.join(", ")
                            }
                        ))
                    }
                }
            }
            _ => Ok(base),
        }
    }
}

/// Config-derived internal-agent defaults — the daemon-side equivalent of
/// `SessionAgent::internal_from_config` — with any caller-supplied
/// provider/provider_key/model/endpoint overrides applied on top.
///
/// Base temperature/max_tokens/MCP servers/precognition always come from the
/// daemon's own config so web sessions match CLI sessions. Only when the
/// provider itself is defaulted does the agent inherit the config default's
/// endpoint/key; an explicit provider override must not silently borrow the
/// default provider's endpoint.
fn build_default_internal_agent(
    req: &Request,
    llm_config: &Option<LlmConfig>,
    mcp_config: Option<&McpConfig>,
) -> Result<crucible_core::session::SessionAgent, String> {
    use crucible_core::config::BackendType;

    let default = llm_config.as_ref().and_then(|c| c.default_provider());
    let (def_provider, def_model, def_key, def_endpoint, def_temperature, def_max_tokens) =
        match default {
            Some((key, p)) => (
                p.provider_type,
                p.model(),
                key.clone(),
                Some(p.endpoint()),
                Some(p.temperature() as f64),
                Some(p.max_tokens()),
            ),
            None => (
                BackendType::Ollama,
                crucible_core::config::DEFAULT_CHAT_MODEL.to_string(),
                BackendType::Ollama.as_str().to_string(),
                None,
                None,
                None,
            ),
        };

    let req_provider = optional_param!(req, "provider", as_str);
    let req_provider_key = optional_param!(req, "provider_key", as_str).map(str::to_string);
    let req_model = optional_param!(req, "model", as_str).map(str::to_string);
    let req_endpoint = optional_param!(req, "endpoint", as_str).map(str::to_string);

    let provider_defaulted = req_provider.is_none();
    let provider = match req_provider {
        Some(p) => p
            .parse::<BackendType>()
            .map_err(|e| format!("Invalid provider: {e}"))?,
        None => def_provider,
    };
    let model = req_model.unwrap_or(def_model);
    let (endpoint, provider_key) = if provider_defaulted {
        (
            req_endpoint.or(def_endpoint),
            req_provider_key.unwrap_or(def_key),
        )
    } else {
        (
            req_endpoint,
            req_provider_key.unwrap_or_else(|| provider.as_str().to_string()),
        )
    };

    let mcp_servers = mcp_config
        .map(|mcp| mcp.servers.iter().map(|s| s.name.clone()).collect())
        .unwrap_or_default();

    Ok(crucible_core::session::SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some(provider_key),
        provider,
        model,
        system_prompt: String::new(),
        temperature: def_temperature,
        max_tokens: def_max_tokens,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint,
        env_overrides: std::collections::HashMap::new(),
        mcp_servers,
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: Default::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
        mode: None,
    })
}

pub(crate) fn validate_trust_level(
    provider_trust_level: TrustLevel,
    classification: DataClassification,
) -> Result<(), String> {
    if provider_trust_level.satisfies(classification) {
        return Ok(());
    }

    Err(format!(
        "Provider trust level '{}' is insufficient for kiln data classification '{}'. Requires '{}' trust or higher.",
        provider_trust_level,
        classification,
        classification.required_trust_level()
    ))
}

pub(crate) fn resolve_provider_trust_level_for_create(
    req: &Request,
    llm_config: &Option<LlmConfig>,
) -> TrustLevel {
    if optional_param!(req, "agent_type", as_str) == Some("acp") {
        return TrustLevel::Cloud;
    }

    if let Some(provider_key) = optional_param!(req, "provider_key", as_str) {
        if let Some(config) = llm_config
            .as_ref()
            .and_then(|cfg| cfg.get_provider(provider_key))
        {
            return config.effective_trust_level();
        }
    }

    if let Some(provider_name) = optional_param!(req, "provider", as_str) {
        if let Ok(backend) = provider_name.parse::<crucible_core::config::BackendType>() {
            return backend.default_trust_level();
        }
    }

    llm_config
        .as_ref()
        .and_then(LlmConfig::default_provider)
        .map(|(_, provider)| provider.effective_trust_level())
        .unwrap_or(TrustLevel::Cloud)
}

pub(crate) fn resolve_kiln_classification_for_create(
    kiln: &Path,
    workspace: Option<&PathBuf>,
) -> Option<DataClassification> {
    let workspace_path = workspace.cloned().unwrap_or_else(|| kiln.to_path_buf());
    crate::trust_resolution::resolve_kiln_classification(&workspace_path, kiln)
}
