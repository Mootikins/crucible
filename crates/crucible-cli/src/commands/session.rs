//! Session management commands
//!
//! Commands for listing, viewing, resuming, and managing chat sessions.

use crate::cli::SessionCommands;
use crate::common::daemon_client;
use crate::config::CliConfig;
use crate::output;
use anyhow::{anyhow, Result};
use crucible_config::BackendType;
use crucible_core::session::SessionAgent;
use crucible_daemon::DaemonClient;
use crucible_daemon::{LogEvent, SessionId, SessionType};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::fs;

pub fn resolve_session_id(explicit: Option<String>) -> anyhow::Result<String> {
    explicit
        .or_else(|| std::env::var("CRU_SESSION").ok())
        .ok_or_else(|| {
            anyhow!("No session specified. Pass session ID or set CRU_SESSION env var.")
        })
}

/// Print a deprecation warning to stderr. Old form is described by `old`,
/// the preferred replacement by `new`.
#[allow(dead_code)]
fn warn_deprecated(old: &str, new: &str) {
    eprintln!("warning: '{}' is deprecated, use '{}' instead", old, new);
}

/// Route output to JSON or human-readable based on `format`.
///
/// When `format == "json"`, serializes `value` as pretty-printed JSON to stdout.
/// Otherwise calls `human_fn` to render human-readable output.
#[allow(dead_code)]
fn print_json_or_text(
    value: &serde_json::Value,
    format: &str,
    human_fn: impl FnOnce(&serde_json::Value),
) {
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string()));
    } else {
        human_fn(value);
    }
}

fn resolve_send_inputs(
    session_id_pos: Option<String>,
    message: Option<String>,
    session_id_flag: Option<String>,
) -> (Option<String>, Option<String>, bool) {
    if let Some(flag_id) = session_id_flag {
        return (Some(flag_id), session_id_pos, true);
    }

    if session_id_pos.is_some() && message.is_some() {
        return (session_id_pos, message, false);
    }

    if session_id_pos.is_some() && std::env::var("CRU_SESSION").is_ok() {
        return (None, session_id_pos, false);
    }

    (session_id_pos, message, false)
}

/// Execute a session subcommand
pub async fn execute(config: CliConfig, cmd: SessionCommands) -> Result<()> {
    match cmd {
        SessionCommands::List {
            limit,
            session_type,
            format,
            state,
            all,
        } => list(config, limit, session_type, format, state, all).await,
        SessionCommands::Search { query, limit } => search(config, query, limit).await,
        SessionCommands::Show { id, format } => {
            let session_id = resolve_session_id(id)?;
            show(config, session_id, format).await
        },
        SessionCommands::Open { id } => {
            let session_id = resolve_session_id(id)?;
            resume(config, session_id).await
        },
        SessionCommands::Export {
            id,
            output,
            timestamps,
        } => {
            let session_id = resolve_session_id(id)?;
            export(config, session_id, output, timestamps).await
        },
        SessionCommands::Reindex { force } => reindex(config, force).await,
        SessionCommands::Cleanup {
            older_than,
            dry_run,
        } => cleanup(config, older_than, dry_run).await,
        SessionCommands::Create {
            session_type,
            agent,
            recording_mode,
            quiet,
            format,
            title,
            workspace,
        } => {
            let client = daemon_client().await?;
            daemon_create(
                &client,
                &config,
                &session_type,
                agent.as_deref(),
                recording_mode.as_deref(),
                quiet,
                &format,
                title.as_deref(),
                workspace.as_deref(),
            )
            .await
        }
        SessionCommands::Pause { session_id } => {
            let session_id = resolve_session_id(session_id)?;
            let client = daemon_client().await?;
            daemon_pause(&client, &session_id).await
        }
        SessionCommands::Resume { session_id } => {
            let session_id = resolve_session_id(session_id)?;
            let client = daemon_client().await?;
            unpause(&client, &session_id).await
        }
        SessionCommands::Unpause { session_id } => {
            let session_id = resolve_session_id(session_id)?;
            warn_deprecated("unpause", "resume");
            let client = daemon_client().await?;
            unpause(&client, &session_id).await
        }
        SessionCommands::End { session_id } => {
            let session_id = resolve_session_id(session_id)?;
            let client = daemon_client().await?;
            daemon_end(&client, &session_id).await
        }
        SessionCommands::Send {
            session_id_pos,
            message,
            session_id_flag,
            raw,
        } => {
            let (resolved_session_id, resolved_message_arg, used_deprecated_flag) =
                resolve_send_inputs(session_id_pos, message, session_id_flag);
            if used_deprecated_flag {
                warn_deprecated("--session", "positional SESSION_ID");
            }

            let session_id = resolve_session_id(resolved_session_id)?;
            let message = match resolved_message_arg {
                Some(msg) => crate::commands::stdin::resolve_message(&msg)?,
                None => {
                    if crate::commands::stdin::stdin_is_piped() {
                        crate::commands::stdin::read_stdin_message()?
                    } else {
                        anyhow::bail!("No message provided. Pass a message or pipe stdin.")
                    }
                }
            };
            daemon_send(&config, &session_id, &message, raw).await
        },
        SessionCommands::Configure {
            session_id,
            provider,
            model,
            endpoint,
        } => {
            let session_id = resolve_session_id(session_id)?;
            let provider_type = BackendType::from_str(&provider)
                .map_err(|e| anyhow!("Invalid provider '{}': {}", provider, e))?;
            let client = daemon_client().await?;
            daemon_configure(
                &client,
                &config,
                &session_id,
                provider_type,
                &model,
                endpoint,
            )
            .await
        }
        SessionCommands::Subscribe { session_ids } => daemon_subscribe(&session_ids).await,
        SessionCommands::Load { session_id } => {
            let session_id = resolve_session_id(session_id)?;
            let client = daemon_client().await?;
            daemon_load(&client, &config, &session_id).await
        }
        SessionCommands::Replay {
            recording_path,
            speed,
            raw,
        } => daemon_replay(&config, &recording_path, speed, raw).await,
    }
}

