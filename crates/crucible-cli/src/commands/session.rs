//! Session management commands
//!
//! Commands for listing, viewing, resuming, and managing chat sessions.

use crate::cli::{DaemonSessionCommands, SessionCommands};
use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use crucible_core::storage::NoteStore;
use crucible_observe::{
    extract_session_content, list_sessions, load_events, render_to_markdown, LogEvent,
    RenderOptions, SessionId, SessionType,
};
use crucible_rpc::DaemonClient;
use std::path::PathBuf;
use tokio::fs;

/// Execute a session subcommand
pub async fn execute(config: CliConfig, cmd: SessionCommands) -> Result<()> {
    match cmd {
        SessionCommands::List {
            limit,
            session_type,
            format,
        } => list(config, limit, session_type, format).await,
        SessionCommands::Search { query, limit } => search(config, query, limit).await,
        SessionCommands::Show { id, format } => show(config, id, format).await,
        SessionCommands::Resume { id } => resume(config, id).await,
        SessionCommands::Export {
            id,
            output,
            timestamps,
        } => export(config, id, output, timestamps).await,
        SessionCommands::Reindex { force } => reindex(config, force).await,
        SessionCommands::Cleanup {
            older_than,
            dry_run,
        } => cleanup(config, older_than, dry_run).await,
        SessionCommands::Daemon(subcmd) => daemon_execute(config, subcmd).await,
    }
}

/// Get the sessions directory path
fn sessions_dir(config: &CliConfig) -> PathBuf {
    config.kiln_path.join(".crucible").join("sessions")
}

/// List recent sessions
async fn list(
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

    let mut ids = list_sessions(&sessions_path).await?;

    // Filter by type if specified
    if let Some(type_filter) = session_type {
        let filter_type: SessionType = type_filter
            .parse()
            .map_err(|_| anyhow!("Invalid session type: {}", type_filter))?;
        ids.retain(|id| id.session_type() == filter_type);
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
            for id in &ids {
                // Get event count and first user message for preview
                let session_dir = sessions_path.join(id.as_str());
                let events = load_events(&session_dir).await.unwrap_or_default();
                let msg_count = events
                    .iter()
                    .filter(|e| matches!(e, LogEvent::User { .. } | LogEvent::Assistant { .. }))
                    .count();

                // Get title from first user message
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

                println!("  {} ({} messages)", id, msg_count);
                println!("    {}\n", title);
            }
        }
    }

    Ok(())
}

