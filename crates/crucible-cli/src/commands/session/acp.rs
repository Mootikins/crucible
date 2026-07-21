use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use crucible_core::config::BackendType;
use crucible_core::session::{OutputValidation, SessionAgent};
use crucible_daemon::DaemonClient;

pub(super) async fn resolve_acp_profile(
    client: &DaemonClient,
    agent_name: &str,
) -> Result<crucible_core::config::AgentProfile> {
    let profile_json = client.agents_resolve_profile(agent_name).await?;
    let profile: crucible_core::config::AgentProfile = serde_json::from_value(profile_json)
        .map_err(|e| anyhow!("Failed to deserialize agent profile: {}", e))?;
    Ok(profile)
}

pub(super) mod rpc {
    use super::*;

    pub(crate) struct CreateParams<'a> {
        pub session_type: &'a str,
        pub agent: Option<&'a str>,
        pub recording_mode: Option<&'a str>,
        pub quiet: bool,
        pub format: &'a str,
        pub title: Option<&'a str>,
        pub workspace: Option<&'a std::path::Path>,
        pub permission_mode: Option<String>,
    }

    pub(crate) async fn list(
        client: &DaemonClient,
        _config: &CliConfig,
        session_type: Option<&str>,
        state: Option<&str>,
        format: &str,
        limit: Option<u32>,
    ) -> Result<()> {
        let result = client
            // Pass None to search all kilns + crucible home, not just config.kiln_path.
            // Sessions may be stored under crucible_home (~/.crucible) regardless of
            // which kiln_path is in the current config.
            .session_list(None, None, session_type, state, None)
            .await?;

        let mut sessions = result["sessions"].as_array().cloned().unwrap_or_default();

        if sessions.is_empty() {
            println!("No daemon sessions found.");
            return Ok(());
        }

        // Apply limit
        if let Some(n) = limit {
            sessions.truncate(n as usize);
        }

        match format {
            "json" => {
                let json_output = serde_json::json!({"sessions": sessions});
                println!("{}", serde_json::to_string_pretty(&json_output)?);
            }
            _ => {
                println!(
                    "{:<40} {:<10} {:<10} STARTED",
                    "SESSION_ID", "TYPE", "STATE"
                );
                println!("{}", "-".repeat(80));

                for session in &sessions {
                    let started = session["started_at"]
                        .as_str()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| {
                            dt.with_timezone(&chrono::Local)
                                .format("%Y-%m-%d %H:%M")
                                .to_string()
                        })
                        .unwrap_or_else(|| "?".to_string());
                    println!(
                        "{:<40} {:<10} {:<10} {}",
                        session["session_id"].as_str().unwrap_or("?"),
                        session["type"].as_str().unwrap_or("?"),
                        session["state"].as_str().unwrap_or("?"),
                        started,
                    );
                }
            }
        }

        Ok(())
    }

    /// Build a default internal SessionAgent from global config.
    ///
    /// Same logic as `cru chat` uses via `SessionAgent::internal_from_config`,
    /// so CLI sessions get the same provider/model defaults.
    fn build_default_session_agent(config: &CliConfig) -> SessionAgent {
        let effective_llm = config.effective_llm_provider().ok();
        let model = effective_llm
            .as_ref()
            .map(|p| p.model.clone())
            .or_else(|| config.chat.model.clone())
            .unwrap_or_else(|| crucible_core::config::DEFAULT_CHAT_MODEL.to_string());
        let backend_type = effective_llm
            .as_ref()
            .map(|p| p.provider_type)
            .unwrap_or(BackendType::Ollama);
        let provider_key = effective_llm
            .as_ref()
            .map(|p| p.key.clone())
            .unwrap_or_else(|| backend_type.as_str().to_string());

        SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some(provider_key),
            provider: backend_type,
            model,
            system_prompt: String::new(),
            temperature: effective_llm
                .as_ref()
                .map(|p| p.temperature as f64)
                .or_else(|| config.chat.temperature.map(|t| t as f64)),
            max_tokens: effective_llm
                .as_ref()
                .map(|p| p.max_tokens)
                .or(config.chat.max_tokens),
            endpoint: effective_llm
                .as_ref()
                .map(|p| p.endpoint.clone())
                .or_else(|| config.chat.endpoint.clone()),
            max_context_tokens: None,
            thinking_budget: None,
            env_overrides: std::collections::HashMap::new(),
            mcp_servers: vec![],
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
            output_validation: OutputValidation::default(),
            validation_retries: 3,
            autocompact_threshold: None,
            mode: None,
        }
    }

    pub(crate) async fn create(
        client: &DaemonClient,
        config: &CliConfig,
        params: CreateParams<'_>,
    ) -> Result<()> {
        let recording_mode_parsed = match params.recording_mode {
            Some("granular") => Some("granular".to_string()),
            Some("coarse") => Some("coarse".to_string()),
            Some(other) => anyhow::bail!(
                "Invalid recording mode: '{}'. Must be 'granular' or 'coarse'",
                other
            ),
            None => None,
        };

        let agent_type = if params.agent.is_some() {
            "acp"
        } else {
            "internal"
        };

        let result = client
            .session_create(crucible_daemon::rpc_client::SessionCreateParams {
                session_type: params.session_type.to_string(),
                kiln: Some(config.session_storage_path()),
                workspace: params.workspace.map(|p| p.to_path_buf()),
                connect_kilns: vec![],
                recording_mode: recording_mode_parsed,
                recording_path: None,
                agent_type: Some(agent_type.to_string()),
            })
            .await?;

        let session_id = result["session_id"].as_str().unwrap_or("unknown");

        if let Some(agent_name) = params.agent {
            let profile = super::resolve_acp_profile(client, agent_name)
                .await
                .map_err(|e| anyhow!("Failed to resolve ACP agent profile: {}", e))?;
            let session_agent = SessionAgent::from_profile(&profile, agent_name);
            client
                .session_configure_agent(session_id, &session_agent)
                .await?;
        } else {
            // Auto-configure from global config (same as `cru chat`)
            let session_agent = build_default_session_agent(config);
            client
                .session_configure_agent(session_id, &session_agent)
                .await?;
        }

        if let Some(t) = params.title {
            client.session_set_title(session_id, t).await?;
        }

        let is_quiet = params.quiet || !crate::output::is_interactive();

        if is_quiet {
            println!("{}", session_id);
        } else if params.format == "json" {
            let json = serde_json::json!({
                "session_id": session_id,
                "type": params.session_type,
                "kiln": config.kiln_path.to_string_lossy(),
                "agent": params.agent,
                "title": params.title,
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        } else {
            println!("Created session: {}", session_id);
            println!("\nTo use this session:");
            println!("  export CRU_SESSION={}", session_id);
            println!(
                "  export CRU_SESSION_DIR={}",
                config
                    .kiln_path
                    .join(".crucible")
                    .join("sessions")
                    .join(session_id)
                    .display()
            );
            println!("Type: {}", params.session_type);
            println!("Kiln: {}", config.kiln_path.display());
            if let Some(mode) = params.recording_mode {
                println!("Recording mode: {}", mode);
            }
            if let Some(agent_name) = params.agent {
                println!("Configured agent: {} (acp)", agent_name);
            }
            if let Some(t) = params.title {
                println!("Title: {}", t);
            }
            if let Some(ref mode) = params.permission_mode {
                println!("Permission mode: {}", mode);
            }
        }

        Ok(())
    }

    pub(crate) async fn pause(client: &DaemonClient, session_id: &str, format: &str) -> Result<()> {
        let result = client.session_pause(session_id).await?;
        if format == "json" {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "session_id": session_id,
                    "previous_state": result["previous_state"].as_str().unwrap_or("?"),
                    "current_state": "paused",
                }))?
            );
        } else {
            println!("Paused session: {}", session_id);
            println!(
                "Previous state: {}",
                result["previous_state"].as_str().unwrap_or("?")
            );
        }
        Ok(())
    }

    pub(crate) async fn resume(
        client: &DaemonClient,
        session_id: &str,
        format: &str,
    ) -> Result<()> {
        let result = client.session_resume(session_id).await?;
        if format == "json" {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "session_id": session_id,
                    "previous_state": result["previous_state"].as_str().unwrap_or("?"),
                    "current_state": "active",
                }))?
            );
        } else {
            println!("Resumed session: {}", session_id);
            println!(
                "Previous state: {}",
                result["previous_state"].as_str().unwrap_or("?")
            );
        }
        Ok(())
    }

    pub(crate) async fn end(client: &DaemonClient, session_id: &str, format: &str) -> Result<()> {
        client.session_end(session_id).await?;
        if format == "json" {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "session_id": session_id,
                    "ended": true,
                }))?
            );
        } else {
            println!("Ended session: {}", session_id);
        }
        Ok(())
    }

    pub(crate) async fn send(
        config: &CliConfig,
        session_id: &str,
        message: &str,
        raw: bool,
        permission_mode: Option<String>,
    ) -> Result<()> {
        use crucible_daemon::DaemonClient;
        use std::io::Write;

        let (client, mut event_rx) = DaemonClient::connect_or_start_with_events().await?;

        client.session_subscribe(&[session_id]).await?;

        let message_id = match client
            .session_send_message_with_permissions(
                session_id,
                message,
                false,
                permission_mode.clone(),
            )
            .await
        {
            Ok(id) => id,
            Err(e) if e.to_string().contains("not found") => {
                eprintln!("Session not in memory, loading from storage...");
                client
                    .session_resume_from_storage(session_id, &config.kiln_path, None, None)
                    .await?;
                client
                    .session_send_message_with_permissions(
                        session_id,
                        message,
                        false,
                        permission_mode,
                    )
                    .await?
            }
            Err(e) => return Err(e),
        };

        if !raw {
            eprintln!("--- Message {} ---", message_id);
        }

        loop {
            match event_rx.recv().await {
                Some(event) => {
                    if event.session_id != session_id {
                        continue;
                    }

                    if raw {
                        println!(
                            "{}",
                            serde_json::json!({
                                "session_id": event.session_id,
                                "event_type": event.event_type,
                                "data": event.data,
                            })
                        );
                    } else {
                        match event.event_type.as_str() {
                            "text_delta" => {
                                if let Some(content) =
                                    event.data.get("content").and_then(|v| v.as_str())
                                {
                                    print!("{}", content);
                                    std::io::stdout().flush().ok();
                                }
                            }
                            "thinking" => {
                                if let Some(content) =
                                    event.data.get("content").and_then(|v| v.as_str())
                                {
                                    eprintln!("[thinking] {}", content);
                                }
                            }
                            "tool_call" => {
                                let tool = event
                                    .data
                                    .get("tool")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                eprintln!("[tool_call] {}", tool);
                            }
                            "tool_result" => {
                                let tool = event
                                    .data
                                    .get("tool")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                eprintln!("[tool_result] {}", tool);
                            }
                            "message_complete" => {
                                println!();
                                eprintln!("[complete]");
                            }
                            "ended" => {
                                let reason = event
                                    .data
                                    .get("reason")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown");
                                eprintln!("[ended] {}", reason);
                            }
                            other => {
                                eprintln!("[{}] {:?}", other, event.data);
                            }
                        }
                    }

                    if event.event_type == "message_complete" || event.event_type == "ended" {
                        break;
                    }
                }
                None => {
                    eprintln!("Event channel closed");
                    break;
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn configure(
        client: &DaemonClient,
        config: &CliConfig,
        session_id: &str,
        provider: BackendType,
        model: &str,
        endpoint: Option<String>,
        format: &str,
    ) -> Result<()> {
        let mcp_servers = config
            .mcp
            .as_ref()
            .map(|mcp| mcp.servers.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default();

        let agent = crucible_core::session::SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some(provider.to_string()),
            provider,
            model: model.to_string(),
            system_prompt: String::new(),
            temperature: None,
            max_tokens: None,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: endpoint.clone(),
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
            output_validation: OutputValidation::default(),
            validation_retries: 3,
            autocompact_threshold: None,
            mode: None,
        };

        client.session_configure_agent(session_id, &agent).await?;

        if format == "json" {
            let json_output = serde_json::json!({
                "session_id": session_id,
                "provider": provider.to_string(),
                "model": model,
                "endpoint": endpoint
            });
            println!("{}", json_output);
        } else {
            println!("Configured agent: {} / {}", provider, model);
        }

        Ok(())
    }

    pub(crate) async fn subscribe(session_ids: &[String]) -> Result<()> {
        use crucible_daemon::DaemonClient;

        let (client, mut event_rx) = DaemonClient::connect_or_start_with_events().await?;

        let refs: Vec<&str> = session_ids.iter().map(|s| s.as_str()).collect();
        client.session_subscribe(&refs).await?;

        println!(
            "Subscribed to {} session(s). Press Ctrl+C to exit.",
            session_ids.len()
        );

        loop {
            match event_rx.recv().await {
                Some(event) => {
                    println!(
                        "[{}] {} {}",
                        event.session_id,
                        event.event_type,
                        serde_json::to_string(&event.data)?
                    );
                }
                None => {
                    eprintln!("Event channel closed");
                    break;
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn replay(
        _config: &CliConfig,
        recording_path: &str,
        speed: f64,
        raw: bool,
    ) -> Result<()> {
        use crucible_daemon::DaemonClient;
        use std::io::Write;
        use std::path::Path;

        let (client, mut event_rx) = DaemonClient::connect_or_start_with_events().await?;

        let result = client
            .session_replay(Path::new(recording_path), speed)
            .await?;

        let session_id = result["session_id"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing session_id in replay response"))?;

        client.session_subscribe(&[session_id]).await?;

        if !raw {
            eprintln!(
                "Replaying {} at {}x speed (session: {})",
                recording_path, speed, session_id
            );
        }

        loop {
            match event_rx.recv().await {
                Some(event) => {
                    if event.session_id != session_id {
                        continue;
                    }

                    if event.event_type == "replay_complete" {
                        if !raw {
                            eprintln!("[replay complete]");
                        }
                        break;
                    }

                    if raw {
                        println!(
                            "{}",
                            serde_json::json!({
                                "session_id": event.session_id,
                                "event_type": event.event_type,
                                "data": event.data,
                            })
                        );
                    } else {
                        match event.event_type.as_str() {
                            "text_delta" => {
                                if let Some(content) =
                                    event.data.get("content").and_then(|v| v.as_str())
                                {
                                    print!("{}", content);
                                    std::io::stdout().flush().ok();
                                }
                            }
                            "thinking" => {
                                if let Some(content) =
                                    event.data.get("content").and_then(|v| v.as_str())
                                {
                                    eprintln!("[thinking] {}", content);
                                }
                            }
                            "tool_call" => {
                                let tool = event
                                    .data
                                    .get("tool")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                eprintln!("[tool_call] {}", tool);
                            }
                            "tool_result" => {
                                let tool = event
                                    .data
                                    .get("tool")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                eprintln!("[tool_result] {}", tool);
                            }
                            "message_complete" => {
                                println!();
                                eprintln!("[complete]");
                            }
                            "ended" => {
                                eprintln!("[ended]");
                                break;
                            }
                            other => {
                                eprintln!("[{}]", other);
                            }
                        }
                    }
                }
                None => {
                    if !raw {
                        eprintln!("[replay complete]");
                    }
                    break;
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn load(
        client: &DaemonClient,
        config: &CliConfig,
        session_id: &str,
    ) -> Result<()> {
        let result = client
            .session_resume_from_storage(session_id, &config.kiln_path, None, None)
            .await?;

        println!("Loaded session: {}", session_id);
        if let Some(events) = result.get("events_loaded").and_then(|v| v.as_u64()) {
            println!("Events loaded: {}", events);
        }
        if let Some(state) = result.get("state").and_then(|v| v.as_str()) {
            println!("State: {}", state);
        }

        Ok(())
    }
}