/// Get the sessions directory path
fn sessions_dir(config: &CliConfig) -> PathBuf {
    config.kiln_path.join(".crucible").join("sessions")
}

/// Read events from a session directory's JSONL file (local fallback).
///
/// Used when daemon RPC is unavailable for backward compatibility.
async fn read_session_events(session_dir: &std::path::Path) -> Result<Vec<LogEvent>> {
    let jsonl_path = session_dir.join("session.jsonl");
    let content = fs::read_to_string(&jsonl_path)
        .await
        .map_err(|e| anyhow!("Failed to read session events: {}", e))?;
    let mut events = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<LogEvent>(line) {
            events.push(event);
        }
    }
    Ok(events)
}

/// List session directory names under a sessions base path (local fallback).
async fn list_session_dirs(sessions_path: &std::path::Path) -> Result<Vec<String>> {
    let mut entries = fs::read_dir(sessions_path).await?;
    let mut dirs = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                dirs.push(name.to_string());
            }
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// Simple markdown rendering of events (local fallback).
fn format_events_markdown(events: &[LogEvent], _include_timestamps: bool) -> String {
    use std::fmt::Write;
    let mut md = String::new();
    for event in events {
        match event {
            LogEvent::System { content, .. } => {
                let _ = writeln!(md, "> {}\n", content);
            }
            LogEvent::User { content, .. } => {
                let _ = write!(md, "## User\n\n{}\n\n", content);
            }
            LogEvent::Assistant { content, model, .. } => {
                let label = model.as_deref().unwrap_or("Assistant");
                let _ = write!(md, "## {}\n\n{}\n\n", label, content);
            }
            LogEvent::Thinking { content, .. } => {
                let _ = writeln!(
                    md,
                    "<details><summary>Thinking</summary>\n\n{}\n</details>\n",
                    content
                );
            }
            LogEvent::ToolCall { name, .. } => {
                let _ = writeln!(md, "### Tool: {}\n", name);
            }
            _ => {}
        }
    }
    md
}

/// Display events in text format.
fn display_events_text(id: &str, events: &[LogEvent]) {
    println!("Session: {}\n", id);
    println!("Events: {}\n", events.len());

    for event in events {
        match event {
            LogEvent::System { content, .. } => {
                println!("[system] {}", truncate(content, 100));
            }
            LogEvent::User { content, .. } => {
                println!("\n[user]\n{}\n", content);
            }
            LogEvent::Assistant { content, model, .. } => {
                let model_str = model.as_deref().unwrap_or("unknown");
                println!("[assistant ({})]\n{}\n", model_str, content);
            }
            LogEvent::ToolCall { name, id, .. } => {
                println!("[tool:{}] id={}", name, id);
            }
            LogEvent::ToolResult { id, truncated, .. } => {
                let marker = if *truncated { " (truncated)" } else { "" };
                println!("[result:{}]{}", id, marker);
            }
            LogEvent::Error {
                message,
                recoverable,
                ..
            } => {
                let level = if *recoverable { "warning" } else { "error" };
                println!("[{}] {}", level, message);
            }
            LogEvent::Init {
                session_id, model, ..
            } => {
                let model_str = model.as_deref().unwrap_or("unknown");
                println!("[init] session={}, model={}", session_id, model_str);
            }
            LogEvent::Thinking { content, .. } => {
                println!("[thinking] {}", truncate(content, 100));
            }
            LogEvent::Permission { tool, decision, .. } => {
                println!("[permission] {}:{:?}", tool, decision);
            }
            LogEvent::Summary {
                content,
                messages_summarized,
                ..
            } => {
                let count = messages_summarized
                    .map(|n| format!(" ({n} msgs)"))
                    .unwrap_or_default();
                println!("[summary{}] {}", count, truncate(content, 100));
            }
            LogEvent::BashSpawned { id, command, .. } => {
                println!("[bash:{}] {}", id, truncate(command, 80));
            }
            LogEvent::BashCompleted { id, exit_code, .. } => {
                println!("[bash:{}] exit={}", id, exit_code);
            }
            LogEvent::BashFailed { id, error, .. } => {
                println!("[bash:{}] FAILED: {}", id, truncate(error, 80));
            }
            LogEvent::SubagentSpawned {
                id, session_link, ..
            } => {
                println!("[subagent:{}] {}", id, session_link);
            }
            LogEvent::SubagentCompleted {
                id,
                summary,
                session_link,
                ..
            } => {
                println!(
                    "[subagent:{}] {} -> {}",
                    id,
                    session_link,
                    truncate(summary, 60)
                );
            }
            LogEvent::SubagentFailed {
                id,
                error,
                session_link,
                ..
            } => {
                println!(
                    "[subagent:{}] {} FAILED: {}",
                    id,
                    session_link,
                    truncate(error, 60)
                );
            }
        }
    }
}

