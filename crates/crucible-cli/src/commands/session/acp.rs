use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use crucible_core::config::BackendType;
use crucible_core::session::OutputValidation;
use crucible_daemon::DaemonClient;

/// Whether `session create` should print only the session id.
///
/// Explicit beats implicit. Piped output defaults to the bare id, which is what
/// a shell script wants — but `--format json` is a request for structured
/// output, and a pipe is the only place anyone would make it. Letting the
/// implicit default win meant `--format json` printed JSON on a terminal and
/// silently degraded to a bare id exactly when something was parsing it.
///
/// `--quiet` is itself explicit, so it still wins over everything.
fn wants_bare_id(quiet: bool, interactive: bool, format: &str) -> bool {
    quiet || (!interactive && format != "json")
}

/// Which agent implementation `session create` asks the daemon for.
///
/// `--agent` and `--acp` name two different namespaces that the wire once
/// collapsed into one field: a card is an internal agent's persona, a profile
/// launches an external ACP subprocess. They are refused together rather than
/// ranked, because guessing which one a name belongs to would make a card
/// called `claude` permanently unreachable — the built-in profiles (`claude`,
/// `gemini`, `codex`, `cursor`, `opencode`) always exist, and
/// `cru agents show "Claude Code"` is the documented example of a card named
/// after one.
///
/// `--agent` is the card because that is what `cru agents` lists. The two
/// disagreed until now: `cru agents list` showed cards while `--agent` took a
/// profile, so the flag named the one thing the command did not.
///
/// Pure so it is testable without a live `DaemonClient`; `create` needs one.
fn agent_type_for(agent: Option<&str>, acp: Option<&str>) -> Result<&'static str> {
    match (agent, acp) {
        (Some(_), Some(_)) => Err(anyhow!(
            "--agent and --acp are mutually exclusive: --agent names an agent card \
             (an internal agent persona), --acp names an ACP profile (an external \
             agent subprocess)"
        )),
        (_, Some(_)) => Ok("acp"),
        (_, None) => Ok("internal"),
    }
}

/// Substring the daemon's unknown-card error is recognised by.
///
/// Matching text is unpleasant, but the RPC code (`INVALID_PARAMS`) is shared
/// with a dozen other create failures, and the alternative — the CLI listing
/// cards itself — would be a fourth card-discovery site rooted at the CLI's cwd
/// rather than the daemon's config dir, which is how the two would disagree.
const UNKNOWN_CARD_MARKER: &str = "Unknown agent card";

/// Point `--agent <profile>` at `--acp`, but only once the daemon confirms the
/// name really is a profile.
///
/// `--agent` used to mean an ACP profile, so `cru session create --agent
/// claude` is in people's shells and scripts today. After the rename it
/// fails as an unknown *card*, and the daemon's message lists cards — accurate,
/// and useless to someone who wanted the subprocess.
///
/// Asked rather than assumed: the built-in names could be hardcoded here, but
/// profiles are also user-defined in `[acp.agents.*]`, so a hardcoded list would
/// answer "no" for exactly the custom profile someone bothered to configure.
/// The lookup only happens on the error path, where a round trip costs nothing.
///
/// `is_known_profile` is passed in rather than looked up here so this stays a
/// pure function: the answer needs a live daemon, the wording does not.
fn annotate_unknown_agent(
    error: anyhow::Error,
    agent: Option<&str>,
    is_known_profile: bool,
) -> anyhow::Error {
    let Some(name) = agent else { return error };
    if !is_known_profile || !error.to_string().contains(UNKNOWN_CARD_MARKER) {
        return error;
    }
    anyhow!(
        "'{name}' is an ACP profile, not an agent card. Start the session with \
         `--acp {name}` instead.\n\n\
         `--agent` names an agent card — the prompt, model and tool policy of an \
         internal agent, as listed by `cru agents list`. It named an ACP profile \
         in earlier versions."
    )
}

/// Does the daemon know `name` as an ACP profile?
///
/// A null result means "no such profile" (`handle_agents_resolve_profile`), and
/// an RPC failure means we cannot tell — both answer `false`, which leaves the
/// daemon's own error (the one that lists the cards) as what the user sees.
async fn is_known_acp_profile(client: &DaemonClient, name: &str) -> bool {
    client
        .agents_resolve_profile(name)
        .await
        .is_ok_and(|v| !v.is_null())
}

pub(super) mod rpc {
    use super::*;

