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

use crucible_core::events::session_event::InternalSessionEvent;
use crucible_core::events::SessionEvent;
use crucible_core::protocol::session_events::{SessionEventPayload, SystemPayload};
use crucible_core::protocol::SessionEventMessage;
use crucible_lua::ScriptHandlerResult;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, warn};

/// The names a handler registers against, one per file event.
///
/// Spelled out rather than taken from `type_name()` alone so each is a
/// declaration `crucible.on`'s `HOOK_NAMES` contract test can see —
/// `hook_names_matches_every_dispatch_site` asserts in both directions, and a
/// name it cannot find in a dispatch site is reported as unreachable.
/// `the_dispatch_names_match_the_events` below pins them to the real
/// `type_name()`.
const FILE_CHANGED_EVENT: &str = "FileChanged";
const FILE_DELETED_EVENT: &str = "FileDeleted";
const FILE_MOVED_EVENT: &str = "FileMoved";

/// The handler-facing name for a file event.
fn dispatch_name(event: &InternalSessionEvent) -> Option<&'static str> {
    match event {
        InternalSessionEvent::FileChanged { .. } => Some(FILE_CHANGED_EVENT),
        InternalSessionEvent::FileDeleted { .. } => Some(FILE_DELETED_EVENT),
        InternalSessionEvent::FileMoved { .. } => Some(FILE_MOVED_EVENT),
        _ => None,
    }
}