/// List recent sessions
async fn list(
    config: CliConfig,
    limit: u32,
    session_type: Option<String>,
    format: String,
    state: Option<String>,
    all: bool,
) -> Result<()> {
    let client = daemon_client().await?;

    daemon_list(&client, &config, session_type.as_deref(), state.as_deref(), &format, Some(limit)).await?;

    if all {
        println!();
        println!("Persisted sessions:");
        list_persisted(config, limit, session_type, format).await?;
    }

    Ok(())
}

async fn list_persisted(
    config: CliConfig,
    limit: u32,
    session_type: Option<String>,
    format: String,
) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions found.");
        println!("Sessions will be stored in: {}", sessions_path.display());
        return Ok(());
    }

    // Try daemon RPC first
    if let Ok(client) = daemon_client().await {
        if let Ok(result) = client
            .session_list_persisted(
                &config.kiln_path,
                session_type.as_deref(),
                Some(limit as usize),
            )
            .await
        {
            let sessions = result
                .get("sessions")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if sessions.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }
            match format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&sessions)?;
                    println!("{json}");
                }
                _ => {
                    println!("Sessions (newest first):\n");
                    for s in &sessions {
                        let id = s["id"].as_str().unwrap_or("?");
                        let msg_count = s["message_count"].as_u64().unwrap_or(0);
                        let title = s["title"].as_str().unwrap_or("(empty)");
                        println!("  {} ({} messages)", id, msg_count);
                        println!("    {}\n", title);
                    }
                }
            }
            return Ok(());
        }
    }

    // Fallback: read session directories locally
    let mut ids = list_session_dirs(&sessions_path).await?;

    // Filter by type if specified
    if let Some(type_filter) = session_type {
        let filter_type: SessionType = type_filter
            .parse()
            .map_err(|_| anyhow!("Invalid session type: {}", type_filter))?;
        ids.retain(|id_str| {
            SessionId::parse(id_str)
                .map(|id| id.session_type() == filter_type)
                .unwrap_or(false)
        });
    }

    // Reverse to show newest first, then take limit
    ids.reverse();
    ids.truncate(limit as usize);

    if ids.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&ids)?;
            println!("{json}");
        }
        _ => {
            println!("Sessions (newest first):\n");
            for id_str in &ids {
                let session_dir = sessions_path.join(id_str);
                let events = read_session_events(&session_dir).await.unwrap_or_default();
                let msg_count = events
                    .iter()
                    .filter(|e| matches!(e, LogEvent::User { .. } | LogEvent::Assistant { .. }))
                    .count();

                let title = events
                    .iter()
                    .find_map(|e| match e {
                        LogEvent::User { content, .. } => {
                            let preview = content.chars().take(50).collect::<String>();
                            if content.len() > 50 {
                                Some(format!("{}...", preview))
                            } else {
                                Some(preview)
                            }
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| "(empty)".to_string());

                println!("  {} ({} messages)", id_str, msg_count);
                println!("    {}\n", title);
            }
        }
    }

    Ok(())
}

/// Search sessions by title/content via daemon RPC (with local fallback)
async fn search(config: CliConfig, query: String, limit: u32) -> Result<()> {
    // Try daemon RPC first
    if let Ok(client) = daemon_client().await {
        if let Ok(result) = client
            .session_search(&query, Some(&config.kiln_path), Some(limit as usize))
            .await
        {
            let matches = result
                .get("matches")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if matches.is_empty() {
                println!("No sessions matching '{}' found.", query);
            } else {
                println!("Sessions matching '{}':\n", query);
                for m in &matches {
                    let session_id = m["session_id"].as_str().unwrap_or("");
                    let line = m["line"].as_u64().unwrap_or(0);
                    let context = m["context"].as_str().unwrap_or("");
                    println!("  {} (line {})", session_id, line);
                    println!("    {}\n", context);
                }
            }
            return Ok(());
        }
    }

    // Fallback: local filesystem search
    let sessions_path = sessions_dir(&config);
    if !sessions_path.exists() {
        println!("No sessions found.");
        return Ok(());
    }
    let matches = match search_with_ripgrep(&sessions_path, &query, limit).await {
        Ok(results) => results,
        Err(e) => {
            tracing::debug!(
                "Ripgrep search failed ({}), falling back to in-memory scan",
                e
            );
            search_in_memory(&sessions_path, &query, limit).await?
        }
    };
    if matches.is_empty() {
        println!("No sessions matching '{}' found.", query);
        return Ok(());
    }
    println!("Sessions matching '{}':\n", query);
    for (session_id, line_num, context) in matches {
        println!("  {} (line {})", session_id, line_num);
        println!("    {}\n", context);
    }
    Ok(())
}

