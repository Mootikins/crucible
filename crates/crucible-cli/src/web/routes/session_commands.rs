//! `/api/session/{id}/command` — web slash-command execution.
//! Split from `session.rs` (file-size ceiling).

use crate::web::services::daemon::AppState;
use crate::web::{error::WebResultExt, WebError};
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
            let help_text = [
                "/help — Show available commands",
                "/search <query> — Search sessions by title",
                "/models — List available models",
                "/clear — Clear the chat view (server history preserved)",
                "/export — Export session to markdown",
                "/model <name> — Switch to a different model",
            ]
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

            // Get session to find kiln path
            let session = state.daemon.session_get(&id).await.daemon_err()?;
            let kiln_str = session.get("kiln").and_then(|v| v.as_str()).unwrap_or("");

            let kiln_path = if kiln_str.is_empty() {
                None
            } else {
                Some(PathBuf::from(kiln_str))
            };

            let results = state
                .daemon
                .session_search(args, kiln_path.as_deref(), Some(10))
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