    pub(crate) struct CreateParams<'a> {
        pub session_type: &'a str,
        pub agent: Option<&'a str>,
        pub acp: Option<&'a str>,
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
        include_children: bool,
    ) -> Result<()> {
        let result = client
            // Pass None to search all kilns + crucible home, not just config.kiln_path.
            // Sessions may be stored under crucible_home (~/.crucible) regardless of
            // which kiln_path is in the current config.
            .session_list_with_children(
                None,
                None,
                session_type,
                state,
                None,
                include_children.then_some(true),
            )
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

        let agent_type = agent_type_for(params.agent, params.acp)?;

        // The daemon owns default-agent resolution: it builds the config-derived
        // internal defaults (or resolves the named ACP profile / agent card) and
        // configures the session's agent as part of create. An unknown profile
        // or card fails here with no session created.
        let agent_spec = crucible_daemon::rpc_client::SessionAgentSpec {
            agent_name: params.acp.map(str::to_string),
            agent_card: params.agent.map(str::to_string),
            ..Default::default()
        };

        let result = client
            .session_create_with_agent(
                crucible_daemon::rpc_client::SessionCreateParams {
                    session_type: params.session_type.to_string(),
                    kiln: Some(config.session_storage_path()),
                    workspace: params.workspace.map(|p| p.to_path_buf()),
                    connect_kilns: vec![],
                    recording_mode: recording_mode_parsed,
                    recording_path: None,
                    agent_type: Some(agent_type.to_string()),

                    isolation: None,
                },
                agent_spec,
            )
            .await;
        // The daemon answers in wire vocabulary (`agent_name`/`agent_card`)
        // because the web and plugins call it too; only the CLI knows those are
        // spelled `--acp` and `--agent` here.
        let result = match result {
            Ok(result) => result,
            Err(e) => {
                let looks_like_a_profile = match params.agent {
                    Some(name) => is_known_acp_profile(client, name).await,
                    None => false,
                };
                return Err(annotate_unknown_agent(
                    e,
                    params.agent,
                    looks_like_a_profile,
                ));
            }
        };

        let session_id = result["session_id"].as_str().unwrap_or("unknown");

        if let Some(t) = params.title {
            client.session_set_title(session_id, t).await?;
        }

        let is_quiet = wants_bare_id(params.quiet, crate::output::is_interactive(), params.format);

        if is_quiet {
            println!("{}", session_id);
        } else if params.format == "json" {
            let json = serde_json::json!({
                "session_id": session_id,
                "type": params.session_type,
                "kiln": config.kiln_path.to_string_lossy(),
                "agent": params.agent,
                "acp": params.acp,
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
            if let Some(card) = params.agent {
                println!("Configured agent: {} (card)", card);
            }
            if let Some(profile) = params.acp {
                println!("Configured agent: {} (acp)", profile);
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
            tool_policy: None,
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

#[cfg(test)]
mod tests {
    use super::{agent_type_for, annotate_unknown_agent, wants_bare_id};

    /// `--agent` names a card, which is what `cru agents list` shows. It named
    /// an ACP profile until the two were split, which is why a card was
    /// unreachable from the CLI at all.
    #[test]
    fn the_agent_flag_takes_the_internal_branch() {
        assert_eq!(
            agent_type_for(Some("researcher"), None).unwrap(),
            "internal"
        );
    }

    #[test]
    fn the_acp_flag_takes_the_acp_branch() {
        assert_eq!(agent_type_for(None, Some("claude")).unwrap(), "acp");
    }

    /// No flags is an internal session on the config defaults — unchanged.
    #[test]
    fn no_agent_flag_is_an_internal_session() {
        assert_eq!(agent_type_for(None, None).unwrap(), "internal");
    }

    /// Refused rather than ranked: the two names live in different namespaces
    /// and there is no correct precedence between them.
    #[test]
    fn a_card_and_a_profile_together_are_refused() {
        let err = agent_type_for(Some("researcher"), Some("claude")).unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"), "{err}");
    }

    /// The migration: `--agent claude` meant the subprocess in earlier
    /// versions and is in people's shells. It now fails as an unknown card, and
    /// the daemon's message helpfully lists cards — so the CLI has to be the one
    /// to say "that is a profile, use --acp".
    #[test]
    fn an_agent_that_is_really_a_profile_points_at_the_acp_flag() {
        let daemon = anyhow::anyhow!(
            "RPC error: {{\"code\":-32602,\"message\":\"Unknown agent card: claude. \
             Available cards: researcher\"}}"
        );
        let annotated = annotate_unknown_agent(daemon, Some("claude"), true).to_string();
        assert!(annotated.contains("--acp claude"), "{annotated}");
        assert!(annotated.contains("is an ACP profile"), "{annotated}");
    }

    /// A genuine typo is not a profile, so the daemon's message — which lists
    /// the cards that do exist — is the better answer and survives untouched.
    #[test]
    fn an_unknown_card_that_is_no_profile_keeps_the_daemons_list() {
        let daemon = anyhow::anyhow!(
            "RPC error: Unknown agent card: resercher. Available cards: researcher"
        );
        let annotated = annotate_unknown_agent(daemon, Some("resercher"), false).to_string();
        assert!(
            annotated.contains("Available cards: researcher"),
            "{annotated}"
        );
        assert!(!annotated.contains("--acp"), "{annotated}");
    }

    /// Anything else keeps its message: a trust refusal or a bad kiln has
    /// nothing to do with which flag named the agent.
    #[test]
    fn other_create_failures_are_left_alone() {
        let other = anyhow::anyhow!("RPC error: kiln is not a directory");
        assert_eq!(
            annotate_unknown_agent(other, Some("claude"), true).to_string(),
            "RPC error: kiln is not a directory"
        );
    }

    /// The regression: a pipe is the only place `--format json` is useful, and
    /// it was the one place the flag was ignored.
    #[test]
    fn format_json_survives_a_pipe() {
        assert!(!wants_bare_id(false, false, "json"));
        assert!(!wants_bare_id(false, true, "json"));
    }

    /// And the behaviour that must not change: no flags, piped, means the id
    /// alone, because that is what `$(cru session create)` expects.
    #[test]
    fn a_pipe_still_defaults_to_the_bare_id() {
        assert!(wants_bare_id(false, false, "text"));
        assert!(!wants_bare_id(false, true, "text"));
    }

    /// `--quiet` is explicit too, and more specific: it asks for the id and
    /// nothing else, whatever format was also named.
    #[test]
    fn quiet_wins_over_a_named_format() {
        assert!(wants_bare_id(true, true, "json"));
        assert!(wants_bare_id(true, false, "json"));
    }
}