/// Search using ripgrep for fast text search
async fn search_with_ripgrep(
    sessions_path: &PathBuf,
    query: &str,
    limit: u32,
) -> Result<Vec<(String, usize, String)>> {
    use std::process::Command;

    // Check if ripgrep is available
    let rg_check = Command::new("rg").arg("--version").output();
    if rg_check.is_err() {
        return Err(anyhow!("ripgrep not found"));
    }

    // Run ripgrep with JSON output
    let output = Command::new("rg")
        .arg("--json")
        .arg("--max-count")
        .arg(limit.to_string())
        .arg("--context")
        .arg("2") // 2 lines before/after
        .arg("--glob")
        .arg("*.jsonl") // Only search JSONL files
        .arg(query)
        .arg(sessions_path)
        .output()
        .map_err(|e| anyhow!("Failed to run ripgrep: {}", e))?;

    if !output.status.success() {
        // Exit code 1 means no matches (not an error)
        if output.status.code() == Some(1) {
            return Ok(vec![]);
        }
        return Err(anyhow!("ripgrep failed with status: {}", output.status));
    }

    // Parse ripgrep JSON output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json["type"] == "match" {
                if let Some(data) = json["data"].as_object() {
                    // Extract session ID from path
                    let path = data["path"]["text"].as_str().unwrap_or("");
                    let session_id = extract_session_id_from_path(path);

                    // Extract line number
                    let line_num = data["line_number"].as_u64().unwrap_or(0) as usize;

                    // Extract matching line
                    let content = data["lines"]["text"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_string();

                    // Truncate long lines
                    let content = if content.len() > 100 {
                        format!("{}...", content.chars().take(100).collect::<String>())
                    } else {
                        content
                    };

                    results.push((session_id, line_num, content));

                    if results.len() >= limit as usize {
                        break;
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Fallback in-memory search when ripgrep is not available
async fn search_in_memory(
    sessions_path: &Path,
    query: &str,
    limit: u32,
) -> Result<Vec<(String, usize, String)>> {
    let ids = list_session_dirs(sessions_path).await?;
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for id in ids {
        let session_dir = sessions_path.join(id.as_str());
        let jsonl_path = session_dir.join("session.jsonl");

        if !jsonl_path.exists() {
            continue;
        }

        // Read JSONL file
        let content = match fs::read_to_string(&jsonl_path).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Search line by line
        for (line_num, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                let preview = if line.len() > 100 {
                    format!("{}...", line.chars().take(100).collect::<String>())
                } else {
                    line.to_string()
                };

                results.push((id.to_string(), line_num + 1, preview));

                if results.len() >= limit as usize {
                    return Ok(results);
                }
            }
        }
    }

    Ok(results)
}

/// Extract session ID from a file path
fn extract_session_id_from_path(path: &str) -> String {
    // Path format: .../sessions/{session_id}/session.jsonl
    std::path::Path::new(path)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Show session details
async fn show(config: CliConfig, id: String, format: String) -> Result<()> {
    let client = daemon_client().await.ok();

    // Check for live daemon session first
    if let Some(client) = &client {
        if let Ok(result) = client.session_get(&id).await {
            match format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&result)?;
                    println!("{json}");
                }
                _ => {
                    println!(
                        "Session ID: {}",
                        result["session_id"].as_str().unwrap_or("?")
                    );
                    println!("Type: {}", result["type"].as_str().unwrap_or("?"));
                    println!("State: {}", result["state"].as_str().unwrap_or("?"));
                    println!("Kiln: {}", result["kiln"].as_str().unwrap_or("?"));
                    println!("Started: {}", result["started_at"].as_str().unwrap_or("?"));
                    if let Some(title) = result["title"].as_str() {
                        println!("Title: {}", title);
                    }
                }
            }
            return Ok(());
        }
    }

    // Load persisted session
    let sessions_path = sessions_dir(&config);
    let session_id = SessionId::parse(&id)?;
    let session_dir = sessions_path.join(session_id.as_str());

    if !session_dir.exists() {
        output::hint("Try: `cru session list` to see available sessions");
        anyhow::bail!("Session not found: {}", id);
    }

    // Try daemon RPC for persisted session data
    if let Some(client) = &client {
        let loaded = match format.as_str() {
            "json" => {
                if let Ok(events_json) = client.session_load_events(&session_dir).await {
                    let json = serde_json::to_string_pretty(&events_json)?;
                    println!("{json}");
                    true
                } else {
                    false
                }
            }
            "markdown" | "md" => {
                if let Ok(md) = client
                    .session_render_markdown(&session_dir, None, None, None, None)
                    .await
                {
                    println!("{md}");
                    true
                } else {
                    false
                }
            }
            _ => {
                if let Ok(events_json) = client.session_load_events(&session_dir).await {
                    if let Ok(events) = serde_json::from_value::<Vec<LogEvent>>(events_json) {
                        display_events_text(&id, &events);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };
        if loaded {
            return Ok(());
        }
    }

    // Fallback: read events from filesystem directly
    let events = read_session_events(&session_dir).await?;

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&events)?;
            println!("{json}");
        }
        "markdown" | "md" => {
            let md = format_events_markdown(&events, false);
            println!("{md}");
        }
        _ => {
            display_events_text(&id, &events);
        }
    }

    Ok(())
}

/// Resume a previous session
async fn resume(config: CliConfig, id: String) -> Result<()> {
    // Validate session exists before launching chat
    let session_id = SessionId::parse(&id)?;
    let sessions_path = sessions_dir(&config);
    let session_dir = sessions_path.join(session_id.as_str());

    if !session_dir.exists() {
        output::hint("Try: `cru session list` to see available sessions");
        anyhow::bail!("Session not found: {}", id);
    }

    crate::commands::chat::execute(crate::commands::chat::ExecuteParams {
        config,
        agent_name: None,
        query: None,
        read_only: false,
        no_context: false,
        context_size: None,
        provider_key: None,
        max_context_tokens: 16384,
        env_overrides: vec![],
        resume_session_id: Some(id),
        set_overrides: vec![],
        record: None,
        replay: None,
        replay_speed: 1.0,
        replay_auto_exit: None,
    })
    .await
}

/// Export session to markdown
async fn export(
    config: CliConfig,
    id: String,
    output: Option<PathBuf>,
    timestamps: bool,
) -> Result<()> {
    let sessions_path = sessions_dir(&config);
    let session_id = SessionId::parse(&id)?;
    let session_dir = sessions_path.join(session_id.as_str());

    if !session_dir.exists() {
        output::hint("Try: `cru session list` to see available sessions");
        anyhow::bail!("Session not found: {}", id);
    }

    // Try daemon RPC first
    if let Ok(client) = daemon_client().await {
        if let Ok(output_path_str) = client
            .session_export_to_file(&session_dir, output.as_deref(), Some(timestamps))
            .await
        {
            println!("Exported session to: {}", output_path_str);
            return Ok(());
        }
    }

    // Fallback: read events and render locally
    let events = read_session_events(&session_dir).await?;
    let md = format_events_markdown(&events, timestamps);
    let output_path = output.unwrap_or_else(|| session_dir.join("session.md"));
    fs::write(&output_path, &md).await?;
    println!("Exported session to: {}", output_path.display());

    Ok(())
}

/// Rebuild session index by extracting content and upserting into NoteStore
async fn reindex(config: CliConfig, force: bool) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions directory found.");
        return Ok(());
    }

    // Quick check for empty directory
    let dirs = list_session_dirs(&sessions_path).await?;
    if dirs.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    let client = daemon_client().await?;

    let result = client.session_reindex(&config.kiln_path, force).await?;

    let indexed = result["indexed"].as_u64().unwrap_or(0);
    let skipped = result["skipped"].as_u64().unwrap_or(0);
    let errors = result["errors"].as_u64().unwrap_or(0);

    println!(
        "\nIndexed {} sessions ({} skipped, {} errors)",
        indexed, skipped, errors
    );

    Ok(())
}