/// Search sessions by title/content using ripgrep (with fallback)
async fn search(config: CliConfig, query: String, limit: u32) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions found.");
        return Ok(());
    }

    // Try ripgrep first, fall back to in-memory scan if not available
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
    sessions_path: &PathBuf,
    query: &str,
    limit: u32,
) -> Result<Vec<(String, usize, String)>> {
    let ids = list_sessions(sessions_path).await?;
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
    let sessions_path = sessions_dir(&config);
    let session_id = SessionId::parse(&id)?;
    let session_dir = sessions_path.join(session_id.as_str());

    if !session_dir.exists() {
        return Err(anyhow!("Session not found: {}", id));
    }

    let events = load_events(&session_dir).await?;

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&events)?;
            println!("{json}");
        }
        "markdown" | "md" => {
            let md = render_to_markdown(&events, &RenderOptions::default());
            println!("{md}");
        }
        _ => {
            // Text format - simplified view
            println!("Session: {}\n", id);
            println!("Events: {}\n", events.len());

            for event in &events {
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
        return Err(anyhow!("Session not found: {}", id));
    }

    crate::commands::chat::execute(
        config,
        None,
        None,
        false,
        false,
        false,
        None,
        false,
        false,
        None,
        16384,
        vec![],
        Some(id),
    )
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
        return Err(anyhow!("Session not found: {}", id));
    }

    let events = load_events(&session_dir).await?;

    let options = RenderOptions {
        include_timestamps: timestamps,
        ..Default::default()
    };

    let md = render_to_markdown(&events, &options);

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

    let ids = list_sessions(&sessions_path).await?;
    if ids.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    println!("Found {} sessions to scan.", ids.len());

    let storage = crate::factories::get_storage(&config).await?;
    let note_store = match storage.note_store() {
        Some(ns) => ns,
        None => {
            println!("NoteStore not available — session content extracted but not stored.");
            println!("Configure storage.mode = \"embedded\", \"daemon\", or \"lightweight\".");
            return Ok(());
        }
    };

    let embedding_provider = match crate::factories::get_or_create_embedding_provider(&config).await
    {
        Ok(p) => Some(p),
        Err(e) => {
            println!(
                "Embedding provider unavailable ({}), indexing without embeddings.",
                e
            );
            None
        }
    };

    let mut indexed = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;

    for id in &ids {
        let session_dir = sessions_path.join(id.as_str());
        let path = format!("sessions/{}", id.as_str());

        if !force {
            match note_store.get(&path).await {
                Ok(Some(_)) => {
                    skipped += 1;
                    continue;
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }

        let events = match load_events(&session_dir).await {
            Ok(e) => e,
            Err(e) => {
                eprintln!("  Error loading {}: {}", id, e);
                errors += 1;
                continue;
            }
        };

        let content = match extract_session_content(id.as_str(), &events) {
            Some(c) => c,
            None => {
                skipped += 1;
                continue;
            }
        };

        let embedding = if let Some(ref provider) = embedding_provider {
            match provider.embed(&content.content_for_embedding()).await {
                Ok(emb) => Some(emb),
                Err(e) => {
                    eprintln!("  Embedding failed for {}: {}", id, e);
                    errors += 1;
                    None
                }
            }
        } else {
            None
        };

        let record = content.to_note_record(embedding);
        if let Err(e) = note_store.upsert(record).await {
            eprintln!("  Store failed for {}: {}", id, e);
            errors += 1;
            continue;
        }

        let label = if force { "Re-indexed" } else { "Indexed" };
        println!(
            "  {} {} ({} user messages)",
            label,
            id,
            content.user_messages.len()
        );
        indexed += 1;
    }

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

    let ids = list_sessions(&sessions_path).await?;
    let cutoff = Utc::now() - Duration::days(older_than as i64);

    let mut to_delete = Vec::new();

    for id in ids {
        let session_dir = sessions_path.join(id.as_str());
        let events = load_events(&session_dir).await.unwrap_or_default();

        // Get latest timestamp from events
        let latest = events.iter().map(|e| e.timestamp()).max();

        if let Some(ts) = latest {
            if ts < cutoff {
                to_delete.push((id, session_dir));
            }
        }
    }

    if to_delete.is_empty() {
        println!("No sessions older than {} days found.", older_than);
        return Ok(());
    }

    println!(
        "Found {} sessions older than {} days:",
        to_delete.len(),
        older_than
    );

    for (id, _) in &to_delete {
        println!("  {}", id);
    }

    if dry_run {
        println!("\nDry run - no sessions deleted.");
    } else {
        for (id, dir) in to_delete {
            fs::remove_dir_all(&dir).await?;
            println!("Deleted: {}", id);
        }
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

// =========================================================================
// Daemon Session Commands
// =========================================================================

/// Execute a daemon session subcommand
async fn daemon_execute(config: CliConfig, cmd: DaemonSessionCommands) -> Result<()> {
    let client = DaemonClient::connect_or_start()
        .await
        .map_err(|e| anyhow!("Failed to connect to daemon: {}", e))?;

    match cmd {
        DaemonSessionCommands::List { state } => daemon_list(&client, &config, state).await,
        DaemonSessionCommands::Create { session_type } => {
            daemon_create(&client, &config, &session_type).await
        }
        DaemonSessionCommands::Get { session_id } => daemon_get(&client, &session_id).await,
        DaemonSessionCommands::Pause { session_id } => daemon_pause(&client, &session_id).await,
        DaemonSessionCommands::Resume { session_id } => daemon_resume(&client, &session_id).await,
        DaemonSessionCommands::End { session_id } => daemon_end(&client, &session_id).await,
        DaemonSessionCommands::Send {
            session_id,
            message,
            raw,
        } => daemon_send(&client, &config, &session_id, &message, raw).await,
        DaemonSessionCommands::Configure {
            session_id,
            provider,
            model,
            endpoint,
        } => daemon_configure(&client, &config, &session_id, &provider, &model, endpoint).await,
        DaemonSessionCommands::Subscribe { session_ids } => daemon_subscribe(&session_ids).await,
        DaemonSessionCommands::Load { session_id } => {
            daemon_load(&client, &config, &session_id).await
        }
    }
}

/// List daemon sessions
async fn daemon_list(
    client: &DaemonClient,
    config: &CliConfig,
    state: Option<String>,
) -> Result<()> {
    let result = client
        .session_list(Some(&config.kiln_path), None, None, state.as_deref())
        .await?;

    let sessions = result["sessions"].as_array();

    let sessions = match sessions {
        Some(arr) if !arr.is_empty() => arr,
        _ => {
            println!("No daemon sessions found.");
            return Ok(());
        }
    };

    println!(
        "{:<40} {:<10} {:<10} STARTED",
        "SESSION_ID", "TYPE", "STATE"
    );
    println!("{}", "-".repeat(80));

    for session in sessions {
        println!(
            "{:<40} {:<10} {:<10} {}",
            session["session_id"].as_str().unwrap_or("?"),
            session["type"].as_str().unwrap_or("?"),
            session["state"].as_str().unwrap_or("?"),
            session["started_at"].as_str().unwrap_or("?"),
        );
    }

    Ok(())
}

/// Create a new daemon session
async fn daemon_create(
    client: &DaemonClient,
    config: &CliConfig,
    session_type: &str,
) -> Result<()> {
    let result = client
        .session_create(session_type, &config.kiln_path, None, vec![])
        .await?;

    let session_id = result["session_id"].as_str().unwrap_or("unknown");
    println!("Created session: {}", session_id);
    println!("Type: {}", session_type);
    println!("Kiln: {}", config.kiln_path.display());

    Ok(())
}

/// Get details of a daemon session
async fn daemon_get(client: &DaemonClient, session_id: &str) -> Result<()> {
    let result = client.session_get(session_id).await?;

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

    Ok(())
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

/// Resume a paused daemon session
async fn daemon_resume(client: &DaemonClient, session_id: &str) -> Result<()> {
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

async fn daemon_send(
    _client: &DaemonClient,
    config: &CliConfig,
    session_id: &str,
    message: &str,
    raw: bool,
) -> Result<()> {
    use crucible_rpc::DaemonClient;
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
    provider: &str,
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
        provider: provider.to_string(),
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
    };

    client.session_configure_agent(session_id, &agent).await?;

    println!("Configured agent: {} / {}", provider, model);

    Ok(())
}

async fn daemon_subscribe(session_ids: &[String]) -> Result<()> {
    use crucible_rpc::DaemonClient;

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
    use crucible_observe::{SessionType, SessionWriter};
    use tempfile::TempDir;

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
        let result = list(config, 10, None, "table".to_string()).await;
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

        let result = list(config, 10, None, "table".to_string()).await;
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
        use crucible_observe::extract_session_content;

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
        use crucible_observe::extract_session_content;

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
}
