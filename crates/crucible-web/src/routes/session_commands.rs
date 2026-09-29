//! `/api/session/{id}/commands` and `/api/session/{id}/command` — the web
//! client's slash commands.
//!
//! The daemon owns the command catalog of a session. The composer completes
//! from it. A built-in command is the client's own action, so this route
//! runs it; every other command goes to the daemon as a chat message, which
//! the daemon routes.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use crucible_core::types::{split_slash_command, BuiltinCommand, CommandKind, SessionCommand};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub(super) struct ExecuteCommandRequest {
    /// The command line, with or without its leading slash.
    command: String,
}

/// What one built-in command produced.
///
/// A command used wrongly comes back here with `type` reading `error`: the
/// composer prints the text either way, and it is not a transport failure.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct CommandResponse {
    /// The text the composer prints.
    result: String,
    /// `success` or `error`.
    #[serde(rename = "type")]
    response_type: String,
    /// The session the browser opens, for `/resume <id>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    open_session: Option<String>,
}

impl CommandResponse {
    fn success(result: impl Into<String>) -> Json<Self> {
        Json(Self {
            result: result.into(),
            response_type: "success".to_string(),
            open_session: None,
        })
    }

    fn error(result: impl Into<String>) -> Json<Self> {
        Json(Self {
            result: result.into(),
            response_type: "error".to_string(),
            open_session: None,
        })
    }
}

/// The command catalog of one session.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct CommandsResponse {
    commands: Vec<SessionCommand>,
}

