use super::super::*;
use crate::rpc::RpcContext;
use crate::rpc_helpers::typed_params;
use crucible_core::protocol::requests::SessionCreateRequest;

use super::spawn_setup_task;
use crate::kiln_registry::refuse_forbidden_scope;
use crucible_core::config::{KilnName, McpConfig};
use crucible_core::session::{Session, SessionSummary, SessionType};

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
    let params = match typed_params::<SessionCreateRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    match ctx.create_session_resolved(&params).await {
        Ok(session) => {
            // The full summary: the record exists by the time this handler
            // answers, so every required field — `started_at`, `event_count`
            // (0, nothing has run yet), `archived` (false) — has a real
            // value, not a placeholder.
            let summary = SessionSummary::from(&session);
            let reply = serde_json::to_value(summary).expect("a session summary serializes");
            Response::success(req.id, reply)
        }
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
    /// **`SessionLifecycle::enforce_session_start` is not here.** Both callers
    /// run it after this returns: `RpcDispatcher::handle_session_create` and
    /// the plugin bridge's `create_session`. The RPC caller emits
    /// `session:created` only after the checks pass, so the checks stay with
    /// the callers. A plugin create from Lua that holds the plugin runtime
    /// (a session hook, `lua.eval`) gets a refusal from the checks, not a
    /// deadlock on the plugin-loader mutex.
    pub(crate) async fn create_session_resolved(
        &self,
        params: &SessionCreateRequest,
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

        // Names, resolved against the registry, and nothing else. A caller can
        // no longer name a *directory* here: the floor that used to run at this
        // site now runs once, at registration, and a name that no entry claims
        // is refused outright rather than becoming an attached kiln that
        // resolves to nothing. That distinction is the whole point — an absent
        // resolution gets read as "unconstrained" by whichever consumer
        // forgets, and this handler is where "said something unresolvable" is
        // separated from "said nothing".
        //
        // Strict where the hand-plucked version was lenient: it `filter_map`ped
        // non-string elements away, so `["/a", 7]` silently connected one kiln.
        // Deserializing `Option<Vec<String>>` makes that INVALID_PARAMS instead.
        // Deduped here so the set the trust gate walks is the set that gets
        // persisted.
        let mut kilns: Vec<KilnName> = Vec::new();
        for raw in params.kilns.clone().unwrap_or_default() {
            let name = KilnName::parse(&raw)
                .map_err(|e| SessionCreateError::Invalid(unknown_kiln_message(&raw, e.reason)))?;
            if self.kiln_registry.resolve(&name).registered().is_none() {
                return Err(SessionCreateError::Invalid(unknown_kiln_message(
                    &raw,
                    "no `[kilns]` entry is registered under it",
                )));
            }
            if !kilns.contains(&name) {
                kilns.push(name);
            }
        }

        let workspace = params.workspace.as_deref().map(PathBuf::from);

        // The workspace is still a path — workspaces have no registry and no
        // name authority to resolve against — so it runs the floor here. This
        // is the only door: a session's workspace is fixed at creation, and
        // `session.set_workspace` refuses every change. The socket has no
        // auth, so create must not be a cheap door.
        let sessions_root = self.sessions.sessions_root().to_path_buf();
        if let Some(workspace) = workspace.as_deref() {
            refuse_forbidden_scope("workspace", workspace, &sessions_root)
                .map_err(SessionCreateError::Invalid)?;
        }
        // The directories those names reach, for the consumers that need one:
        // the trust gate's classification walk, and the agent-card discovery
        // below.
        let kiln_paths = self.kiln_registry.paths_for(&kilns);

        // Kiln-less create yields a genuinely EMPTY set. It used to fall back
        // to the daemon's data root, which is the PARENT of the sessions root
        // — so every kiln-less session carried an allowed root enclosing every
        // transcript the daemon had ever written, and `grep` walked straight
        // into it. Zero kilns is a legitimate state: a tools-only agent with
        // no corpus. It degrades capabilities (no note/kiln tools, no
        // precognition, no semantic search — see `CrucibleMcpServer::list_tools`)
        // and must never degrade containment.
        //
        // The workspace fallback below still wants exactly one kiln, and has
        // to cope with there being none; see `Session::default_kiln`.
        // Agent-card discovery reads every kiln in `kiln_paths`.
        let default_kiln = kiln_paths.first().cloned();

        // Forwarded untouched: `false`, a profile name and an environment
        // object are the isolating plugin's vocabulary, not the daemon's.
        // `Option<Value>` rather than a parsed type so a shape the daemon has
        // never heard of still reaches the plugin that defined it. Absent stays
        // absent — see `Session::isolation`; `false` and absent are different
        // instructions. serde already maps JSON `null` to `None` for an
        // `Option`, so the explicit null filter the hand-plucked version needed
        // is gone with it.
        let isolation = params.isolation.clone();

        let recording_mode = params
            .recording_mode
            .as_deref()
            .and_then(|s| s.parse::<RecordingMode>().ok());
        let custom_recording_path = params.recording_path.as_deref().map(PathBuf::from);

        // Read locally — drives ACP vs internal branching in the setup task
        // below, and the agent the trust gate checks.
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
                    &kiln_paths,
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
            if let Some(prompt) = &params.system_prompt {
                agent.system_prompt.clone_from(prompt);
            }
            if let Some(servers) = &params.mcp_servers {
                agent.mcp_servers.clone_from(servers);
            }
            // `configure_agent` below runs the endpoint check too; here it
            // refuses before a session exists, so a refusal leaves no row behind.
            self.agents
                .refuse_internal_endpoint(&agent)
                .await
                .map_err(|e| SessionCreateError::Invalid(e.to_string()))?;
            Some(agent)
        } else {
            None
        };

        // The trust gate, against every kiln this session is about to hold,
        // before `create_session` persists anything. A refusal after the save
        // left an agent-less row that answered `NoAgentConfigured` for good.
        //
        // The gate checks an agent, not the request fields, so create reads
        // the provider key through the same rule as every later gate. With
        // `configure_agent`, that is the resolved agent, which a card can
        // move onto another provider. Without it, it is the agent that the
        // request describes; `session.configure_agent` later checks the
        // agent that actually arrives.
        let described_agent;
        let gate_agent = match &resolved_agent {
            Some(agent) => agent,
            None => {
                let mut agent = build_default_internal_agent(
                    params,
                    &self.llm_config.get().map(|c| (*c).clone()),
                    self.mcp_config.as_ref(),
                )
                .map_err(SessionCreateError::Invalid)?;
                agent.agent_type.clone_from(&agent_type);
                described_agent = agent;
                &described_agent
            }
        };
        self.agents
            .refuse_untrusted(Some(gate_agent), &kiln_paths, workspace.as_deref())
            .map_err(|e| SessionCreateError::Invalid(e.to_string()))?;

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
        // Stored on every type. The reflection pass reads it to tell a session
        // that a plugin created from a user's own. A proposal names the plugin
        // as its author only on a plugin session (`author_of`).
        let plugin = params.plugin.clone();
        if isolation.is_some() || plugin.is_some() {
            session.isolation = isolation.clone();
            session.plugin = plugin.clone();
            self.sessions
                .modify_session(&session.id, |live| {
                    live.isolation = isolation;
                    live.plugin = plugin;
                    true
                })
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
        // session.list(). A `lazy` entry is opened here too: lazy means "do not
        // open this unasked", and a caller that named it in `kilns` has asked.
        for kiln in &kiln_paths {
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
                session.id.to_string(),
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
    /// and the source roots come off `self` instead of seven positional arguments.
    fn resolve_create_agent(
        &self,
        params: &SessionCreateRequest,
        agent_type: &str,
        workspace: &std::path::Path,
        kilns: &[PathBuf],
    ) -> Result<crucible_core::session::SessionAgent, String> {
        if agent_type == "acp" {
            let name = params.agent_name.as_deref().unwrap_or("");
            if name.is_empty() {
                return Err("agent_name is required when agent_type is \"acp\"".to_string());
            }
            let profiles = self.agents.build_available_agents();
            match profiles.get(name) {
                Some(profile) => {
                    let mut agent =
                        crucible_core::session::SessionAgent::from_profile(profile, name);
                    agent.env_overrides.extend(params.env_overrides.clone());
                    Ok(agent)
                }
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
                            self.agents.source_roots(),
                            workspace,
                            kilns,
                        )
                        .into_keys()
                    ),
                )),
            }
        } else {
            let base = build_default_internal_agent(
                params,
                &self.llm_config.get().map(|c| (*c).clone()),
                self.mcp_config.as_ref(),
            )?;
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
                self.agents.source_roots(),
                workspace,
                kilns,
            );
            match crate::agent_cards::resolve_card(&cards, name)? {
                Some(card) => {
                    let mut agent = crucible_core::session::SessionAgent::from_card(
                        card,
                        &base,
                        self.llm_config
                            .get()
                            .map(|c| c.models.clone().into_iter().collect())
                            .as_ref(),
                    );
                    agent.agent_card_name = Some(name.to_string());
                    Ok(agent)
                }
                None => Err(format!(
                    "Unknown agent card: {name}. Available cards: {}",
                    name_list(cards.into_keys())
                )),
            }
        }
    }
}

