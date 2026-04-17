use super::helpers::truncate;
use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use crucible_daemon::LogEvent;
use std::path::PathBuf;
use tokio::fs;

pub(super) fn sessions_dir(config: &CliConfig) -> PathBuf {
    config.kiln_path.join(".crucible").join("sessions")
}

pub(super) async fn read_session_events(session_dir: &std::path::Path) -> Result<Vec<LogEvent>> {
    let jsonl_path = session_dir.join("session.jsonl");
    let content = fs::read_to_string(&jsonl_path)
        .await
        .map_err(|e| anyhow!("Failed to read session events: {}", e))?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<LogEvent>(line).ok())
        .collect())
}

pub(super) async fn list_session_dirs(sessions_path: &std::path::Path) -> Result<Vec<String>> {
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

pub(super) fn format_events_markdown(events: &[LogEvent], _include_timestamps: bool) -> String {
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

pub(super) fn display_events_text(id: &str, events: &[LogEvent]) {
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