/// Rebuild the typed event from its broadcast form.
///
/// The watcher's events reach subscribers already flattened to JSON, so the
/// typed value has to be reconstructed for handlers that expect the same shape
/// every other hook receives. Returns `None` for anything that is not a file
/// event.
///
/// Decoding the payload rather than digging keys out of it fixes two bugs at
/// once. It used to read `data["path"]` *before* matching the name, so
/// `file_moved` — which carries `from`/`to` and no `path` — could never reach a
/// handler. And `kind` used to be a two-arm string match that mapped everything
/// that was not `created` to `Modified`; `FileChangeKind` now decodes itself.
fn to_internal_event(msg: &SessionEventMessage) -> Option<InternalSessionEvent> {
    match msg.payload() {
        Ok(SessionEventPayload::System(SystemPayload::FileChanged { path, kind })) => {
            Some(InternalSessionEvent::FileChanged { path, kind })
        }
        Ok(SessionEventPayload::System(SystemPayload::FileDeleted { path })) => {
            Some(InternalSessionEvent::FileDeleted { path })
        }
        Ok(SessionEventPayload::System(SystemPayload::FileMoved { from, to })) => {
            Some(InternalSessionEvent::FileMoved { from, to })
        }
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
            let Some(event_name) = dispatch_name(&internal) else {
                continue;
            };
            let event = SessionEvent::internal(internal);

            // `runtime_handlers_for`, not `handlers_for`: the two are
            // different halves of the registry and only one of them is
            // written to. `crucible.on(...)` records into `runtime_handlers`;
            // `handlers` was the annotation-discovered vec, which nothing
            // populates any more. Reading the wrong half is why a
            // `crucible.on("FileChanged", ...)` handler had never once fired —
            // registered fine, matched nothing, silently.
            //
            // `None` for the identifier: a file event has no tool name, and a
            // handler that declared a `pattern` is asking to filter on one, so
            // it correctly does not match here.
            let matched = handlers.runtime_handlers_for(event_name, None);
            if matched.is_empty() {
                continue;
            }

            for handler in matched {
                match handlers
                    .execute_runtime_handler(&lua, &handler.name, &event, None)
                    .await
                {
                    // `Event Hooks.md`: a handler that cancels stops the chain.
                    // There is nothing to cancel here — the event already
                    // happened and was already broadcast — but the *chain* half
                    // of the contract still holds, and a handler asking to stop
                    // it should not be silently ignored just because this event
                    // class has no pipeline to abort.
                    Ok(ScriptHandlerResult::Cancel { reason }) => {
                        debug!(
                            handler = %handler.name,
                            reason = %reason,
                            "file event handler stopped the chain"
                        );
                        break;
                    }
                    // Transform and Handled have no meaning for an event that
                    // has already been broadcast: nothing downstream reads a
                    // rewritten value. Logged rather than dropped in silence,
                    // so an author who returns one finds out.
                    Ok(ScriptHandlerResult::Transform(_) | ScriptHandlerResult::Handled { .. }) => {
                        debug!(
                            handler = %handler.name,
                            "file event handler returned a value; file events are \
                             notifications, so it has no effect"
                        );
                    }
                    Ok(_) => {}
                    Err(e) => warn!(
                        handler = %handler.name,
                        error = %e,
                        "file event handler failed (continuing)"
                    ),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::session_event::FileChangeKind;
    use serde_json::json;
    use std::path::PathBuf;

    fn msg(event: &str, data: serde_json::Value) -> SessionEventMessage {
        SessionEventMessage::new("system", event, data)
    }

    #[test]
    fn a_file_changed_message_rebuilds_the_typed_event() {
        let m = msg(
            "file_changed",
            json!({ "path": "/w/a.md", "kind": "modified" }),
        );
        assert!(matches!(
            to_internal_event(&m),
            Some(InternalSessionEvent::FileChanged { .. })
        ));
    }

    #[test]
    fn the_created_kind_survives_the_round_trip() {
        let m = msg(
            "file_changed",
            json!({ "path": "/w/a.md", "kind": "created" }),
        );
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

    /// `to_internal_event` read `data["path"]` before matching the event name,
    /// but a `file_moved` payload carries `from`/`to` and no `path`
    /// (`file_watch_bridge.rs`). So `InternalSessionEvent::FileMoved` was
    /// broadcast and could never reach a Lua handler.
    #[test]
    fn a_file_moved_message_rebuilds_the_typed_event() {
        let m = msg("file_moved", json!({ "from": "/w/a.md", "to": "/w/b.md" }));
        assert!(matches!(
            to_internal_event(&m),
            Some(InternalSessionEvent::FileMoved { .. })
        ));
    }

    #[test]
    fn unrelated_events_are_ignored() {
        assert!(to_internal_event(&msg("message_complete", json!({}))).is_none());
        // A file event with no path is meaningless: the payload requires `path`,
        // so this is a malformed decode and no handler fires on an empty path.
        assert!(to_internal_event(&msg("file_changed", json!({}))).is_none());
    }

    /// A handler registered with `crucible.on` actually fires on a file event.
    ///
    /// Everything above this tests `to_internal_event` — the translation — and
    /// stops there, which is exactly how the dispatch stayed broken. The
    /// registry has two disjoint halves: `crucible.on` writes `runtime_handlers`,
    /// while this module read `handlers`, the annotation-discovered vec that
    /// nothing populates. Handlers registered, matched nothing, and never ran,
    /// with no error anywhere. This drives the real spawn end to end.
    #[tokio::test]
    async fn a_crucible_on_handler_fires_for_a_file_event() {
        use crucible_lua::LuaScriptHandlerRegistry;

        let lua = Arc::new(mlua::Lua::new());
        let registry = Arc::new(LuaScriptHandlerRegistry::new());
        crucible_lua::register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("register crucible.on");

        lua.load(
            r#"
            fired_path = nil
            crucible.on("FileChanged", function(ctx, event)
                fired_path = event.path
            end)
        "#,
        )
        .exec()
        .expect("register handler");

        assert_eq!(
            registry.runtime_handlers_for("FileChanged", None).len(),
            1,
            "the handler should be registered before any event is sent"
        );

        let (tx, rx) = broadcast::channel(8);
        spawn_file_event_hooks(rx, Arc::clone(&registry), Arc::clone(&lua));

        tx.send(msg(
            "file_changed",
            json!({ "path": "/w/a.md", "kind": "modified" }),
        ))
        .expect("send");

        // Poll rather than sleep a fixed interval: the dispatch is a spawned
        // task and a fixed wait is either flaky or slow.
        let mut fired: Option<String> = None;
        for _ in 0..200 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            fired = lua.globals().get("fired_path").ok().flatten();
            if fired.is_some() {
                break;
            }
        }

        assert_eq!(
            fired.as_deref(),
            Some("/w/a.md"),
            "a crucible.on(\"FileChanged\") handler never ran"
        );
    }

    /// The dispatch constants are the real `type_name()`s.
    ///
    /// They exist as constants so `crucible.on`'s HOOK_NAMES contract test can
    /// see them, which means they could drift from the events they name and
    /// nothing else would notice — handlers would register against a name that
    /// matches no dispatch.
    #[test]
    fn the_dispatch_names_match_the_events() {
        for (event, expected) in [
            (
                InternalSessionEvent::FileChanged {
                    path: PathBuf::from("/w/a.md"),
                    kind: FileChangeKind::Modified,
                },
                FILE_CHANGED_EVENT,
            ),
            (
                InternalSessionEvent::FileDeleted {
                    path: PathBuf::from("/w/a.md"),
                },
                FILE_DELETED_EVENT,
            ),
            (
                InternalSessionEvent::FileMoved {
                    from: PathBuf::from("/w/a.md"),
                    to: PathBuf::from("/w/b.md"),
                },
                FILE_MOVED_EVENT,
            ),
        ] {
            assert_eq!(dispatch_name(&event), Some(expected));
            assert_eq!(
                SessionEvent::internal(event).type_name(),
                expected,
                "the constant must be the name the registry matches on"
            );
        }
    }

    /// Every name this module dispatches is one `crucible.on` accepts.
    ///
    /// The two lists live in different crates. When they disagreed, the whole
    /// feature was dead: registration raised "unknown event `FileChanged`" and
    /// dispatch read a vec nothing wrote to.
    #[test]
    fn every_dispatched_name_is_registerable() {
        for name in [FILE_CHANGED_EVENT, FILE_DELETED_EVENT, FILE_MOVED_EVENT] {
            assert!(
                crucible_lua::HOOK_NAMES.contains(&name),
                "`{name}` is dispatched here but `crucible.on` would reject it"
            );
        }
    }
}