/// Why a `kilns` element is not a kiln, said the same way for both halves of
/// the rule: a string that is not a name at all, and a name no entry claims.
///
/// Both name the value the caller supplied — which is safe to echo back to that
/// caller, because they wrote it — and both name the fix, because "unknown
/// kiln" with nothing else in it leaves the caller guessing whether they typed
/// it wrong or never registered it.
fn unknown_kiln_message(raw: &str, reason: &str) -> String {
    format!(
        "Unknown kiln {raw:?}: {reason}. Kilns are addressed by the name of their \
         `[kilns]` entry, not by path — register one with `cru kiln register <name> <path>`."
    )
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

/// Config-derived internal-agent defaults — `SessionAgent::internal_defaults`
/// — with any caller-supplied provider/provider_key/model/endpoint overrides
/// applied on top.
///
/// Base temperature/max_tokens/MCP servers/precognition always come from the
/// daemon's own config so web sessions match CLI sessions. Only when the
/// provider itself is defaulted does the agent inherit the config default's
/// endpoint/key; an explicit provider override must not silently borrow the
/// default provider's endpoint.
fn build_default_internal_agent(
    params: &SessionCreateRequest,
    llm_config: &Option<LlmConfig>,
    mcp_config: Option<&McpConfig>,
) -> Result<crucible_core::session::SessionAgent, String> {
    use crucible_core::config::BackendType;

    let mut agent =
        crucible_core::session::SessionAgent::internal_defaults(llm_config.as_ref(), mcp_config);

    match params.provider.as_deref() {
        None => {
            if let Some(key) = params.provider_key.as_deref() {
                let provider = llm_config
                    .as_ref()
                    .and_then(|config| config.get_provider(key))
                    .ok_or_else(|| format!("Unknown provider key: {key}"))?;
                agent.provider = provider.provider_type;
                agent.provider_key = Some(key.to_string());
                agent.model = provider.model();
                agent.endpoint = Some(provider.endpoint());
            }
        }
        Some(p) => {
            agent.provider = p
                .parse::<BackendType>()
                .map_err(|e| format!("Invalid provider: {e}"))?;
            agent.endpoint = None;
            agent.provider_key = Some(
                params
                    .provider_key
                    .clone()
                    .unwrap_or_else(|| agent.provider.as_str().to_string()),
            );
        }
    }
    if let Some(model) = &params.model {
        agent.model.clone_from(model);
    }
    if let Some(endpoint) = &params.endpoint {
        agent.endpoint = Some(endpoint.clone());
    }

    Ok(agent)
}

#[cfg(test)]
mod tests {
    use super::build_default_internal_agent;
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
    use crucible_core::protocol::requests::SessionCreateRequest;

    /// A config whose default provider is `local`, an Ollama at a custom
    /// endpoint with its own model.
    fn llm_with_default() -> Option<LlmConfig> {
        let provider = LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint("http://ollama.test:11434")
            .model("config-model")
            .build();
        Some(LlmConfig {
            default: Some("local".to_string()),
            providers: [
                ("local".to_string(), provider),
                (
                    "named".to_string(),
                    LlmProviderConfig::builder(BackendType::OpenAI)
                        .endpoint("http://named.test")
                        .model("named-model")
                        .build(),
                ),
            ]
            .into_iter()
            .collect(),
            models: Default::default(),
        })
    }

    fn build(params: SessionCreateRequest) -> crucible_core::session::SessionAgent {
        build_default_internal_agent(&params, &llm_with_default(), None).unwrap()
    }

    #[tokio::test]
    async fn acp_create_merges_profile_environment_and_preserves_delegation() {
        use crate::agent_manager::{AgentManager, AgentManagerParams};
        use crucible_core::config::{AcpConfig, AgentProfile, DelegationConfig};
        use std::sync::Arc;

        let tmp = tempfile::tempdir().unwrap();
        let sm = crate::test_support::temp_session_manager();
        let km = Arc::new(crate::kiln_manager::KilnManager::new());
        let (event_tx, _) = crate::EventBus::channel(64);
        let delegation = DelegationConfig {
            enabled: true,
            max_depth: 2,
            allowed_targets: Some(vec!["researcher".into()]),
            result_max_bytes: 102400,
            max_concurrent_delegations: 4,
            timeout_secs: 300,
        };
        let profile = AgentProfile {
            command: Some("fixture-acp".into()),
            env: [
                ("KEEP".into(), "profile".into()),
                ("OPENCODE_MODEL".into(), "old".into()),
            ]
            .into(),
            delegation: Some(delegation.clone()),
            ..Default::default()
        };
        let am = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: km.clone(),
            session_manager: sm.clone(),
            background_manager: Arc::new(crate::background_manager::BackgroundJobManager::new(
                event_tx.clone(),
            )),
            mcp_gateway: None,
            llm_config: None,
            acp_config: Some(AcpConfig {
                agents: [("fixture".into(), profile)].into(),
                ..Default::default()
            }),
            context_config: None,
            permission_config: None,
            plugin_loader: None,
            source_roots: Default::default(),
            review_snapshot_root: crate::test_support::scratch_snapshot_root(),
        }));
        let mut ctx = crate::rpc::RpcContext::for_test(
            km,
            sm.clone(),
            am,
            Arc::new(crate::project_manager::ProjectManager::new(
                tmp.path().join("projects.json"),
            )),
            event_tx,
            tmp.path().to_path_buf(),
        );
        let params = serde_json::from_value(serde_json::json!({
            "type": "chat", "agent_type": "acp", "agent_name": "fixture", "configure_agent": true,
            "env_overrides": { "OPENCODE_MODEL": "chosen", "EXPLICIT": "yes" }
        }))
        .unwrap();
        let session = ctx.create_session_resolved(&params).await.unwrap();
        let agent = sm.get_session(&session.id).unwrap().agent.unwrap();
        // The profile's own `env` stays in the config. `acp_launch` reads it
        // there at every spawn, under the session's overrides.
        assert!(!agent.env_overrides.contains_key("KEEP"));
        assert_eq!(
            agent
                .env_overrides
                .get("OPENCODE_MODEL")
                .map(String::as_str),
            Some("chosen")
        );
        assert_eq!(
            agent.env_overrides.get("EXPLICIT").map(String::as_str),
            Some("yes")
        );
        assert_eq!(agent.delegation_config, Some(delegation));

        ctx.mcp_config = Some(
            serde_json::from_value(serde_json::json!({"servers": [{
                "name": "configured", "prefix": "fixture_",
                "transport": {"type": "stdio", "command": "never-launched"}
            }]}))
            .unwrap(),
        );
        for explicit in [false, true] {
            let params = SessionCreateRequest {
                session_type: "chat".into(),
                configure_agent: true,
                system_prompt: explicit.then(|| "Explicit reviewer instructions".into()),
                mcp_servers: explicit.then(Vec::new),
                ..Default::default()
            };
            let session = ctx.create_session_resolved(&params).await.unwrap();
            let agent = sm.get_session(&session.id).unwrap().agent.unwrap();
            assert_eq!(
                agent.mcp_servers,
                if explicit {
                    vec![]
                } else {
                    vec!["configured".to_string()]
                }
            );
            if explicit {
                assert_eq!(agent.system_prompt, "Explicit reviewer instructions");
            }
        }
    }

    /// A session whose workspace is a kiln root still gets created, and the
    /// kiln does not become a project on the way: the rail files it in the
    /// project-less bucket. This is `cru --standalone web` run inside a kiln,
    /// which used to leave a project named after the kiln in `projects.json`.
    #[tokio::test]
    async fn a_kiln_root_workspace_creates_the_session_but_no_project() {
        use crate::agent_manager::{AgentManager, AgentManagerParams};
        use std::sync::Arc;

        let tmp = tempfile::tempdir().unwrap();
        let notes = tmp.path().join("notes");
        std::fs::create_dir(&notes).unwrap();
        let sm = crate::test_support::temp_session_manager_with_kilns(&[("notes", &notes)]);
        let km = Arc::new(crate::kiln_manager::KilnManager::new());
        let (event_tx, _) = crate::EventBus::channel(64);
        let am = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: km.clone(),
            session_manager: sm.clone(),
            background_manager: Arc::new(crate::background_manager::BackgroundJobManager::new(
                event_tx.clone(),
            )),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: None,
            source_roots: Default::default(),
            review_snapshot_root: crate::test_support::scratch_snapshot_root(),
        }));
        let pm = Arc::new(
            crate::project_manager::ProjectManager::new(tmp.path().join("projects.json"))
                .with_kiln_registry(sm.kiln_registry().clone()),
        );
        let ctx = crate::rpc::RpcContext::for_test(
            km,
            sm.clone(),
            am,
            pm.clone(),
            event_tx,
            tmp.path().to_path_buf(),
        );

        let params = SessionCreateRequest {
            session_type: "chat".into(),
            kilns: Some(vec!["notes".into()]),
            workspace: Some(notes.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let session = ctx.create_session_resolved(&params).await.unwrap();
        assert_eq!(session.workspace.as_deref(), Some(notes.as_path()));
        assert!(
            pm.list().is_empty(),
            "the kiln must not register itself as a project: {:?}",
            pm.list()
        );
    }

    /// No request fields: the config default provider supplies everything.
    #[test]
    fn an_empty_request_takes_the_config_default_provider() {
        let agent = build(SessionCreateRequest::default());
        assert_eq!(agent.provider, BackendType::Ollama);
        assert_eq!(agent.provider_key.as_deref(), Some("local"));
        assert_eq!(agent.model, "config-model");
        assert_eq!(agent.endpoint.as_deref(), Some("http://ollama.test:11434"));
    }

    /// A model alone replaces the model; the provider, key and endpoint stay.
    #[test]
    fn a_model_override_keeps_the_config_provider() {
        let agent = build(SessionCreateRequest {
            model: Some("request-model".to_string()),
            ..Default::default()
        });
        assert_eq!(agent.model, "request-model");
        assert_eq!(agent.provider_key.as_deref(), Some("local"));
        assert_eq!(agent.endpoint.as_deref(), Some("http://ollama.test:11434"));
    }

    /// A key selects the whole configured provider; explicit fields win last.
    #[test]
    fn endpoint_and_key_override_the_config_provider_fields() {
        let agent = build(SessionCreateRequest {
            endpoint: Some("http://other.test".to_string()),
            ..Default::default()
        });
        assert_eq!(agent.endpoint.as_deref(), Some("http://other.test"));
        assert_eq!(agent.provider_key.as_deref(), Some("local"));

        let agent = build(SessionCreateRequest {
            provider_key: Some("named".to_string()),
            ..Default::default()
        });
        assert_eq!(agent.provider_key.as_deref(), Some("named"));
        assert_eq!(agent.provider, BackendType::OpenAI);
        assert_eq!(agent.model, "named-model");
        assert_eq!(agent.endpoint.as_deref(), Some("http://named.test"));

        let agent = build(SessionCreateRequest {
            provider_key: Some("named".into()),
            model: Some("explicit-model".into()),
            endpoint: Some("http://explicit.test".into()),
            ..Default::default()
        });
        assert_eq!(agent.model, "explicit-model");
        assert_eq!(agent.endpoint.as_deref(), Some("http://explicit.test"));

        let err = build_default_internal_agent(
            &SessionCreateRequest {
                provider_key: Some("missing".into()),
                ..Default::default()
            },
            &llm_with_default(),
            None,
        )
        .unwrap_err();
        assert!(err.contains("Unknown provider key: missing"), "{err}");
    }

    /// An explicit provider does not borrow the config default's endpoint or
    /// key: the endpoint is the request's (or none), and the key falls back to
    /// the provider name.
    #[test]
    fn an_explicit_provider_drops_the_config_endpoint_and_key() {
        let agent = build(SessionCreateRequest {
            provider: Some("openai".to_string()),
            model: Some("gpt".to_string()),
            ..Default::default()
        });
        assert_eq!(agent.provider, BackendType::OpenAI);
        assert_eq!(agent.provider_key.as_deref(), Some("openai"));
        assert_eq!(agent.model, "gpt");
        assert_eq!(agent.endpoint, None);

        let agent = build(SessionCreateRequest {
            provider: Some("openai".to_string()),
            provider_key: Some("work".to_string()),
            endpoint: Some("http://proxy.test".to_string()),
            ..Default::default()
        });
        assert_eq!(agent.provider_key.as_deref(), Some("work"));
        assert_eq!(agent.endpoint.as_deref(), Some("http://proxy.test"));
    }

    /// An unknown provider name is the caller's error, not a silent default.
    #[test]
    fn an_unknown_provider_is_refused() {
        let err = build_default_internal_agent(
            &SessionCreateRequest {
                provider: Some("no-such-provider".to_string()),
                ..Default::default()
            },
            &llm_with_default(),
            None,
        )
        .unwrap_err();
        assert!(err.starts_with("Invalid provider"), "{err}");
    }
}
