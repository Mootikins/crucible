use super::*;
use crate::empty_providers::EmptyEmbeddingProvider;
use crate::rpc_helpers::typed_params;
use crucible_core::enrichment::EmbeddingProvider;

/// One skill in a `skills.list` or `skills.search` answer.
///
/// The two RPCs answer the same row, because a list and a search are the same
/// question asked of two different sets. A second row type here would let one
/// of them grow a field the other cannot report.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkillSummary {
    /// The skill's name, which is also the key `skills.get` takes.
    pub name: String,
    /// The discovery scope the skill came from, as `SkillScope` spells it.
    pub scope: String,
    pub description: String,
    /// How many same-named skills this one shadows.
    pub shadowed_count: usize,
}

/// What `skills.list` and `skills.search` answer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkillsReply {
    pub skills: Vec<SkillSummary>,
}

/// What `skills.get` answers: one skill, with the body a summary omits.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkillDetail {
    pub name: String,
    /// The discovery scope the skill came from, as `SkillScope` spells it.
    pub scope: String,
    pub description: String,
    /// Where the skill file sits on disk.
    pub source_path: String,
    /// The agent the skill declares, when it declares one. Always written.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub agent: Option<String>,
    /// The licence the skill declares, when it declares one. Always written.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub license: Option<String>,
    /// The skill's Markdown body, without its frontmatter.
    pub body: String,
}

/// One ACP agent profile, with the availability probe's verdict.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AgentProfileEntry {
    pub name: String,
    /// The profile's description, or an empty string when it declares none.
    pub description: String,
    /// The command that spawns the agent, or an empty string when the profile
    /// names none. A profile with no command can never spawn, so it is never
    /// available.
    pub command: String,
    /// Whether the daemon ships this profile, rather than a config declaring it.
    pub is_builtin: bool,
    /// Whether the probe found the command on PATH and it answered `--version`.
    pub available: bool,
}

/// What `agents.list_profiles` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AgentProfilesReply {
    pub profiles: Vec<AgentProfileEntry>,
}

/// Answer with `value` as JSON, or report the serialisation failure.
///
/// The reply types here hold only strings, numbers, booleans and vectors of
/// those, so the error arm is unreachable in practice. It exists because an
/// `expect` here would take the daemon down over a reply nobody can act on.
fn reply<T: serde::Serialize>(id: Option<crate::protocol::RequestId>, value: T) -> Response {
    match serde_json::to_value(value) {
        Ok(value) => Response::success(id, value),
        Err(e) => Response::error(id, INTERNAL_ERROR, e.to_string()),
    }
}

pub(crate) async fn handle_mcp_start(
    req: Request,
    km: &Arc<KilnManager>,
    mcp_mgr: &Arc<McpServerManager>,
    plugin_tools: Option<Arc<crate::plugin_tools::PluginRegistry>>,
) -> Response {
    // The client's own request type is the contract (gate A6): it derives
    // `Deserialize`, the client serializes it, and re-deriving its five field
    // names here is what let `LuaInitSessionRequest.config` drift.
    let params = match typed_params::<crate::rpc_client::McpStartRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let transport = params.transport.as_deref().unwrap_or("sse");
    let port = params.port.unwrap_or(3847);

    // The same provider an internal agent gets: the daemon's enrichment
    // config, or the empty provider, which reports semantic_search unavailable.
    let embedding_provider: Arc<dyn EmbeddingProvider> = match km.enrichment_config() {
        Some(config) => match crate::embedding::get_or_create_embedding_provider(config).await {
            Ok(provider) => provider,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to create embedding provider for the MCP server; \
                     semantic_search will report unavailable"
                );
                Arc::new(EmptyEmbeddingProvider)
            }
        },
        None => Arc::new(EmptyEmbeddingProvider),
    };

    match mcp_mgr
        .start(
            km,
            transport,
            port,
            &params.kiln_path,
            params.no_just,
            plugin_tools,
            embedding_provider,
        )
        .await
    {
        Ok(result) => Response::success(req.id, result),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e),
    }
}

