//! `/api/session/{id}/command` — web slash-command execution.
//! Split from `session.rs` (file-size ceiling).

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub(super) struct ExecuteCommandRequest {
    command: String,
}

#[derive(Debug, Serialize)]
pub(super) struct CommandResponse {
    result: String,
    #[serde(rename = "type")]
    response_type: String,
}

/// One slash command, as both the `/help` text and the web autocomplete see it.
#[derive(Debug, Clone, Serialize)]
pub(super) struct SlashCommand {
    /// Bare name, no leading slash.
    pub name: &'static str,
    /// Argument placeholder shown in help/completion, empty when nullary.
    pub args: &'static str,
    pub description: &'static str,
}

/// The command set, declared once.
///
/// `/help` renders from this and `GET /api/commands` serves it, so the web
/// autocomplete can't drift from what `execute_command` actually accepts —
/// it previously hardcoded its own list and silently omitted `/models`.
pub(super) const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "help",
        args: "",
        description: "Show available commands",
    },
    SlashCommand {
        name: "search",
        args: "<query>",
        description: "Search sessions by title",
    },
    SlashCommand {
        name: "models",
        args: "",
        description: "List available models",
    },
    SlashCommand {
        name: "model",
        args: "<name>",
        description: "Switch to a different model",
    },
    SlashCommand {
        name: "clear",
        args: "",
        description: "Clear the chat view (server history preserved)",
    },
    SlashCommand {
        name: "export",
        args: "",
        description: "Export session to markdown",
    },
];

impl SlashCommand {
    /// `/search <query> — Search sessions by title`
    fn help_line(&self) -> String {
        let head = if self.args.is_empty() {
            format!("/{}", self.name)
        } else {
            format!("/{} {}", self.name, self.args)
        };
        format!("{} — {}", head, self.description)
    }
}

#[derive(Debug, Serialize)]
pub(super) struct CommandsResponse {
    commands: &'static [SlashCommand],
}

/// `GET /api/commands` — the slash commands the web composer can complete.
pub(super) async fn list_commands() -> Json<CommandsResponse> {
    Json(CommandsResponse {
        commands: SLASH_COMMANDS,
    })
}

/// The kiln set a `session.get` payload reports, as `session.search` wants it.
///
/// The **whole** set, because search scope is kiln-set overlap: a session on
/// `[A, B]` that searched with only `A` found nothing in a session on `[B]`
/// despite the two sharing a corpus. An empty set is passed through as empty —
/// a kiln-less session overlaps nothing, and the daemon answers accordingly.
fn session_scope_kilns(session: &serde_json::Value) -> Vec<PathBuf> {
    session
        .get("kilns")
        .and_then(|v| v.as_array())
        .map(|kilns| {
            kilns
                .iter()
                .filter_map(|v| v.as_str())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) async fn execute_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteCommandRequest>,
) -> Result<Json<CommandResponse>, WebError> {
    let raw = req.command.trim().to_string();
    let command_str = raw.strip_prefix('/').unwrap_or(&raw);
    let (cmd, args) = match command_str.split_once(' ') {
        Some((c, a)) => (c.trim(), a.trim()),
        None => (command_str.trim(), ""),
    };

    match cmd {
        "help" => {
            let help_text = SLASH_COMMANDS
                .iter()
                .map(SlashCommand::help_line)
                .collect::<Vec<_>>()
                .join("\n");
            Ok(Json(CommandResponse {
                result: help_text,
                response_type: "success".to_string(),
            }))
        }
        "search" => {
            if args.is_empty() {
                return Ok(Json(CommandResponse {
                    result: "Usage: /search <query>".to_string(),
                    response_type: "error".to_string(),
                }));
            }

            let session = state.daemon.session_get(&id).await.daemon_err()?;
            let results = state
                .daemon
                .session_search(args, &session_scope_kilns(&session), Some(10))
                .await
                .daemon_err()?;

            let result_text = if let Some(sessions) = results.as_array() {
                if sessions.is_empty() {
                    format!("No results found for '{}'", args)
                } else {
                    let mut lines = vec![format!(
                        "Search results for '{}' ({} found):",
                        args,
                        sessions.len()
                    )];
                    for (i, item) in sessions.iter().enumerate() {
                        let title = item
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Untitled");
                        let id_val = item
                            .get("session_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        lines.push(format!("  {}. {} ({})", i + 1, title, id_val));
                    }
                    lines.join("\n")
                }
            } else {
                format!("Search results for '{}':\n{}", args, results)
            };

            Ok(Json(CommandResponse {
                result: result_text,
                response_type: "success".to_string(),
            }))
        }
        "models" => {
            let models = state.daemon.session_list_models(&id).await.daemon_err()?;
            let result = if models.is_empty() {
                "No models available".to_string()
            } else {
                let mut lines = vec![format!("Available models ({}):", models.len())];
                for model in &models {
                    lines.push(format!("  • {}", model));
                }
                lines.join("\n")
            };
            Ok(Json(CommandResponse {
                result,
                response_type: "success".to_string(),
            }))
        }
        "model" => {
            if args.is_empty() {
                return Ok(Json(CommandResponse {
                    result: "Usage: /model <name>".to_string(),
                    response_type: "error".to_string(),
                }));
            }
            state
                .daemon
                .session_switch_model(&id, args)
                .await
                .daemon_err()?;
            Ok(Json(CommandResponse {
                result: format!("Switched model to {}", args),
                response_type: "success".to_string(),
            }))
        }
        // The frontend clears its local view on /clear; nothing is cleared
        // daemon-side. (TUI :clear ends + recreates the session — full parity
        // deliberately deferred; ACP sessions reject clear.) The response must
        // not overclaim.
        "clear" => Ok(Json(CommandResponse {
            result: "Chat view cleared (server-side history preserved)".to_string(),
            response_type: "success".to_string(),
        })),
        "export" => {
            // Return a hint — the actual export is handled by the existing export endpoint
            Ok(Json(CommandResponse {
                result: "Use the export dialog to download your session as markdown.".to_string(),
                response_type: "success".to_string(),
            }))
        }
        _ => Ok(Json(CommandResponse {
            result: format!(
                "Unknown command: /{}. Type /help for available commands.",
                cmd
            ),
            response_type: "error".to_string(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Search scope is kiln-set overlap, so `/search` has to hand the daemon
    /// every kiln the session reaches. Sending only the first tested a
    /// fraction of the caller's reach: a session on `[A, B]` found nothing in
    /// a session on `[B]`.
    #[test]
    fn search_scope_is_the_sessions_whole_kiln_set() {
        let session = serde_json::json!({ "kilns": ["/kilns/a", "/kilns/b"] });
        assert_eq!(
            session_scope_kilns(&session),
            vec![PathBuf::from("/kilns/a"), PathBuf::from("/kilns/b")]
        );
    }

    /// Zero kilns is a legitimate session shape (tools-only), not a missing
    /// value to substitute for: an empty scope overlaps nothing and the daemon
    /// answers with no matches.
    #[test]
    fn a_kiln_less_session_searches_with_an_empty_scope() {
        assert!(session_scope_kilns(&serde_json::json!({ "kilns": [] })).is_empty());
        assert!(session_scope_kilns(&serde_json::json!({})).is_empty());
    }
}
