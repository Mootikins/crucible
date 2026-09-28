use crate::config::CliAppConfig;
use anyhow::{anyhow, Result};
use crucible_core::text::truncate_chars;
use crucible_core::transcript::{DelegationStatus, ItemBody, Notice, ToolStatus, Transcript};
use crucible_daemon::FileSessionStorage;
use std::path::PathBuf;
use tokio::fs;

/// Where the daemon keeps session directories, for the CLI's
/// daemon-unreachable fallbacks.
///
/// Sessions live under the daemon's data root, not inside a kiln, so this
/// mirrors `FileSessionStorage::root_for` rather than spelling the layout a
/// second time. `data_home` is read off the config first so a relocated daemon
/// root is honored without an env var; `crucible_home()` is the same default
/// the daemon resolves when the config says nothing.
pub(crate) fn sessions_dir(config: &CliAppConfig) -> PathBuf {
    let data_home = config
        .data_home
        .clone()
        .unwrap_or_else(crucible_core::config::crucible_home);
    FileSessionStorage::root_for(&data_home)
}

/// The transcript of a stored session, for when no daemon can start.
///
/// The daemon's own fold reads the file (`crucible_daemon::load_transcript`),
/// so the offline view and the RPC view come from one fold.
pub(super) async fn read_transcript(session_dir: &std::path::Path) -> Result<Transcript> {
    crucible_daemon::load_transcript(session_dir)
        .await
        .map_err(|e| anyhow!("Failed to read session events: {}", e))
}

/// The transcript in the `transcript` field of a `session.history` answer.
pub(super) async fn history_transcript(
    client: &crucible_daemon::DaemonClient,
    session_id: &str,
) -> Result<Transcript> {
    // No page of raw events: the transcript is the fold of the whole log.
    let history = client.session_history(session_id, Some(0), None).await?;
    let transcript = history
        .get("transcript")
        .cloned()
        .ok_or_else(|| anyhow!("session.history answered no transcript"))?;
    Ok(serde_json::from_value(transcript)?)
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

/// The plain-text view of `cru session show`: one short line for each item,
/// and the whole text of each turn.
pub(super) fn transcript_text(id: &str, transcript: &Transcript) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "Session: {}\n", id);
    let _ = writeln!(out, "Items: {}\n", transcript.items.len());

    for item in &transcript.items {
        match &item.body {
            ItemBody::UserTurn {
                content,
                precognition,
                ..
            } => {
                let _ = writeln!(out, "\n[user]\n{}\n", content);
                if let Some(p) = precognition {
                    let text = format!(
                        "Context injected: {} note(s) for \"{}\"",
                        p.notes_count, p.query_summary
                    );
                    let _ = writeln!(out, "[system] {}", truncate_chars(&text, 100, true));
                }
            }
            ItemBody::AssistantSegment {
                text,
                thinking,
                model,
                ..
            } => {
                if !thinking.is_empty() {
                    let _ = writeln!(out, "[thinking] {}", truncate_chars(thinking, 100, true));
                }
                if !text.is_empty() {
                    let model = model.as_deref().unwrap_or("unknown");
                    let _ = writeln!(out, "[assistant ({})]\n{}\n", model, text);
                }
            }
            ItemBody::ToolCard {
                call_id,
                name,
                status,
                ..
            } => {
                let _ = writeln!(out, "[tool:{}] id={}", name, call_id);
                match status {
                    ToolStatus::Complete => {
                        let _ = writeln!(out, "[result:{}]", call_id);
                    }
                    ToolStatus::Failed => {
                        let _ = writeln!(out, "[result:{}] (error)", call_id);
                    }
                    ToolStatus::Running | ToolStatus::Incomplete => {}
                }
            }
            ItemBody::Delegation {
                delegation_id,
                prompt,
                status,
                outcome,
                ..
            } => {
                let outcome = outcome.as_deref().unwrap_or_default();
                let _ = match status {
                    DelegationStatus::Running => writeln!(
                        out,
                        "[subagent:{}] {}",
                        delegation_id,
                        truncate_chars(prompt, 60, true)
                    ),
                    DelegationStatus::Complete => writeln!(
                        out,
                        "[subagent:{}] -> {}",
                        delegation_id,
                        truncate_chars(outcome, 60, true)
                    ),
                    DelegationStatus::Failed => writeln!(
                        out,
                        "[subagent:{}] FAILED: {}",
                        delegation_id,
                        truncate_chars(outcome, 60, true)
                    ),
                };
            }
            ItemBody::InjectedContext { role, content, .. } => {
                let _ = match role.as_str() {
                    "user" => writeln!(out, "\n[user]\n{}\n", content),
                    "assistant" => writeln!(out, "[assistant (unknown)]\n{}\n", content),
                    _ => writeln!(out, "[system] {}", truncate_chars(content, 100, true)),
                };
            }
            ItemBody::Notice { notice } => {
                let _ = match notice {
                    Notice::ContextCleared { plugin } => writeln!(
                        out,
                        "[context cleared by {}]",
                        plugin.as_deref().unwrap_or("user")
                    ),
                    Notice::StopReason { text, .. } => writeln!(out, "[notice] {}", text),
                    Notice::TurnFailed { status, error } => match error {
                        Some(error) => writeln!(out, "[error] {}", error),
                        None => writeln!(out, "[error] {:?}", status),
                    },
                };
            }
        }
    }
    out
}