pub(crate) async fn handle_mcp_stop(req: Request, mcp_mgr: &Arc<McpServerManager>) -> Response {
    match mcp_mgr.stop().await {
        Ok(result) => Response::success(req.id, result),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e),
    }
}

pub(crate) async fn handle_mcp_status(req: Request, mcp_mgr: &Arc<McpServerManager>) -> Response {
    reply(req.id, mcp_mgr.status().await)
}

/// Discover the skills visible from `kiln_path`, off the async runtime.
async fn discover_skills(
    kiln_path: String,
) -> Result<
    crate::skills::SkillResult<std::collections::HashMap<String, crate::skills::ResolvedSkill>>,
    tokio::task::JoinError,
> {
    tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln), dirs::home_dir().as_deref());
        FolderDiscovery::new(paths).discover()
    })
    .await
}

pub(crate) async fn handle_skills_list(req: Request) -> Response {
    let params = match typed_params::<crate::rpc_client::SkillsListRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kiln_path = params.kiln_path;
    let scope_filter = params.scope_filter;

    let result = discover_skills(kiln_path).await;

    match result {
        Ok(Ok(skills)) => {
            let mut skills: Vec<SkillSummary> = skills
                .iter()
                .filter(|(_, resolved)| {
                    if let Some(ref filter) = scope_filter {
                        resolved.skill.source.scope.to_string() == *filter
                    } else {
                        true
                    }
                })
                .map(|(name, resolved)| SkillSummary {
                    name: name.clone(),
                    scope: resolved.skill.source.scope.to_string(),
                    description: resolved.skill.description.clone(),
                    shadowed_count: resolved.shadowed.len(),
                })
                .collect();
            skills.sort_by(|a, b| a.name.cmp(&b.name));
            reply(req.id, SkillsReply { skills })
        }
        Ok(Err(e)) => internal_error(req.id, e),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_skills_get(req: Request) -> Response {
    let params = match typed_params::<crate::rpc_client::SkillsGetRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let name = params.name;
    let kiln_path = params.kiln_path;

    let result = discover_skills(kiln_path).await;

    match result {
        Ok(Ok(skills)) => match skills.get(&name) {
            Some(resolved) => {
                let skill = &resolved.skill;
                reply(
                    req.id,
                    SkillDetail {
                        name: skill.name.clone(),
                        scope: skill.source.scope.to_string(),
                        description: skill.description.clone(),
                        source_path: skill.source.path.to_string_lossy().into_owned(),
                        agent: skill.source.agent.clone(),
                        license: skill.license.clone(),
                        body: skill.body.clone(),
                    },
                )
            }
            None => Response::error(req.id, INVALID_PARAMS, format!("Skill not found: {}", name)),
        },
        Ok(Err(e)) => internal_error(req.id, e),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_skills_search(req: Request) -> Response {
    let params = match typed_params::<crate::rpc_client::SkillsSearchRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let query = params.query;
    let kiln_path = params.kiln_path;
    let limit = params.limit.unwrap_or(20);

    let result = discover_skills(kiln_path).await;

    match result {
        Ok(Ok(skills)) => {
            let query_lower = query.to_lowercase();
            let matches: Vec<SkillSummary> = skills
                .iter()
                .filter(|(name, resolved)| {
                    name.to_lowercase().contains(&query_lower)
                        || resolved
                            .skill
                            .description
                            .to_lowercase()
                            .contains(&query_lower)
                })
                .take(limit)
                .map(|(name, resolved)| SkillSummary {
                    name: name.clone(),
                    scope: resolved.skill.source.scope.to_string(),
                    description: resolved.skill.description.clone(),
                    shadowed_count: resolved.shadowed.len(),
                })
                .collect();
            reply(req.id, SkillsReply { skills: matches })
        }
        Ok(Err(e)) => internal_error(req.id, e),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_agents_list_profiles(
    req: Request,
    agent_manager: &Arc<AgentManager>,
) -> Response {
    let profiles = agent_manager.build_available_agents();
    let builtins = crate::acp::discovery::default_agent_profiles();

    // Probe availability concurrently: missing binaries fail the PATH lookup
    // in ~1ms, installed ones are bounded by the 2s --version probe timeout.
    let probes = profiles.iter().map(|(name, profile)| {
        let name = name.clone();
        let profile = profile.clone();
        let is_builtin = builtins.contains_key(&name);
        async move {
            let available = probe_profile_availability(&profile).await;
            AgentProfileEntry {
                name,
                description: profile.description.clone().unwrap_or_default(),
                command: profile.command.clone().unwrap_or_default(),
                is_builtin,
                available,
            }
        }
    });
    let mut profiles: Vec<AgentProfileEntry> = futures::future::join_all(probes).await;
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    reply(req.id, AgentProfilesReply { profiles })
}

/// The agent cards a session started from the request's workspace would
/// resolve, sorted by name. The daemon's own discovery answers, so
/// `cru agents list` cannot advertise a card `session.create` would refuse.
pub(crate) async fn handle_agents_list_cards(
    req: Request,
    agent_manager: &Arc<AgentManager>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::AgentsListCardsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let workspace = std::path::PathBuf::from(params.workspace);
    let kiln_path = params.kiln_path.map(std::path::PathBuf::from);
    let mut cards: Vec<_> = crate::agent_cards::discover_agent_cards_in(
        agent_manager.card_roots(),
        &workspace,
        kiln_path.as_deref(),
    )
    .into_values()
    .collect();
    cards.sort_by(|a, b| a.name.cmp(&b.name));
    Response::success(req.id, serde_json::json!({ "cards": cards }))
}

/// A profile with no command can never spawn, so it is never available;
/// otherwise availability is the binary probe (PATH + bounded --version).
async fn probe_profile_availability(profile: &crucible_core::config::AgentProfile) -> bool {
    match profile.command.as_deref() {
        Some(cmd) => crate::acp::is_agent_available(cmd).await,
        None => false,
    }
}

pub(crate) async fn handle_agents_resolve_profile(
    req: Request,
    agent_manager: &Arc<AgentManager>,
) -> Response {
    let name = match typed_params::<crate::rpc_client::NameRequest>(&req) {
        Ok(p) => p.name,
        Err(response) => return *response,
    };
    let profiles = agent_manager.build_available_agents();
    let builtins = crate::acp::discovery::default_agent_profiles();

    match profiles.get(&name) {
        Some(profile) => Response::success(
            req.id,
            serde_json::json!({
                "name": name,
                "description": profile.description.clone().unwrap_or_default(),
                "command": profile.command.clone().unwrap_or_default(),
                "is_builtin": builtins.contains_key(&name),
                "args": profile.args.clone().unwrap_or_default(),
                "env": profile.env,
            }),
        ),
        None => Response::success(req.id, serde_json::Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::AgentProfile;

    fn profile_with_command(command: Option<&str>) -> AgentProfile {
        AgentProfile {
            extends: None,
            command: command.map(str::to_string),
            args: None,
            env: std::collections::BTreeMap::new(),
            description: None,
            delegation: None,
            permissions: None,
        }
    }

    #[tokio::test]
    async fn profile_without_command_is_unavailable() {
        assert!(!probe_profile_availability(&profile_with_command(None)).await);
    }

    #[tokio::test]
    async fn profile_with_unknown_command_is_unavailable() {
        let profile = profile_with_command(Some("crucible-no-such-agent-binary-98765"));
        assert!(!probe_profile_availability(&profile).await);
    }

    /// `skills.get` needs both fields; the typed request rejects a caller that
    /// sends one, naming the one it missed.
    #[tokio::test]
    async fn skills_get_without_a_kiln_path_is_invalid_params() {
        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: Some(crate::protocol::RequestId::Number(1)),
            method: "skills.get".to_string(),
            params: serde_json::json!({ "name": "commit" }),
        };

        let resp = handle_skills_get(req).await;

        let error = resp.error.expect("a request with no `kiln_path` must fail");
        assert_eq!(error.code, INVALID_PARAMS);
        assert!(
            error.message.contains("kiln_path"),
            "the message must name the field: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn profile_with_present_command_is_available() {
        // `cargo` exists wherever the tests run and answers --version.
        let profile = profile_with_command(Some("cargo"));
        assert!(probe_profile_availability(&profile).await);
    }
}