/// `GET /api/session/{id}/commands` — the commands the composer completes.
///
/// The daemon answers the catalog of the session: built-in, mode, plugin,
/// skill and agent commands. A `commands_changed` event on the session's
/// stream says when to ask again.
#[utoipa::path(
    get,
    path = "/api/session/{id}/commands",
    params(("id" = String, Path, description = "The session whose commands to list")),
    responses(
        (status = 200, body = CommandsResponse),
        (status = 502, description = "The daemon could not list the commands"),
    )
)]
pub(super) async fn list_commands(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CommandsResponse>, WebError> {
    let commands = state.daemon.session_commands(&id).await.daemon_err()?;
    Ok(Json(CommandsResponse { commands }))
}

/// `/name hint — description (source)`, one line of `/help`.
fn help_line(command: &SessionCommand) -> String {
    let head = match &command.input_hint {
        Some(hint) => format!("/{} {}", command.name, hint),
        None => format!("/{}", command.name),
    };
    let source = match &command.kind {
        CommandKind::Builtin { .. } => None,
        CommandKind::Mode { .. } => Some("mode"),
        CommandKind::Plugin { .. } => Some("plugin"),
        CommandKind::Skill => Some("skill"),
        CommandKind::Agent => Some("agent"),
    };
    match source {
        Some(source) => format!("{head} — {} ({source})", command.description),
        None => format!("{head} — {}", command.description),
    }
}

/// Run one built-in command in a session.
///
/// The composer sends only built-in commands here. Any other command is a
/// chat message, so this route refuses it with an `error` reply.
#[utoipa::path(
    post,
    path = "/api/session/{id}/command",
    params(("id" = String, Path, description = "The session to run the command in")),
    request_body = ExecuteCommandRequest,
    responses(
        (status = 200, body = CommandResponse),
        (status = 502, description = "The daemon could not serve the command"),
    )
)]
pub(super) async fn execute_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteCommandRequest>,
) -> Result<Json<CommandResponse>, WebError> {
    let raw = req.command.trim();
    let line = if raw.starts_with('/') {
        raw.to_string()
    } else {
        format!("/{raw}")
    };
    let Some((name, args)) = split_slash_command(&line) else {
        return Ok(CommandResponse::error("Type /help for the commands."));
    };
    let Some(command) = BuiltinCommand::from_name(name) else {
        return Ok(CommandResponse::error(format!(
            "/{name} is not a built-in command. Send it as a message."
        )));
    };

    match command {
        BuiltinCommand::Help => {
            let commands = state.daemon.session_commands(&id).await.daemon_err()?;
            Ok(CommandResponse::success(
                commands
                    .iter()
                    .map(help_line)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ))
        }
        BuiltinCommand::Search => {
            if args.is_empty() {
                return Ok(CommandResponse::error("Usage: /search <query>"));
            }
            // Search scope is kiln-set overlap, so `/search` has to hand the
            // daemon every kiln the session reaches: the **whole** set. A
            // session on `[A, B]` searched with only `A` found nothing in a
            // session on `[B]` despite the two sharing a corpus. An empty set
            // is passed through as empty — a kiln-less session overlaps
            // nothing, and the daemon answers accordingly.
            let session = state.daemon.session_get(&id).await.daemon_err()?;
            let found = state
                .daemon
                .session_search(args, &session.kilns, Some(10))
                .await
                .daemon_err()?;
            Ok(CommandResponse::success(found.to_text(args)))
        }
        // No name lists the models; a name switches to it.
        BuiltinCommand::Model if args.is_empty() => {
            let models = state.daemon.session_list_models(&id).await.daemon_err()?;
            if models.is_empty() {
                return Ok(CommandResponse::success("No models available"));
            }
            let mut lines = vec![format!("Available models ({}):", models.len())];
            lines.extend(models.iter().map(|model| format!("  • {model}")));
            Ok(CommandResponse::success(lines.join("\n")))
        }
        BuiltinCommand::Model => {
            state
                .daemon
                .session_switch_model(&id, args)
                .await
                .daemon_err()?;
            Ok(CommandResponse::success(format!(
                "Switched model to {args}"
            )))
        }
        BuiltinCommand::Mode => {
            let modes = state.daemon.session_list_modes(&id).await.daemon_err()?;
            let Some(next) = modes.next_mode() else {
                return Ok(CommandResponse::error("This session has no next mode."));
            };
            state
                .daemon
                .session_set_mode(&id, next)
                .await
                .daemon_err()?;
            Ok(CommandResponse::success(format!("Mode: {next}")))
        }
        // `session.clear`, the same user clear as the TUI's. The transcript
        // keeps the history, and the daemon's `context_cleared` draws the
        // divider.
        BuiltinCommand::Clear => {
            state.daemon.session_clear(&id).await.daemon_err()?;
            Ok(CommandResponse::success("Context cleared"))
        }
        BuiltinCommand::Undo => {
            let Ok(count) = (if args.is_empty() {
                Ok(1)
            } else {
                args.parse::<usize>()
            }) else {
                return Ok(CommandResponse::error("Usage: /undo [turns]"));
            };
            let undone = state
                .daemon
                .session_undo(&id, count.max(1))
                .await
                .daemon_err()?;
            Ok(CommandResponse::success(format!(
                "Undid {} turn(s)",
                undone.len()
            )))
        }
        BuiltinCommand::Resume if args.is_empty() => Ok(CommandResponse::success(
            "Choose a session in the session list, or type /resume <session>.",
        )),
        BuiltinCommand::Resume => Ok(Json(CommandResponse {
            result: format!("Opening {args}"),
            response_type: "success".to_string(),
            open_session: Some(args.to_string()),
        })),
        // The export endpoint downloads the file; the dialog calls it.
        BuiltinCommand::Export => Ok(CommandResponse::success(
            "Use the export dialog to download your session as markdown.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::request_json;

    /// `/search` used to call `.as_array()` on the daemon's `{matches, total}`
    /// object — always `None`, since the reply is an object, not an array —
    /// and fall through to printing the whole object as raw JSON. It also
    /// read a `title` key no match carries. Two matches must read back as two
    /// human lines, not a JSON blob.
    #[tokio::test]
    async fn a_search_reply_with_two_matches_prints_two_readable_lines() {
        let (status, json) = request_json(
            "POST",
            "/api/session/test-session-001/command",
            Some(serde_json::json!({ "command": "/search two hits" })),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "{json}");

        let reply: CommandResponse =
            serde_json::from_value(json.clone()).expect("the reply reads back as its own struct");
        assert_eq!(reply.response_type, "success");
        assert!(
            !reply.result.trim_start().starts_with('{'),
            "the result must be readable lines, not the raw `{{matches, total}}` object: {}",
            reply.result
        );
        let lines: Vec<&str> = reply.result.lines().collect();
        assert_eq!(
            lines.len(),
            3,
            "a header line plus one line per match: {}",
            reply.result
        );
        assert!(lines[1].contains("s1"), "{}", reply.result);
        assert!(lines[1].contains("Test Session one"), "{}", reply.result);
        assert!(lines[2].contains("s2"), "{}", reply.result);
        assert!(lines[2].contains("Test Session two"), "{}", reply.result);
    }

    // `session_scope_kilns` used to read `session["kilns"]` off an untyped
    // `serde_json::Value` and filter out a path-shaped or malformed entry —
    // the daemon's own reply, so a filtered entry could only mean protocol
    // drift. `session.get` now answers the typed `SessionSummary`, whose
    // `kilns: Vec<KilnName>` is valid by construction: a value that failed
    // to parse as a `KilnName` fails to deserialize the whole reply, rather
    // than silently dropping out of the search scope. `/search` reads
    // `session.kilns` directly (see `execute_command` above); the three
    // tests that lived here — the whole-set case, the path-filter case, and
    // the kiln-less case — tested a filter that no longer exists.
}
