use super::super::*;
use crate::rpc::RpcContext;
use crate::rpc_helpers::typed_params;

use super::scope::refuse_forbidden_scope;
use super::spawn_setup_task;
use crucible_core::config::McpConfig;
use crucible_core::session::{Session, SessionType};

/// Why a `session.create` failed, split by who can fix it.
///
/// Not `Result<_, String>`: `Invalid` becomes `INVALID_PARAMS` (-32602) and
/// `crucible-web` maps that code to HTTP 422 while everything else becomes a
/// 502 (`crucible-web/src/routes/session/mod.rs`). Collapsing the two would
/// report a caller's typo as a daemon fault.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SessionCreateError {
    /// Caller-fixable: unparseable params, a scope the daemon refuses, a trust
    /// level the kiln's classification does not permit, an unknown agent.
    #[error("{0}")]
    Invalid(String),
    /// The daemon failed at something it agreed to do.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub(crate) async fn handle_session_create(req: Request, ctx: &RpcContext) -> Response {
    // The client's own request type is the contract: it derives `Deserialize`,
    // it has wire-format tests (`rpc_client/client/mod.rs`), and it lives in
    // this crate — so there is no reason for the server to re-derive fourteen
    // field names by hand. It did, and the fourteen happened to agree; nothing
    // asserted that they would. (`LuaInitSessionRequest.config` is the same
    // shape and does NOT agree — the client serializes it, no handler reads it.)
    //
    // Unknown fields are tolerated on purpose (no `deny_unknown_fields`): a
    // newer client must be able to talk to an older daemon.
    let params = match typed_params::<crate::rpc_client::SessionCreateRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    match ctx.create_session_resolved(&params).await {
        Ok(session) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "type": session.session_type.as_prefix(),
                "kilns": session.kilns,
                "workspace": session.workspace,
                "state": format!("{}", session.state),
                // Present only when the daemon configured the agent as part
                // of create; lets callers render the model without a
                // separate session.get. Null/absent otherwise.
                "agent_model": session.agent.as_ref().map(|a| a.model.clone()),
            }),
        ),
        Err(SessionCreateError::Invalid(message)) => {
            Response::error(req.id, INVALID_PARAMS, message)
        }
        Err(SessionCreateError::Internal(e)) => internal_error(req.id, e),
    }
}

