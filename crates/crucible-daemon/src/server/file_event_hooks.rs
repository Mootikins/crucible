//! Dispatching file-watch events to Lua handlers.
//!
//! Every hookable event before this one sat on the agent turn loop —
//! `pre_llm_call`, `tool_result`, `turn:complete` and friends. The knowledge
//! side had none, so a handler could not react to the workspace changing at all.
//!
//! That gap is what made "show git status in the statusline" impossible to write
//! honestly: the value has to be recomputed when files change, and the only
//! trigger available was a polling loop. The daemon already ran a file watcher
//! and already broadcast its events to RPC subscribers; Lua simply could not
//! subscribe. This connects the two.
//!
//! Matching and conversion already worked — `InternalSessionEvent::FileChanged`
//! reports `type_name() == "FileChanged"`, which the handler registry matches on
//! and `session_event_to_lua` converts. Only the dispatch was missing.

use crucible_core::events::session_event::{FileChangeKind, InternalSessionEvent};
use crucible_core::events::SessionEvent;
use crucible_core::protocol::SessionEventMessage;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, warn};

/// Rebuild the typed event from its broadcast form.
///
/// The watcher's events reach subscribers already flattened to JSON, so the
/// typed value has to be reconstructed for handlers that expect the same shape
/// every other hook receives. Returns `None` for anything that is not a file
/// event.
fn to_internal_event(msg: &SessionEventMessage) -> Option<InternalSessionEvent> {
    let path = PathBuf::from(msg.data.get("path")?.as_str()?);
    match msg.event.as_str() {
        "file_changed" => {
            let kind = match msg.data.get("kind").and_then(|k| k.as_str()) {
                Some("created") | Some("Created") => FileChangeKind::Created,
                _ => FileChangeKind::Modified,
            };
            Some(InternalSessionEvent::FileChanged { path, kind })
        }
        "file_deleted" => Some(InternalSessionEvent::FileDeleted { path }),
        _ => None,
    }
}

/// Subscribe to the event bus and run matching Lua handlers for file events.
///
/// Fail-open throughout: a handler that errors is logged and the next one still
/// runs. Nothing here is a gate, so a broken handler must not stop the watcher
/// or the rest of the daemon.
pub fn spawn_file_event_hooks(
    mut rx: broadcast::Receiver<SessionEventMessage>,
    handlers: Arc<crucible_lua::LuaScriptHandlerRegistry>,
    lua: Arc<mlua::Lua>,
) {
    tokio::spawn(async move {
        loop {
            let msg = match rx.recv().await {
                Ok(msg) => msg,
                // A slow consumer misses events rather than stalling the bus.
                // Dropping is correct here: these are triggers, not a ledger,
                // and the next change re-triggers anyway.
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!("file event hooks lagged {n} events");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };

            let Some(internal) = to_internal_event(&msg) else {
                continue;
            };
            let event = SessionEvent::internal(internal);

            let matched = handlers.handlers_for(&event);
            if matched.is_empty() {
                continue;
            }

            if let Err(e) = crucible_lua::run_handler_chain(&lua, &matched, event) {
                warn!(error = %e, "file event handler failed (continuing)");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(event: &str, data: serde_json::Value) -> SessionEventMessage {
        SessionEventMessage::new("system", event, data)
    }

    #[test]
    fn a_file_changed_message_rebuilds_the_typed_event() {
        let m = msg("file_changed", json!({ "path": "/w/a.md", "kind": "modified" }));
        assert!(matches!(
            to_internal_event(&m),
            Some(InternalSessionEvent::FileChanged { .. })
        ));
    }

    #[test]
    fn the_created_kind_survives_the_round_trip() {
        let m = msg("file_changed", json!({ "path": "/w/a.md", "kind": "created" }));
        assert!(matches!(
            to_internal_event(&m),
            Some(InternalSessionEvent::FileChanged {
                kind: FileChangeKind::Created,
                ..
            })
        ));
    }

    #[test]
    fn a_deletion_rebuilds_as_a_deletion() {
        let m = msg("file_deleted", json!({ "path": "/w/a.md" }));
        assert!(matches!(
            to_internal_event(&m),
            Some(InternalSessionEvent::FileDeleted { .. })
        ));
    }

    /// The registry matches on `type_name`, so this pins the string a handler
    /// has to register against. Changing it silently unhooks every config.
    #[test]
    fn the_handler_facing_event_name_is_file_changed() {
        let event = SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::from("/w/a.md"),
            kind: FileChangeKind::Modified,
        });
        assert_eq!(event.type_name(), "FileChanged");
    }

    #[test]
    fn unrelated_events_are_ignored() {
        assert!(to_internal_event(&msg("message_complete", json!({}))).is_none());
        assert!(to_internal_event(&msg("file_changed", json!({}))).is_none());
    }
}
