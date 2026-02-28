use super::*;

pub(super) async fn handle_mcp_start(
    req: Request,
    km: &Arc<KilnManager>,
    mcp_mgr: &Arc<McpServerManager>,
) -> Response {
    let kiln_path = require_param!(req, "kiln_path", as_str);
    let transport = optional_param!(req, "transport", as_str).unwrap_or("sse");
    let port = optional_param!(req, "port", as_u64).unwrap_or(3847) as u16;
    let no_just = optional_param!(req, "no_just", as_bool).unwrap_or(false);
    let just_dir = optional_param!(req, "just_dir", as_str);

    match mcp_mgr
        .start(km, transport, port, kiln_path, no_just, just_dir)
        .await
    {
        Ok(result) => Response::success(req.id, result),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e),
    }
}

pub(super) async fn handle_mcp_stop(req: Request, mcp_mgr: &Arc<McpServerManager>) -> Response {
    match mcp_mgr.stop().await {
        Ok(result) => Response::success(req.id, result),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e),
    }
}

pub(super) async fn handle_mcp_status(req: Request, mcp_mgr: &Arc<McpServerManager>) -> Response {
    let status = mcp_mgr.status().await;
    Response::success(req.id, status)
}

pub(super) async fn handle_skills_list(req: Request) -> Response {
    let kiln_path = require_param!(req, "kiln_path", as_str).to_string();
    let scope_filter = optional_param!(req, "scope_filter", as_str).map(|s| s.to_string());

    let result = tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln));
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

pub(super) async fn handle_skills_get(req: Request) -> Response {
    let name = require_param!(req, "name", as_str).to_string();
    let kiln_path = require_param!(req, "kiln_path", as_str).to_string();

    let result = tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln));
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

pub(super) async fn handle_skills_search(req: Request) -> Response {
    let query = require_param!(req, "query", as_str).to_string();
    let kiln_path = require_param!(req, "kiln_path", as_str).to_string();
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(20) as usize;

    let result = tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln));
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

pub(super) async fn handle_agents_list_profiles(
    req: Request,
    agent_manager: &Arc<AgentManager>,
) -> Response {
    let profiles = agent_manager.build_available_agents();
    let builtins = crucible_acp::discovery::default_agent_profiles();

    let mut entries: Vec<serde_json::Value> = profiles
        .iter()
        .map(|(name, profile)| {
            serde_json::json!({
                "name": name,
                "description": profile.description.clone().unwrap_or_default(),
                "command": profile.command.clone().unwrap_or_default(),
                "is_builtin": builtins.contains_key(name),
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    Response::success(req.id, serde_json::json!({ "profiles": entries }))
}

pub(super) async fn handle_agents_resolve_profile(
    req: Request,
    agent_manager: &Arc<AgentManager>,
) -> Response {
    let name = require_param!(req, "name", as_str).to_string();
    let profiles = agent_manager.build_available_agents();
    let builtins = crucible_acp::discovery::default_agent_profiles();

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