impl RpcContext {
    /// Create a session from already-typed params: everything `session.create`
    /// does between deserializing the request and projecting a JSON response.
    ///
    /// Returns the `Session` rather than JSON because its two callers disagree
    /// on the projection — RPC answers `session_id`, plugins read `session.id`.
    ///
    /// **`SessionLifecycle::enforce_session_start` is deliberately not here.**
    /// It stays at the RPC layer (`RpcDispatcher::handle_session_create`) until
    /// `fire_session_start`/`fire_session_end` stop holding `plugin_loader`'s
    /// mutex across their Lua call: the reflection plugin calls
    /// `cru.sessions.create` from inside `on_session_end`, and tokio's mutex is
    /// not reentrant, so a plugin-side create that reached the start hooks
    /// would deadlock the daemon.
    pub(crate) async fn create_session_resolved(
        &self,
        params: &crate::rpc_client::SessionCreateRequest,
    ) -> Result<Session, SessionCreateError> {
        // Contradictory agent selection, refused rather than resolved by
        // precedence: on an internal session the two fields mean the same
        // thing, and on an ACP session picking `agent_name` would silently
        // discard a card the caller asked for.
        if params.agent_card.is_some() && params.agent_name.is_some() {
            return Err(SessionCreateError::Invalid(
                "agent_card and agent_name are mutually exclusive; agent_name on an internal session is a deprecated alias for agent_card".to_string(),
            ));
        }

        let session_type: SessionType = params.session_type.parse().map_err(|_| {
            SessionCreateError::Invalid(format!("Invalid session type: {}", params.session_type))
        })?;

        // Strict where the hand-plucked version was lenient: it `filter_map`ped
        // non-string elements away, so `["/a", 7]` silently connected one kiln.
        // Deserializing `Option<Vec<String>>` makes that INVALID_PARAMS instead.
        // Deduped here so the set the trust gate walks is the set that gets
        // persisted.
        let kilns: Vec<PathBuf> = params
            .kilns
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .fold(Vec::new(), |mut acc, kiln| {
                if !acc.contains(&kiln) {
                    acc.push(kiln);
                }
                acc
            });

        let workspace = params.workspace.as_deref().map(PathBuf::from);

        // Before anything reads them: every caller-supplied directory that
        // becomes session scope goes through the same floor as
        // `session.connect_kiln`/`session.set_workspace` — the socket has no
        // auth, so create must not be the cheaper door. That floor now also
        // refuses anything at or under the sessions root, which is the same
        // hole through this door: an allowed root inside the denied sessions
        // root out-ranks the denial.
        let sessions_root = self.sessions.sessions_root().to_path_buf();
        let scopes = kilns
            .iter()
            .map(|k| ("kiln", k))
            .chain(workspace.iter().map(|w| ("workspace", w)));
        for (kind, path) in scopes {
            refuse_forbidden_scope(kind, path, &sessions_root)
                .map_err(SessionCreateError::Invalid)?;
        }

        // Kiln-less create yields a genuinely EMPTY set. It used to fall back
        // to the daemon's data root, which is the PARENT of the sessions root
        // — so every kiln-less session carried an allowed root enclosing every
        // transcript the daemon had ever written, and `grep` walked straight
        // into it. Zero kilns is a legitimate state: a tools-only agent with
        // no corpus. It degrades capabilities (no note/kiln tools, no
        // precognition, no semantic search — see `CrucibleMcpServer::list_tools`)
        // and must never degrade containment.
        //
        // Agent-card discovery and the ACP resolver still want exactly one
        // kiln, and now have to cope with there being none; see
        // `Session::default_kiln`.
        let default_kiln = kilns.first().cloned();

        // Forwarded untouched: `false`, a profile name and an environment
        // object are the isolating plugin's vocabulary, not the daemon's.
        // `Option<Value>` rather than a parsed type so a shape the daemon has
        // never heard of still reaches the plugin that defined it. Absent stays
        // absent — see `Session::isolation`; `false` and absent are different
        // instructions. serde already maps JSON `null` to `None` for an
        // `Option`, so the explicit null filter the hand-plucked version needed
        // is gone with it.
        let isolation = params.isolation.clone();

        let provider_trust_level =
            resolve_provider_trust_level_for_create(params, &self.llm_config);
        // Every kiln the session will hold, not only the one it was created
        // with. The set is flat, so no member is the one that gets classified
        // — and a confidential kiln arriving alongside the first used to reach
        // no create-time trust check at all.
        for kiln in &kilns {
            if let Some(classification) =
                resolve_kiln_classification_for_create(kiln, workspace.as_ref())
            {
                validate_trust_level(provider_trust_level, classification)
                    .map_err(SessionCreateError::Invalid)?;
            }
        }

        let recording_mode = params
            .recording_mode
            .as_deref()
            .and_then(|s| s.parse::<RecordingMode>().ok());
        let custom_recording_path = params.recording_path.as_deref().map(PathBuf::from);

        // Read locally — drives ACP vs internal branching in the setup task
        // below. `resolve_provider_trust_level_for_create` above already reads
        // this field for trust resolution.
        let agent_type = params
            .agent_type
            .clone()
            .unwrap_or_else(|| "internal".to_string());

        // Resolve the agent BEFORE creating the session. `configure_agent` is
        // the caller's opt-in to have the daemon own default-agent resolution
        // (ACP profile or config-derived internal defaults) instead of each
        // client building its own copy. Absent/false ⇒ today's behavior
        // exactly: the session is created agent-less and configured later via
        // `session.configure_agent`. Resolving first means an unknown ACP
        // profile (or an unparseable provider override) fails without orphaning
        // a session.
        let resolved_agent = if params.configure_agent {
            let mut agent = self
                .resolve_create_agent(
                    params,
                    &agent_type,
                    workspace
                        .as_deref()
                        .or(default_kiln.as_deref())
                        .unwrap_or(Path::new("")),
                    default_kiln.as_deref(),
                )
                .map_err(SessionCreateError::Invalid)?;
            // Last word, over the card's own `tools:`. A card is a global file
            // the operator wrote once; this policy is what the *caller* decided
            // for this one session — the Discord plugin's per-sender access
            // tier, say. A card that could widen it would turn "this sender
            // gets reads only" into whatever the card felt like.
            if let Some(policy) = params.tool_policy.clone() {
                agent.tool_policy = Some(policy);
            }
            // The resolved agent, against every kiln this session is about to
            // hold — checked HERE, before `create_session` persists anything.
            //
            // `configure_agent` runs the same gate below, and running it only
            // there was a bug: it fires after the session exists, so a refusal
            // left an agent-less row on disk and in `session.list` answering
            // `NoAgentConfigured` for good.
            //
            // One thing reaches this that `validate_trust_level` above cannot:
            // that gate reads the *request's* provider, while a card's
            // `provider:`/`specialty:` can override it — a local default
            // resolving through a card onto a cloud provider passed the first
            // gate on a provider it was no longer going to use.
            self.agents
                .refuse_untrusted_for_kilns(kilns.iter(), &agent)
                .map_err(|e| SessionCreateError::Invalid(e.to_string()))?;
            Some(agent)
        } else {
            None
        };

        // Only a real workspace registers as a project. Falling back to the
        // kiln here used to register kiln/config dirs (e.g. ~/.crucible) as
        // "projects" — a kiln is where knowledge goes, not where work happens.
        if let Some(project_path) = workspace.as_ref() {
            if let Err(e) = self.project_manager.register_if_missing(project_path) {
                tracing::warn!(path = %project_path.display(), error = %e, "Failed to auto-register project");
            }
        }

        let mut session = self
            .sessions
            .create_session(session_type, kilns, workspace, recording_mode)
            .await
            .map_err(|e| SessionCreateError::Internal(e.into()))?;

        // Persisted before anything else can observe the session: the plugin
        // start hooks that read it fire once the RPC handler returns
        // (`SessionLifecycle::enforce_session_start`), and a resume reads it
        // back off disk. A second write rather than a `create_session`
        // argument keeps the isolation opt-in out of ~90 unrelated call sites,
        // and only happens when the caller asked for it.
        if let Some(isolation) = isolation {
            session.isolation = Some(isolation);
            self.sessions
                .update_session(&session)
                .await
                .map_err(|e| SessionCreateError::Internal(e.into()))?;
        }

        // Configure the resolved agent as part of create so the session is
        // usable immediately (no follow-up `session.configure_agent`
        // round-trip) and the setup task's `session_initialized` event can
        // carry the real model/endpoint. Mutating the local `session` here
        // mirrors what `configure_agent` persists to the manager.
        if let Some(agent) = resolved_agent {
            // `InvalidConfig` is caller-fixable — it is how `configure_agent`
            // reports its trust gate, which sees the walked-up classification
            // and the connected kilns that the create-time gate above does not.
            // Reporting that as -32602 keeps the web's 422/502 split honest.
            self.agents
                .configure_agent(&session.id, agent.clone())
                .await
                .map_err(|e| match e {
                    crate::agent_manager::AgentError::InvalidConfig(message) => {
                        SessionCreateError::Invalid(message)
                    }
                    other => SessionCreateError::Internal(other.into()),
                })?;
            session.agent = Some(agent);
        }

        // Open every kiln in KilnManager so they're discoverable by
        // session.list()
        for kiln in &session.kilns {
            if let Err(e) = self.kiln.open(kiln).await {
                tracing::warn!(kiln = %kiln.display(), error = %e, "Failed to open kiln in manager");
            }
        }

        if session.recording_mode == Some(RecordingMode::Granular) {
            let recording_path = match custom_recording_path {
                Some(ref p) => p.clone(),
                None => self
                    .sessions
                    .session_dir(&session.id)
                    .join("recording.jsonl"),
            };
            let (writer, tx) = RecordingWriter::new(
                recording_path,
                session.id.clone(),
                RecordingMode::Granular,
                None,
            );
            self.sessions.set_recording_sender(&session.id, tx);
            let _handle = writer.start();
        }

        // Spawn the setup task. Must not be awaited here — the session must be
        // usable the moment `session.create` returns, even while the task is
        // still indexing / listing providers in the background. Any failures
        // inside the task are logged but never reach the caller.
        spawn_setup_task(
            &session,
            agent_type,
            self.event_tx.clone(),
            self.agents.clone(),
            self.mcp_config.clone(),
        );

        Ok(session)
    }

