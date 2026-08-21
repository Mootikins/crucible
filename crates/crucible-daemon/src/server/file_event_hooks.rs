//! Dispatching daemon broadcast events to Lua handlers.
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
//! The file name is historical: this dispatches **every** event with a row in
//! [`crate::event_map`], not only the three file-watch ones it started with. It
//! keeps the name because renaming it would drag `server/mod.rs` and every
//! `::test_name` citation in `docs/Meta/Product.md` along for no gain.
//!
//! Matching, conversion and the identifier `opts.pattern` filters on all come
//! from that one table, which the outbound bridge (`file_watch_bridge.rs`)
//! consults too. This module owns the loop and the fail-open policy; it owns no
//! list of event names.

use crucible_core::protocol::SessionEventMessage;
use crucible_lua::EventOutcome;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::event_map::{self, HookedEvent};

/// Subscribe to the event bus and run matching Lua handlers.
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

            let Some(HookedEvent {
                hook,
                event,
                identifier,
            }) = event_map::decode(&msg)
            else {
                continue;
            };

            // `runtime_handlers_for`, not `handlers_for`: the two are
            // different halves of the registry and only one of them is
            // written to. `crucible.on(...)` records into `runtime_handlers`;
            // `handlers` was the annotation-discovered vec, which nothing
            // populates any more. Reading the wrong half is why a
            // `crucible.on("FileChanged", ...)` handler had never once fired —
            // registered fine, matched nothing, silently.
            //
            // The identifier is what `opts.pattern` globs against: the note
            // path for a note event, the webhook name for a delivery, and
            // `None` for a file event, which carries no identifier — so a
            // handler that declared a pattern on one of those is asking to
            // filter on something that does not exist, and correctly does not
            // match.
            let matched = handlers.runtime_handlers_for(hook.as_str(), identifier.as_deref());
            if matched.is_empty() {
                continue;
            }

            for handler in matched {
                match handlers
                    .execute_runtime_handler(&lua, &handler.name, &event, None)
                    .await
                {
                    Ok(result) => {
                        // Narrowed at the boundary: an event has already
                        // happened and already been broadcast, so `Transform`,
                        // `Inject` and `Handled` cannot apply. This loop used to
                        // carry an arm that logged and dropped them; now the
                        // type says they are not outcomes here, and the closure
                        // reports anything a handler asked for that cannot.
                        let outcome = result.into_event_outcome(&mut |dropped| {
                            debug!(
                                handler = %handler.name,
                                hook = %hook,
                                dropped = dropped,
                                "daemon event handler returned something an event cannot act on"
                            );
                        });
                        match outcome {
                            EventOutcome::Observed => {}
                            EventOutcome::StopChain { reason } => {
                                debug!(
                                    handler = %handler.name,
                                    reason = %reason,
                                    "daemon event handler stopped the chain"
                                );
                                break;
                            }
                        }
                    }
                    Err(e) => warn!(
                        handler = %handler.name,
                        error = %e,
                        "daemon event handler failed (continuing)"
                    ),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::session_event::{FileChangeKind, InternalSessionEvent};
    use crucible_core::events::SessionEvent;
    use crucible_lua::LuaScriptHandlerRegistry;
    use serde_json::json;
    use std::path::PathBuf;

    fn msg(event: &str, data: serde_json::Value) -> SessionEventMessage {
        SessionEventMessage::new("system", event, data)
    }

    /// Register one `crucible.on` handler, put `sent` on the bus, and return
    /// the global the handler set — or `None` if it never ran.
    ///
    /// Drives the real spawn. Everything short of this tests the translation
    /// and stops there, which is exactly how the dispatch stayed broken: the
    /// registry has two disjoint halves, `crucible.on` writes one and this
    /// module used to read the other.
    ///
    /// A caller expecting `None` must use [`dispatch_expecting_silence`]
    /// instead — waiting the whole timeout out is the only way this one can
    /// return it.
    async fn dispatch(hook: &str, body: &str, sent: SessionEventMessage) -> Option<String> {
        let (lua, tx) = spawn_with_handler(hook, body);
        tx.send(sent).expect("send");

        // Poll rather than sleep a fixed interval: the dispatch is a spawned
        // task and a fixed wait is either flaky or slow.
        for _ in 0..200 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            if let Ok(Some(fired)) = lua.globals().get::<Option<String>>("fired") {
                return Some(fired);
            }
        }
        None
    }

    /// Assert that `sent` fires nothing, without waiting a timeout out.
    ///
    /// A negative dispatch has no event to wait for, so [`dispatch`] can only
    /// conclude "never fired" by exhausting its poll budget — a flat second of
    /// wall clock on every run, for a test whose answer is known in
    /// microseconds. This sends a second event a *different* handler does fire
    /// on, and reads `fired` once that one lands. The dispatch loop takes one
    /// message at a time in order, so the barrier arriving proves `sent` was
    /// already processed and dropped.
    async fn dispatch_expecting_silence(hook: &str, body: &str, sent: SessionEventMessage) {
        let (lua, tx) = spawn_with_handler(hook, body);
        lua.load("barrier = nil\ncrucible.on(\"webhook:received\", function() barrier = true end)")
            .exec()
            .expect("register barrier handler");

        tx.send(sent).expect("send");
        tx.send(event_map::webhook_received(
            "barrier".into(),
            serde_json::Map::new(),
            "{}".into(),
        ))
        .expect("send barrier");

        for _ in 0..200 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            if lua.globals().get::<Option<bool>>("barrier").ok().flatten() == Some(true) {
                assert_eq!(
                    lua.globals().get::<Option<String>>("fired").ok().flatten(),
                    None,
                    "the handler ran on an event it should not have matched"
                );
                return;
            }
        }
        panic!("the barrier event never dispatched, so the test proved nothing");
    }

    /// A Lua VM with `crucible.on` registered, one handler loaded, and the
    /// dispatch task running against a fresh bus.
    fn spawn_with_handler(
        hook: &str,
        body: &str,
    ) -> (Arc<mlua::Lua>, broadcast::Sender<SessionEventMessage>) {
        let lua = Arc::new(mlua::Lua::new());
        let registry = Arc::new(LuaScriptHandlerRegistry::new());
        crucible_lua::register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("register crucible.on");

        lua.load(format!("fired = nil\ncrucible.on(\"{hook}\", {body})"))
            .exec()
            .expect("register handler");

        let (tx, rx) = broadcast::channel(8);
        spawn_file_event_hooks(rx, registry, Arc::clone(&lua));
        (lua, tx)
    }

    /// A handler registered with `crucible.on` actually fires on a file event.
    #[tokio::test]
    async fn a_crucible_on_handler_fires_for_a_file_event() {
        let fired = dispatch(
            "FileChanged",
            "function(ctx, event) fired = event.path end",
            msg(
                "file_changed",
                json!({ "path": "/w/a.md", "kind": "modified" }),
            ),
        )
        .await;
        assert_eq!(
            fired.as_deref(),
            Some("/w/a.md"),
            "a crucible.on(\"FileChanged\") handler never ran"
        );
    }

    /// The reason this module was generalised: the note pipeline's events now
    /// reach Lua by the same route, with the hook name the author registered
    /// visible on the event.
    #[tokio::test]
    async fn a_note_event_reaches_a_handler() {
        let sent = event_map::message_for(&InternalSessionEvent::NoteCreated {
            path: PathBuf::from("Daily/2026-08-18.md"),
            title: Some("Today".into()),
        })
        .expect("note:created has a wire form");

        let fired = dispatch(
            "note:created",
            "function(ctx, event) fired = event.type .. \" \" .. event.path end",
            sent,
        )
        .await;
        assert_eq!(
            fired.as_deref(),
            Some("note:created Daily/2026-08-18.md"),
            "a crucible.on(\"note:created\") handler never ran"
        );
    }

    /// `webhook:received` has been broadcast to nobody since the ingress
    /// shipped: `POST /api/webhook/{name}` put it on this very bus and the
    /// dispatch dropped it, so the documented GitHub/IFTTT integrations rested
    /// on a dead half.
    #[tokio::test]
    async fn a_webhook_delivery_reaches_a_handler() {
        let sent = event_map::webhook_received(
            "ci".into(),
            serde_json::Map::new(),
            r#"{"event":"push"}"#.into(),
        );
        let fired = dispatch(
            "webhook:received",
            "function(ctx, event) fired = event.name .. \" \" .. event.body end",
            sent,
        )
        .await;
        assert_eq!(
            fired.as_deref(),
            Some(r#"ci {"event":"push"}"#),
            "a crucible.on(\"webhook:received\") handler never ran"
        );
    }

    /// A handler that narrowed with `opts.pattern` is filtered on the event's
    /// identifier, end to end through the real spawn.
    #[tokio::test]
    async fn a_pattern_that_does_not_cover_the_note_never_fires() {
        let sent = event_map::message_for(&InternalSessionEvent::NoteModified {
            path: PathBuf::from("Meta/Design.md"),
            change_type: crucible_core::events::NoteChangeType::Content,
        })
        .expect("note:modified has a wire form");

        dispatch_expecting_silence(
            "note:modified",
            "{ pattern = \"Daily/*\" }, function(ctx, event) fired = event.path end",
            sent,
        )
        .await;
    }

    /// The dispatch constants are the real `type_name()`s.
    ///
    /// The file events reach Lua as their typed internal event, so their hook
    /// name and their `type_name()` have to agree — a handler registers against
    /// one and reads the other off the event.
    #[test]
    fn the_dispatch_names_match_the_events() {
        for (event, expected) in [
            (
                InternalSessionEvent::FileChanged {
                    path: PathBuf::from("/w/a.md"),
                    kind: FileChangeKind::Modified,
                },
                event_map::FILE_CHANGED_EVENT,
            ),
            (
                InternalSessionEvent::FileDeleted {
                    path: PathBuf::from("/w/a.md"),
                },
                event_map::FILE_DELETED_EVENT,
            ),
            (
                InternalSessionEvent::FileMoved {
                    from: PathBuf::from("/w/a.md"),
                    to: PathBuf::from("/w/b.md"),
                },
                event_map::FILE_MOVED_EVENT,
            ),
        ] {
            assert_eq!(
                SessionEvent::internal(event).type_name(),
                expected,
                "the constant must be the name the registry matches on"
            );
        }
    }
}