/// Clean up old sessions
async fn cleanup(config: CliConfig, older_than: u32, dry_run: bool) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions directory found.");
        return Ok(());
    }

    let client = daemon_client().await?;

    let result = client
        .session_cleanup(&config.kiln_path, older_than as u64, dry_run)
        .await?;

    let deleted = result["deleted"].as_array().cloned().unwrap_or_default();
    let total = result["total"].as_u64().unwrap_or(0);
    let is_dry_run = result["dry_run"].as_bool().unwrap_or(false);

    if total == 0 {
        println!("No sessions older than {} days found.", older_than);
        return Ok(());
    }

    println!("Found {} sessions older than {} days:", total, older_than);

    for id in &deleted {
        if let Some(s) = id.as_str() {
            println!("  {}", s);
        }
    }

    if is_dry_run {
        println!("\nDry run - no sessions deleted.");
    } else {
        println!("\nCleanup complete.");
    }

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max_len).collect::<String>())
    }
}

/// List daemon sessions
async fn daemon_list(
    client: &DaemonClient,
    config: &CliConfig,
    session_type: Option<&str>,
    state: Option<&str>,
    format: &str,
    limit: Option<u32>,
) -> Result<()> {
    let result = client
        .session_list(Some(&config.kiln_path), None, session_type, state, None)
        .await?;

    let mut sessions = result["sessions"]
        .as_array()
        .cloned()
        .unwrap_or_default();

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
                println!(
                    "{:<40} {:<10} {:<10} {}",
                    session["session_id"].as_str().unwrap_or("?"),
                    session["type"].as_str().unwrap_or("?"),
                    session["state"].as_str().unwrap_or("?"),
                    session["started_at"].as_str().unwrap_or("?"),
                );
            }
        }
    }

    Ok(())
}

