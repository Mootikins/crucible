use super::*;
use crate::rpc_helpers::typed_params;

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

    match mcp_mgr
        .start(
            km,
            transport,
            port,
            &params.kiln_path,
            params.no_just,
            plugin_tools,
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
    let status = mcp_mgr.status().await;
    Response::success(req.id, status)
}

pub(crate) async fn handle_skills_list(req: Request) -> Response {
    let params = match typed_params::<crate::rpc_client::SkillsListRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kiln_path = params.kiln_path;
    let scope_filter = params.scope_filter;

    let result = tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln), dirs::home_dir().as_deref());
        let discovery = FolderDiscovery::new(paths);
        discovery.discover()
    })
    .await;

    match result {
        Ok(Ok(skills)) => {
            let mut entries: Vec<serde_json::Value> = skills
                .iter()
                .filter(|(_, resolved)| {
                    if let Some(ref filter) = scope_filter {
                        resolved.skill.source.scope.to_string() == *filter
                    } else {
                        true
                    }
                })
                .map(|(name, resolved)| {
                    serde_json::json!({
                        "name": name,
                        "scope": resolved.skill.source.scope.to_string(),
                        "description": resolved.skill.description,
                        "shadowed_count": resolved.shadowed.len(),
                    })
                })
                .collect();
            entries.sort_by(|a, b| {
                a["name"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["name"].as_str().unwrap_or(""))
            });
            Response::success(req.id, serde_json::json!({ "skills": entries }))
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

    let result = tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln), dirs::home_dir().as_deref());
        let discovery = FolderDiscovery::new(paths);
        discovery.discover()
    })
    .await;

    match result {
        Ok(Ok(skills)) => match skills.get(&name) {
            Some(resolved) => {
                let skill = &resolved.skill;
                Response::success(
                    req.id,
                    serde_json::json!({
                        "name": skill.name,
                        "scope": skill.source.scope.to_string(),
                        "description": skill.description,
                        "source_path": skill.source.path.to_string_lossy(),
                        "agent": skill.source.agent,
                        "license": skill.license,
                        "body": skill.body,
                    }),
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

    let result = tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln), dirs::home_dir().as_deref());
        let discovery = FolderDiscovery::new(paths);
        discovery.discover()
    })
    .await;

    match result {
        Ok(Ok(skills)) => {
            let query_lower = query.to_lowercase();
            let matches: Vec<serde_json::Value> = skills
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
                .map(|(name, resolved)| {
                    serde_json::json!({
                        "name": name,
                        "scope": resolved.skill.source.scope.to_string(),
                        "description": resolved.skill.description,
                        "shadowed_count": resolved.shadowed.len(),
                    })
                })
                .collect();
            Response::success(req.id, serde_json::json!({ "skills": matches }))
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
            serde_json::json!({
                "name": name,
                "description": profile.description.clone().unwrap_or_default(),
                "command": profile.command.clone().unwrap_or_default(),
                "is_builtin": is_builtin,
                "available": available,
            })
        }
    });
    let mut entries: Vec<serde_json::Value> = futures::future::join_all(probes).await;
    entries.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    Response::success(req.id, serde_json::json!({ "profiles": entries }))
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
            env: std::collections::HashMap::new(),
            description: None,
            capabilities: None,
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

    #[tokio::test]
    async fn profile_with_present_command_is_available() {
        // `cargo` exists wherever the tests run and answers --version.
        let profile = profile_with_command(Some("cargo"));
        assert!(probe_profile_availability(&profile).await);
    }
}
