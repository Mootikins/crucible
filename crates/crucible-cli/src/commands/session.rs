//! Session management commands
//!
//! Commands for listing, viewing, resuming, and managing chat sessions.

use crate::cli::SessionCommands;
use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use crucible_observe::{
    list_sessions, load_events, render_to_markdown, LogEvent, RenderOptions, SessionId, SessionType,
};
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
        SessionCommands::Reindex => reindex(config).await,
        SessionCommands::Cleanup {
            older_than,
            dry_run,
        } => cleanup(config, older_than, dry_run).await,
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

/// Search sessions by title/content
async fn search(config: CliConfig, query: String, limit: u32) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions found.");
        return Ok(());
    }

    let ids = list_sessions(&sessions_path).await?;
    let query_lower = query.to_lowercase();

    let mut matches = Vec::new();

    for id in ids {
        let session_dir = sessions_path.join(id.as_str());
        let events = load_events(&session_dir).await.unwrap_or_default();

        // Search through user and assistant messages
        let matched = events.iter().any(|e| match e {
            LogEvent::User { content, .. } | LogEvent::Assistant { content, .. } => {
                content.to_lowercase().contains(&query_lower)
            }
            _ => false,
        });

        if matched {
            // Get first user message as title
            let title = events
                .iter()
                .find_map(|e| match e {
                    LogEvent::User { content, .. } => {
                        let preview = content.chars().take(60).collect::<String>();
                        if content.len() > 60 {
                            Some(format!("{}...", preview))
                        } else {
                            Some(preview)
                        }
                    }
                    _ => None,
                })
                .unwrap_or_else(|| "(empty)".to_string());

            matches.push((id, title));
        }

        if matches.len() >= limit as usize {
            break;
        }
    }

    if matches.is_empty() {
        println!("No sessions matching '{}' found.", query);
        return Ok(());
    }

    println!("Sessions matching '{}':\n", query);
    for (id, title) in matches {
        println!("  {}", id);
        println!("    {}\n", title);
    }

    Ok(())
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
                }
            }
        }
    }

    Ok(())
}

/// Resume a previous session (placeholder - actual resumption happens in chat)
async fn resume(_config: CliConfig, id: String) -> Result<()> {
    // For now, just validate the session exists and print instructions
    // Full resumption will be implemented when chat integration is done
    let session_id = SessionId::parse(&id)?;

    println!("To resume session {}, run:", session_id);
    println!("  cru chat --resume {}", session_id);
    println!();
    println!("Note: Session resumption is not yet fully implemented.");

    Ok(())
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

/// Rebuild session index (placeholder - SQLite index not yet integrated)
async fn reindex(config: CliConfig) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions directory found.");
        return Ok(());
    }

    let ids = list_sessions(&sessions_path).await?;
    println!("Found {} sessions to index.", ids.len());

    // TODO: When SQLite index is integrated, rebuild it here
    println!("Note: Session indexing is not yet fully implemented.");
    println!("Sessions are currently stored as JSONL files without a central index.");

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
}