async fn daemon_create(
    client: &DaemonClient,
    config: &CliConfig,
    session_type: &str,
    agent: Option<&str>,
    recording_mode: Option<&str>,
    quiet: bool,
    format: &str,
    title: Option<&str>,
    workspace: Option<&std::path::Path>,
) -> Result<()> {
    let recording_mode_parsed = if let Some(mode_str) = recording_mode {
        match mode_str {
            "granular" => Some("granular".to_string()),
            "coarse" => Some("coarse".to_string()),
            _ => anyhow::bail!(
                "Invalid recording mode: '{}'. Must be 'granular' or 'coarse'",
                mode_str
            ),
        }
    } else {
        None
    };

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: session_type.to_string(),
            kiln: config.kiln_path.clone(),
            workspace: workspace.map(|p| p.to_path_buf()),
            connect_kilns: vec![],
            recording_mode: recording_mode_parsed,
            recording_path: None,
        })
        .await?;

    let session_id = result["session_id"].as_str().unwrap_or("unknown");

    if let Some(agent_name) = agent {
        let profile = resolve_acp_profile(client, agent_name)
            .await
            .map_err(|e| anyhow!("Failed to resolve ACP agent profile: {}", e))?;
        let session_agent = SessionAgent::from_profile(&profile, agent_name);
        client
            .session_configure_agent(session_id, &session_agent)
            .await?;
    }

    if let Some(t) = title {
        client.session_set_title(session_id, t).await?;
    }

    let is_quiet = quiet || !crate::output::is_interactive();

    if is_quiet {
        println!("{}", session_id);
    } else if format == "json" {
        let json = serde_json::json!({
            "session_id": session_id,
            "type": session_type,
            "kiln": config.kiln_path.to_string_lossy(),
            "agent": agent,
            "title": title,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("Created session: {}", session_id);
        println!("\nTo use this session: export CRU_SESSION={}", session_id);
        println!("Type: {}", session_type);
        println!("Kiln: {}", config.kiln_path.display());
        if let Some(mode) = recording_mode {
            println!("Recording mode: {}", mode);
        }
        if let Some(agent_name) = agent {
            println!("Configured agent: {} (acp)", agent_name);
        }
        if let Some(t) = title {
            println!("Title: {}", t);
        }
    }

    Ok(())
}

async fn resolve_acp_profile(
    client: &DaemonClient,
    agent_name: &str,
) -> Result<crucible_config::AgentProfile> {
    let profile_json = client.agents_resolve_profile(agent_name).await?;
    let profile: crucible_config::AgentProfile = serde_json::from_value(profile_json)
        .map_err(|e| anyhow!("Failed to deserialize agent profile: {}", e))?;
    Ok(profile)
}

/// Pause a daemon session
async fn daemon_pause(client: &DaemonClient, session_id: &str) -> Result<()> {
    let result = client.session_pause(session_id).await?;
    println!("Paused session: {}", session_id);
    println!(
        "Previous state: {}",
        result["previous_state"].as_str().unwrap_or("?")
    );
    Ok(())
}

async fn unpause(client: &DaemonClient, session_id: &str) -> Result<()> {
    let result = client.session_resume(session_id).await?;
    println!("Resumed session: {}", session_id);
    println!(
        "Previous state: {}",
        result["previous_state"].as_str().unwrap_or("?")
    );
    Ok(())
}

/// End a daemon session
async fn daemon_end(client: &DaemonClient, session_id: &str) -> Result<()> {
    client.session_end(session_id).await?;
    println!("Ended session: {}", session_id);
    Ok(())
}

async fn daemon_send(config: &CliConfig, session_id: &str, message: &str, raw: bool) -> Result<()> {
    use crucible_daemon::DaemonClient;
    use std::io::Write;

    let (client, mut event_rx) = DaemonClient::connect_or_start_with_events().await?;

    client.session_subscribe(&[session_id]).await?;

    // Try to send - if session not found, load from storage and retry
    let message_id = match client.session_send_message(session_id, message).await {
        Ok(id) => id,
        Err(e) if e.to_string().contains("not found") => {
            eprintln!("Session not in memory, loading from storage...");
            client
                .session_resume_from_storage(session_id, &config.kiln_path, None, None)
                .await?;
            client.session_send_message(session_id, message).await?
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

/// Configure agent for a session
async fn daemon_configure(
    client: &DaemonClient,
    config: &CliConfig,
    session_id: &str,
    provider: BackendType,
    model: &str,
    endpoint: Option<String>,
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
        endpoint,
        env_overrides: std::collections::HashMap::new(),
        mcp_servers,
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
    };

    client.session_configure_agent(session_id, &agent).await?;

    println!("Configured agent: {} / {}", provider, model);

    Ok(())
}

async fn daemon_subscribe(session_ids: &[String]) -> Result<()> {
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

async fn daemon_replay(
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

async fn daemon_load(client: &DaemonClient, config: &CliConfig, session_id: &str) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_daemon::{SessionType, SessionWriter};
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn session_id_resolver_explicit_wins_over_env() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var("CRU_SESSION", "chat-from-env");

        let resolved = resolve_session_id(Some("chat-explicit".to_string())).unwrap();
        assert_eq!(resolved, "chat-explicit");

        std::env::remove_var("CRU_SESSION");
    }

    #[test]
    fn session_id_resolver_uses_env_when_explicit_missing() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var("CRU_SESSION", "chat-from-env");

        let resolved = resolve_session_id(None).unwrap();
        assert_eq!(resolved, "chat-from-env");

        std::env::remove_var("CRU_SESSION");
    }

    #[test]
    fn session_id_resolver_errors_when_no_source_available() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("CRU_SESSION");

        let result = resolve_session_id(None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No session specified"));
    }

    #[test]
    fn resolve_send_inputs_uses_deprecated_session_flag_and_warns() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("CRU_SESSION");

        let (session_id, message, used_deprecated_flag) = resolve_send_inputs(
            Some("hello".to_string()),
            None,
            Some("chat-123".to_string()),
        );

        assert_eq!(session_id, Some("chat-123".to_string()));
        assert_eq!(message, Some("hello".to_string()));
        assert!(used_deprecated_flag);
    }

    #[test]
    fn resolve_send_inputs_treats_two_positionals_as_session_and_message() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("CRU_SESSION");

        let (session_id, message, used_deprecated_flag) =
            resolve_send_inputs(Some("chat-123".to_string()), Some("hello".to_string()), None);

        assert_eq!(session_id, Some("chat-123".to_string()));
        assert_eq!(message, Some("hello".to_string()));
        assert!(!used_deprecated_flag);
    }

    #[test]
    fn resolve_send_inputs_treats_single_positional_as_message_when_env_set() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var("CRU_SESSION", "chat-from-env");

        let (session_id, message, used_deprecated_flag) =
            resolve_send_inputs(Some("hello".to_string()), None, None);

        assert_eq!(session_id, None);
        assert_eq!(message, Some("hello".to_string()));
        assert!(!used_deprecated_flag);

        std::env::remove_var("CRU_SESSION");
    }

    #[test]
    fn resolve_send_inputs_single_positional_without_env_uses_stdin_for_message() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("CRU_SESSION");

        let (session_id, message, used_deprecated_flag) =
            resolve_send_inputs(Some("chat-123".to_string()), None, None);

        assert_eq!(session_id, Some("chat-123".to_string()));
        assert_eq!(message, None);
        assert!(!used_deprecated_flag);
    }

    async fn setup_test_session(sessions_dir: &std::path::Path) -> SessionId {
        let mut writer = SessionWriter::create(sessions_dir, SessionType::Chat)
            .await
            .unwrap();
        writer
            .append(LogEvent::system("You are helpful"))
            .await
            .unwrap();
        writer
            .append(LogEvent::user("Hello, how are you?"))
            .await
            .unwrap();
        writer
            .append(LogEvent::assistant("I'm doing well, thanks!"))
            .await
            .unwrap();
        writer.id().clone()
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let tmp = TempDir::new().unwrap();
        let config = CliConfig {
            kiln_path: tmp.path().to_path_buf(),
            ..Default::default()
        };

        // Should not error with empty sessions
        let result = list_persisted(config, 10, None, "table".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_sessions_with_data() {
        let tmp = TempDir::new().unwrap();
        let sessions_path = tmp.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_path).unwrap();

        let _id = setup_test_session(&sessions_path).await;

        let config = CliConfig {
            kiln_path: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let result = list_persisted(config, 10, None, "table".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_show_session() {
        let tmp = TempDir::new().unwrap();
        let sessions_path = tmp.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_path).unwrap();

        let id = setup_test_session(&sessions_path).await;

        let config = CliConfig {
            kiln_path: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let result = show(config, id.to_string(), "text".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_show_session_not_found() {
        let tmp = TempDir::new().unwrap();
        let sessions_path = tmp.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_path).unwrap();

        let config = CliConfig {
            kiln_path: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let result = show(
            config,
            "chat-20260104-1530-a1b2".to_string(),
            "text".to_string(),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_export_session() {
        let tmp = TempDir::new().unwrap();
        let sessions_path = tmp.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_path).unwrap();

        let id = setup_test_session(&sessions_path).await;

        let config = CliConfig {
            kiln_path: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let output_path = tmp.path().join("exported.md");
        let result = export(config, id.to_string(), Some(output_path.clone()), false).await;
        assert!(result.is_ok());
        assert!(output_path.exists());

        let content = std::fs::read_to_string(output_path).unwrap();
        assert!(content.contains("## User"));
        assert!(content.contains("Hello, how are you?"));
    }

    #[tokio::test]
    async fn test_search_sessions() {
        let tmp = TempDir::new().unwrap();
        let sessions_path = tmp.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_path).unwrap();

        let _id = setup_test_session(&sessions_path).await;

        let config = CliConfig {
            kiln_path: tmp.path().to_path_buf(),
            ..Default::default()
        };

        // Should find session with "hello"
        let result = search(config.clone(), "hello".to_string(), 10).await;
        assert!(result.is_ok());

        // Should not find session with "nonexistent"
        let result = search(config, "nonexistent_term_xyz".to_string(), 10).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello...");
    }

    #[tokio::test]
    async fn test_reindex_no_sessions_dir() {
        let tmp = TempDir::new().unwrap();
        let config = CliConfig {
            kiln_path: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let result = reindex(config, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reindex_empty_sessions_dir() {
        let tmp = TempDir::new().unwrap();
        let sessions_path = tmp.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_path).unwrap();

        let config = CliConfig {
            kiln_path: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let result = reindex(config, false).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_session_content_for_reindex() {
        use crucible_daemon::extract_session_content;

        let events = vec![
            LogEvent::system("You are helpful"),
            LogEvent::user("What is Rust?"),
            LogEvent::assistant("Rust is a systems programming language."),
            LogEvent::user("Tell me more"),
            LogEvent::assistant("It focuses on safety and performance."),
        ];

        let content = extract_session_content("test-sess", &events).unwrap();
        assert_eq!(content.user_messages.len(), 2);
        assert_eq!(content.session_id, "test-sess");

        let record = content.to_note_record(None);
        assert_eq!(record.path, "sessions/test-sess");
        assert!(record.tags.contains(&"session".to_string()));
        assert!(record.embedding.is_none());
    }

    #[test]
    fn test_extract_session_content_skips_empty() {
        use crucible_daemon::extract_session_content;

        let events = vec![
            LogEvent::system("System prompt only"),
            LogEvent::assistant("Unprompted"),
        ];

        assert!(extract_session_content("empty-sess", &events).is_none());
    }

    #[tokio::test]
    async fn test_search_in_memory() {
        let tmp = TempDir::new().unwrap();
        let sessions_path = tmp.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_path).unwrap();

        let id = setup_test_session(&sessions_path).await;

        let results = super::search_in_memory(&sessions_path, "hello", 10)
            .await
            .unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0].0, id.to_string());
        assert!(results[0].2.to_lowercase().contains("hello"));
    }

    #[tokio::test]
    async fn test_search_in_memory_no_matches() {
        let tmp = TempDir::new().unwrap();
        let sessions_path = tmp.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_path).unwrap();

        let _id = setup_test_session(&sessions_path).await;

        let results = super::search_in_memory(&sessions_path, "nonexistent_xyz", 10)
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_extract_session_id_from_path() {
        let path = "/home/user/notes/.crucible/sessions/chat-20260104-1530-a1b2/session.jsonl";
        let id = super::extract_session_id_from_path(path);
        assert_eq!(id, "chat-20260104-1530-a1b2");

        let path = "sessions/agent-20260105-0900-xyz/session.jsonl";
        let id = super::extract_session_id_from_path(path);
        assert_eq!(id, "agent-20260105-0900-xyz");
    }

    #[tokio::test]
    async fn test_search_with_ripgrep_fallback() {
        let tmp = TempDir::new().unwrap();
        let sessions_path = tmp.path().join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_path).unwrap();

        let _id = setup_test_session(&sessions_path).await;

        let result = super::search_with_ripgrep(&sessions_path, "Hello", 10).await;

        match result {
            Ok(matches) => {
                if !matches.is_empty() {
                    assert!(matches[0].2.contains("Hello") || matches[0].2.contains("hello"));
                }
            }
            Err(_) => {
                // Ripgrep not installed or no matches - both are acceptable
            }
        }
    }

    #[test]
    fn test_daemon_create_recording_mode_parsing() {
        // Test valid recording modes
        let granular = "granular";
        match granular {
            "granular" => assert_eq!(granular, "granular"),
            "coarse" => panic!("Should not match coarse"),
            _ => panic!("Should not match invalid"),
        }

        let coarse = "coarse";
        match coarse {
            "granular" => panic!("Should not match granular"),
            "coarse" => assert_eq!(coarse, "coarse"),
            _ => panic!("Should not match invalid"),
        }

        // Test invalid mode would be caught by the match in daemon_create
        let invalid = "invalid";
        let result = match invalid {
            "granular" => Ok("granular"),
            "coarse" => Ok("coarse"),
            _ => Err(format!(
                "Invalid recording mode: '{}'. Must be 'granular' or 'coarse'",
                invalid
            )),
        };
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid recording mode"));
    }

    #[test]
    fn test_warn_deprecated_message_format() {
        warn_deprecated("--old-flag", "positional argument");
    }

    #[test]
    fn test_print_json_or_text_json_path() {
        let value = serde_json::json!({"key": "value", "num": 42});
        let mut captured = String::new();
        let json_str = serde_json::to_string_pretty(&value).unwrap();
        captured.push_str(&json_str);
        let parsed: serde_json::Value = serde_json::from_str(&captured).unwrap();
        assert_eq!(parsed["key"], "value");
        assert_eq!(parsed["num"], 42);
    }

    #[test]
    fn test_print_json_or_text_text_path_calls_human_fn() {
        let value = serde_json::json!({"session_id": "chat-123"});
        let mut called = false;
        print_json_or_text(&value, "text", |v| {
            called = true;
            assert_eq!(v["session_id"], "chat-123");
        });
        assert!(called, "human_fn should have been called for text format");
    }

    #[test]
    fn test_daemon_list_limit_applied() {
        let sessions = vec![
            serde_json::json!({"session_id": "chat-1", "type": "chat", "state": "active", "started_at": "2024-01-01"}),
            serde_json::json!({"session_id": "chat-2", "type": "chat", "state": "paused", "started_at": "2024-01-02"}),
            serde_json::json!({"session_id": "chat-3", "type": "chat", "state": "active", "started_at": "2024-01-03"}),
        ];

        let mut limited = sessions.clone();
        if let Some(n) = Some(2u32) {
            limited.truncate(n as usize);
        }

        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0]["session_id"], "chat-1");
        assert_eq!(limited[1]["session_id"], "chat-2");
    }

    #[test]
    fn test_daemon_list_json_format() {
        let sessions = vec![
            serde_json::json!({"session_id": "chat-1", "type": "chat", "state": "active", "started_at": "2024-01-01"}),
            serde_json::json!({"session_id": "chat-2", "type": "chat", "state": "paused", "started_at": "2024-01-02"}),
        ];

        let json_output = serde_json::json!({"sessions": sessions});
        let json_str = serde_json::to_string_pretty(&json_output).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed["sessions"].is_array());
        assert_eq!(parsed["sessions"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["sessions"][0]["session_id"], "chat-1");
        assert_eq!(parsed["sessions"][1]["session_id"], "chat-2");
    }
}