    /// Resolve the [`SessionAgent`] to configure at create time from the
    /// request's agent spec.
    ///
    /// ACP profiles are looked up in the same table `agents.resolve_profile`
    /// uses (`AgentManager::build_available_agents`); an unknown name is an
    /// `Err`, which the caller turns into `INVALID_PARAMS` — the session is
    /// never created, so an unknown agent can't orphan an agent-less row.
    /// Internal agents get config-derived defaults (see
    /// [`build_default_internal_agent`]), optionally layered with an agent card.
    ///
    /// A method rather than a free function so the managers, the LLM/MCP config
    /// and `config_home` come off `self` instead of seven positional arguments.
    fn resolve_create_agent(
        &self,
        params: &crate::rpc_client::SessionCreateRequest,
        agent_type: &str,
        workspace: &std::path::Path,
        kiln: Option<&std::path::Path>,
    ) -> Result<crucible_core::session::SessionAgent, String> {
        if agent_type == "acp" {
            let name = params.agent_name.as_deref().unwrap_or("");
            if name.is_empty() {
                return Err("agent_name is required when agent_type is \"acp\"".to_string());
            }
            let profiles = self.agents.build_available_agents();
            match profiles.get(name) {
                Some(profile) => Ok(crucible_core::session::SessionAgent::from_profile(
                    profile, name,
                )),
                // Cards are listed too, and not out of generosity: a card name
                // sent here is the likeliest cause, because until `agent_card`
                // existed a card name had no other field to travel in. Naming
                // the field that does resolve it is the whole diagnostic.
                None => Err(format!(
                    "Unknown ACP agent profile: {name}. Available profiles: {}. \
                     Agent cards (select with agent_card, not agent_name): {}",
                    name_list(profiles.keys().cloned()),
                    name_list(
                        crate::agent_cards::discover_agent_cards_in(
                            self.config_home.as_deref(),
                            workspace,
                            kiln,
                        )
                        .into_keys()
                    ),
                )),
            }
        } else {
            let base =
                build_default_internal_agent(params, &self.llm_config, self.mcp_config.as_ref())?;
            // An agent card (specialized internal agent): card
            // prompt/model/tools layered over the config-derived defaults.
            // Unknown card = error before the session exists, mirroring the ACP
            // branch. `agent_name` is the deprecated alias — see the field doc;
            // both-set was already refused in `create_session_resolved`.
            let card_name = params
                .agent_card
                .as_deref()
                .or(params.agent_name.as_deref())
                .filter(|name| !name.is_empty());
            let Some(name) = card_name else {
                return Ok(base);
            };
            let cards = crate::agent_cards::discover_agent_cards_in(
                self.config_home.as_deref(),
                workspace,
                kiln,
            );
            match cards.get(name) {
                Some(card) => Ok(crucible_core::session::SessionAgent::from_card(
                    card,
                    &base,
                    self.llm_config.as_ref().map(|c| &c.models),
                )),
                None => Err(format!(
                    "Unknown agent card: {name}. Available cards: {}",
                    name_list(cards.into_keys())
                )),
            }
        }
    }
}

/// Sorted, comma-joined names for a "did you mean" list; `(none)` when empty,
/// because an empty list reads as a truncated message.
fn name_list(names: impl Iterator<Item = String>) -> String {
    let mut names: Vec<_> = names.collect();
    names.sort();
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.join(", ")
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
    params: &crate::rpc_client::SessionCreateRequest,
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

    let req_provider = params.provider.as_deref();
    let req_provider_key = params.provider_key.clone();
    let req_model = params.model.clone();
    let req_endpoint = params.endpoint.clone();

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
    params: &crate::rpc_client::SessionCreateRequest,
    llm_config: &Option<LlmConfig>,
) -> TrustLevel {
    if params.agent_type.as_deref() == Some("acp") {
        return TrustLevel::Cloud;
    }

    if let Some(provider_key) = params.provider_key.as_deref() {
        if let Some(config) = llm_config
            .as_ref()
            .and_then(|cfg| cfg.get_provider(provider_key))
        {
            return config.effective_trust_level();
        }
    }

    if let Some(provider_name) = params.provider.as_deref() {
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
