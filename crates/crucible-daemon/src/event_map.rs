//! The one table from a daemon event to the name a Lua handler registers for.
//!
//! Two bridges carry daemon events, and until this module they each carried
//! their own hand-written list of three:
//!
//! - `file_watch_bridge.rs` turns an [`InternalSessionEvent`] into the
//!   `SessionEventMessage` clients subscribe to (**outbound**).
//! - `server/file_event_hooks.rs` reads that same broadcast and runs matching
//!   Lua handlers (**inbound**).
//!
//! Nothing tied the two lists together, so every event the daemon emits that
//! was not one of those three reached Lua not at all — including
//! `webhook:received`, which the webhook ingress has broadcast to nobody since
//! it shipped. Adding an event meant editing two `match` arms in two files and
//! a third list in another crate, and forgetting any one of them failed
//! silently.
//!
//! [`ROWS`] is that single list. A new event is a row plus its
//! [`HOOK_NAMES`](crucible_lua::HOOK_NAMES) entry, and the tests below prove
//! the two agree in both directions.
//!
//! # The three columns
//!
//! | Column | Meaning |
//! |---|---|
//! | `wire` | `SessionEventMessage.event` — the name on the daemon's broadcast bus, which clients also see |
//! | `hook` | the name `crucible.on(...)` registers against, and the `type` field the handler reads off the event |
//! | `identifier` | the `data` key `opts.pattern` globs against, when the event has one |
//!
//! `wire` and `hook` differ only for the file events, whose hook names are
//! their Rust `type_name()`s (`FileChanged`) and predate this table; renaming
//! them would unhook every config that already registers one. Everything added
//! since is colon-namespaced and spelled the same on both sides.
//!
//! # What a handler sees
//!
//! `event.type` is always the hook name the author registered — that is the
//! invariant `the_handler_sees_the_name_it_registered` pins. The file events
//! reach Lua as their typed [`InternalSessionEvent`], as they always have; the
//! rest arrive as [`SessionEvent::Custom`] carrying the wire `data` verbatim,
//! which the Lua projection flattens to the top level. Either way the payload
//! keys are the `data` keys.

use crucible_core::events::session_event::InternalSessionEvent;
use crucible_core::events::SessionEvent;
use crucible_core::protocol::session_events::{SessionEventPayload, SystemPayload};
use crucible_core::protocol::SessionEventMessage;
use crucible_lua::EventName;

/// The session id every daemon-wide event is addressed to.
///
/// Not a session: these events belong to the daemon, not a conversation. The
/// file watcher, the kiln manager and the classification prompt all already use
/// it, and the reprocess task in `server/mod.rs` filters on it.
pub const SYSTEM_SESSION: &str = "system";

/// The session id the webhook ingress addresses its deliveries to.
pub const WEBHOOK_SESSION: &str = "__webhook__";

// ── Hook names ──────────────────────────────────────────────────────────────
//
// Aliases for the [`EventName`] variants, kept because the wire name and the
// hook name coincide for everything except the three file events and reading
// `NOTE_CREATED_EVENT` twice in one row is clearer than reading
// `EventName::NoteCreated.as_str()` twice. The enum is the source: a name added
// here without a variant does not compile, and a variant with no row is caught
// by `every_hook_name_has_a_row` below.

/// A watched file was created or modified.
pub const FILE_CHANGED_EVENT: &str = EventName::FileChanged.as_str();
/// A watched file was removed.
pub const FILE_DELETED_EVENT: &str = EventName::FileDeleted.as_str();
/// A watched file was renamed or moved.
pub const FILE_MOVED_EVENT: &str = EventName::FileMoved.as_str();
/// A note reached the index for the first time.
pub const NOTE_CREATED_EVENT: &str = EventName::NoteCreated.as_str();
/// An already-indexed note was written again.
pub const NOTE_MODIFIED_EVENT: &str = EventName::NoteModified.as_str();
/// A note left the index.
pub const NOTE_DELETED_EVENT: &str = EventName::NoteDeleted.as_str();
/// A note moved, with its inbound links repointed.
pub const NOTE_RENAMED_EVENT: &str = EventName::NoteRenamed.as_str();
/// A signed webhook delivery arrived at `POST /api/webhook/{name}`.
pub const WEBHOOK_RECEIVED_EVENT: &str = EventName::WebhookReceived.as_str();

/// One daemon event a Lua handler can see.
pub struct EventRow {
    /// `SessionEventMessage.event`.
    pub wire: &'static str,
    /// The name `crucible.on` registers against.
    ///
    /// Typed, not a `&str`: these are broadcast events, and giving them the
    /// same type as an interception stage is how `Cancel` came to mean two
    /// different things depending on which name the plugin author had written.
    pub hook: EventName,
    /// The `data` key whose value `opts.pattern` filters on, or `None` when the
    /// event carries no identifier — a handler that sets `pattern` on one of
    /// those correctly matches nothing.
    pub identifier: Option<&'static str>,
}

/// Every daemon event that reaches Lua. Adding one is a row.
pub const ROWS: &[EventRow] = &[
    EventRow {
        wire: "file_changed",
        hook: EventName::FileChanged,
        identifier: None,
    },
    EventRow {
        wire: "file_deleted",
        hook: EventName::FileDeleted,
        identifier: None,
    },
    EventRow {
        wire: "file_moved",
        hook: EventName::FileMoved,
        identifier: None,
    },
    EventRow {
        wire: NOTE_CREATED_EVENT,
        hook: EventName::NoteCreated,
        identifier: Some("path"),
    },
    EventRow {
        wire: NOTE_MODIFIED_EVENT,
        hook: EventName::NoteModified,
        identifier: Some("path"),
    },
    EventRow {
        wire: NOTE_DELETED_EVENT,
        hook: EventName::NoteDeleted,
        identifier: Some("path"),
    },
    // The destination, not the source: a handler filtering on a rename is
    // asking about the note as it is now.
    EventRow {
        wire: NOTE_RENAMED_EVENT,
        hook: EventName::NoteRenamed,
        identifier: Some("to"),
    },
    EventRow {
        wire: WEBHOOK_RECEIVED_EVENT,
        hook: EventName::WebhookReceived,
        identifier: Some("name"),
    },
];

/// The row for a wire name, or `None` for an event no handler can see.
pub fn row_for_wire(wire: &str) -> Option<&'static EventRow> {
    ROWS.iter().find(|row| row.wire == wire)
}

/// Build the broadcast message for an internal event — the **outbound** half.
///
/// Built with [`SessionEventMessage::typed`], so the wire name and the payload
/// shape come from one serde declaration and cannot disagree;
/// `every_outbound_name_is_its_row` pins the names that come out against the
/// table.
///
/// Returns `None` for an internal event with no wire form. Those are not
/// oversights — storage, embedding and interception events are pipeline
/// signals that deliberately never cross the RPC wire.
pub fn message_for(event: &InternalSessionEvent) -> Option<SessionEventMessage> {
    let payload = match event {
        InternalSessionEvent::FileChanged { path, kind } => SystemPayload::FileChanged {
            path: path.clone(),
            kind: *kind,
        },
        InternalSessionEvent::FileDeleted { path } => {
            SystemPayload::FileDeleted { path: path.clone() }
        }
        InternalSessionEvent::FileMoved { from, to } => SystemPayload::FileMoved {
            from: from.clone(),
            to: to.clone(),
        },
        InternalSessionEvent::NoteCreated { path, title } => SystemPayload::NoteCreated {
            path: path.to_string_lossy().to_string(),
            title: title.clone(),
        },
        InternalSessionEvent::NoteModified { path, change_type } => SystemPayload::NoteModified {
            path: path.to_string_lossy().to_string(),
            change_type: *change_type,
        },
        InternalSessionEvent::NoteDeleted { path, existed } => SystemPayload::NoteDeleted {
            path: path.to_string_lossy().to_string(),
            existed: *existed,
        },
        _ => return None,
    };
    Some(SessionEventMessage::typed(SYSTEM_SESSION, payload))
}

/// Build the `note:renamed` message.
///
/// Its own constructor rather than a [`message_for`] arm because there is no
/// `InternalSessionEvent::NoteRenamed` to build it from: a rename is not a
/// pipeline signal, it is the `note.rename` refactor reporting that the delete
/// and the insert it just performed were one move. Both paths are
/// kiln-relative.
pub fn note_renamed(from: &str, to: &str) -> SessionEventMessage {
    SessionEventMessage::typed(
        SYSTEM_SESSION,
        SystemPayload::NoteRenamed {
            from: from.to_string(),
            to: to.to_string(),
        },
    )
}

/// Build the `webhook:received` message.
///
/// Also its own constructor: a delivery arrives over HTTP, so there is no
/// internal event behind it either.
pub fn webhook_received(
    name: String,
    headers: serde_json::Map<String, serde_json::Value>,
    body: String,
) -> SessionEventMessage {
    SessionEventMessage::typed(
        WEBHOOK_SESSION,
        SystemPayload::WebhookReceived {
            name,
            headers,
            body,
        },
    )
}

/// A broadcast message resolved to everything a dispatch site needs.
pub struct HookedEvent {
    /// The name to look handlers up under.
    pub hook: EventName,
    /// The event handlers receive.
    pub event: SessionEvent,
    /// What `opts.pattern` filters on, when the event has an identifier.
    pub identifier: Option<String>,
}

/// Resolve a broadcast message to its hook — the **inbound** half.
///
/// `None` when the event has no row, or when its payload does not decode. The
/// decode is not decoration: it is what stops a malformed `file_changed` with
/// no `path` from firing every handler on an empty path, which an earlier
/// key-digging consumer did.
pub fn decode(msg: &SessionEventMessage) -> Option<HookedEvent> {
    let row = row_for_wire(&msg.event)?;
    let SessionEventPayload::System(payload) = msg.payload().ok()? else {
        return None;
    };

    // The file events keep their typed `InternalSessionEvent` form, which is
    // what handlers have always received for them. Everything else is carried
    // as `Custom`, whose Lua projection flattens `payload` to the top level and
    // reports `type` as the name — so a handler registered for `note:created`
    // reads `event.type == "note:created"`, rather than the Rust variant name a
    // typed event would report.
    let event = match payload {
        SystemPayload::FileChanged { path, kind } => {
            SessionEvent::internal(InternalSessionEvent::FileChanged { path, kind })
        }
        SystemPayload::FileDeleted { path } => {
            SessionEvent::internal(InternalSessionEvent::FileDeleted { path })
        }
        SystemPayload::FileMoved { from, to } => {
            SessionEvent::internal(InternalSessionEvent::FileMoved { from, to })
        }
        _ => SessionEvent::Custom {
            name: row.hook.as_str().to_string(),
            payload: msg.data.clone(),
        },
    };

    let identifier = row
        .identifier
        .and_then(|key| msg.data.get(key))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Some(HookedEvent {
        hook: row.hook,
        event,
        identifier,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::session_event::{FileChangeKind, NoteChangeType};
    use crucible_lua::LuaScriptHandlerRegistry;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// Run one decoded event through a real `crucible.on` handler and hand back
    /// the table the handler saw, with the VM that owns it.
    ///
    /// The `Arc<Lua>` comes back deliberately: an `mlua::Table` is a handle
    /// into its VM, and returning it alone drops the VM and panics with "Lua
    /// instance is destroyed" the moment the caller reads a field.
    ///
    /// The projection from `SessionEvent` to a Lua table is private to
    /// `crucible-lua`, and rightly so — asserting on it from the outside is
    /// asserting on the thing the plugin author actually reads, which is the
    /// point. So this registers through the real API and executes through the
    /// real registry rather than reaching for an internal helper.
    async fn as_the_handler_sees_it(hooked: &HookedEvent) -> (Arc<mlua::Lua>, mlua::Table) {
        let lua = Arc::new(mlua::Lua::new());
        let registry = LuaScriptHandlerRegistry::new();
        crucible_lua::register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("register crucible.on");
        lua.load(format!(
            "seen = nil\ncrucible.on(\"{}\", function(ctx, event) seen = event end)",
            hooked.hook
        ))
        .exec()
        .expect("register handler");

        let handlers =
            registry.runtime_handlers_for(hooked.hook.as_str(), hooked.identifier.as_deref());
        assert_eq!(
            handlers.len(),
            1,
            "`{}` matched no handler",
            hooked.hook.as_str()
        );
        registry
            .execute_runtime_handler(&lua, &handlers[0].name, &hooked.event, None)
            .await
            .expect("handler runs");
        let seen = lua.globals().get("seen").expect("the handler ran");
        (lua, seen)
    }

    /// One representative event per row, in row order, for the round-trip
    /// tests below. Anything that has an internal form is built from it, so the
    /// outbound half is exercised too.
    fn sample_messages() -> Vec<SessionEventMessage> {
        vec![
            message_for(&InternalSessionEvent::FileChanged {
                path: PathBuf::from("/w/a.md"),
                kind: FileChangeKind::Modified,
            })
            .expect("file_changed has a wire form"),
            message_for(&InternalSessionEvent::FileDeleted {
                path: PathBuf::from("/w/a.md"),
            })
            .expect("file_deleted has a wire form"),
            message_for(&InternalSessionEvent::FileMoved {
                from: PathBuf::from("/w/a.md"),
                to: PathBuf::from("/w/b.md"),
            })
            .expect("file_moved has a wire form"),
            message_for(&InternalSessionEvent::NoteCreated {
                path: PathBuf::from("Daily/a.md"),
                title: Some("A".into()),
            })
            .expect("note:created has a wire form"),
            message_for(&InternalSessionEvent::NoteModified {
                path: PathBuf::from("Daily/a.md"),
                change_type: NoteChangeType::Content,
            })
            .expect("note:modified has a wire form"),
            message_for(&InternalSessionEvent::NoteDeleted {
                path: PathBuf::from("Daily/a.md"),
                existed: true,
            })
            .expect("note:deleted has a wire form"),
            note_renamed("Daily/a.md", "Daily/b.md"),
            webhook_received("ci".into(), Default::default(), "{}".into()),
        ]
    }

    /// The table and the sample set are the same length, so a row added without
    /// a sample silently escapes every test below.
    #[test]
    fn every_row_has_a_sample() {
        assert_eq!(sample_messages().len(), ROWS.len());
        for (msg, row) in sample_messages().iter().zip(ROWS) {
            assert_eq!(msg.event, row.wire, "sample order must follow the table");
        }
    }

    /// The outbound constructors mint the names the table declares.
    ///
    /// They come from serde renames on `SystemPayload`, in another crate. When
    /// those drift from the table the event is broadcast under a name the
    /// inbound half does not recognise, and the hook goes quiet with nothing
    /// logged anywhere.
    #[test]
    fn every_outbound_name_is_its_row() {
        for msg in sample_messages() {
            assert!(
                row_for_wire(&msg.event).is_some(),
                "`{}` is broadcast but has no row",
                msg.event
            );
        }
    }

    /// Every broadcast message resolves back to its row.
    #[test]
    fn every_message_decodes_to_its_hook() {
        for (msg, row) in sample_messages().iter().zip(ROWS) {
            let hooked = decode(msg).unwrap_or_else(|| panic!("`{}` did not decode", msg.event));
            assert_eq!(hooked.hook, row.hook);
        }
    }

    /// **The contract this module exists to keep.** A handler registered for a
    /// name reads that same name off the event.
    ///
    /// Not free: the typed events report their Rust `type_name()`, so a note
    /// event carried as `InternalSessionEvent::NoteCreated` would tell a
    /// handler registered for `note:created` that it had received a
    /// `NoteCreated`.
    #[tokio::test]
    async fn the_handler_sees_the_name_it_registered() {
        for msg in sample_messages() {
            let hooked = decode(&msg).expect("decodes");
            let (_lua, seen) = as_the_handler_sees_it(&hooked).await;
            let reported: String = seen.get("type").expect("every event carries `type`");
            assert_eq!(
                reported,
                hooked.hook.as_str(),
                "`{}` presents the wrong `type` to its handler",
                msg.event
            );
        }
    }

    /// Every event `crucible.on` accepts has a row here.
    ///
    /// The other direction is now the compiler's: `EventRow.hook` is an
    /// [`EventName`], so a row naming something unregisterable does not exist.
    /// This is the half that is still hand-maintained — a variant added to
    /// `EventName` with no row is a name a plugin may register for and which
    /// nothing will ever broadcast, which is exactly how `webhook:received`
    /// reached nobody for months.
    #[test]
    fn every_registerable_event_has_a_row() {
        for event in EventName::ALL {
            assert!(
                ROWS.iter().any(|row| row.hook == *event),
                "`{}` is registerable but no row broadcasts it",
                event.as_str()
            );
        }
    }

    /// The identifier column reaches the value `opts.pattern` globs against.
    #[test]
    fn the_identifier_is_the_value_a_pattern_filters_on() {
        let expected = [
            None,
            None,
            None,
            Some("Daily/a.md"),
            Some("Daily/a.md"),
            Some("Daily/a.md"),
            // The destination path, so a rename INTO a watched folder matches.
            Some("Daily/b.md"),
            Some("ci"),
        ];
        for (msg, want) in sample_messages().iter().zip(expected) {
            let hooked = decode(msg).expect("decodes");
            assert_eq!(
                hooked.identifier.as_deref(),
                want,
                "wrong identifier for `{}`",
                msg.event
            );
        }
    }

    /// The payload keys survive to the handler, per event.
    #[tokio::test]
    async fn the_payload_reaches_the_handler() {
        async fn seen(wire: &str) -> (Arc<mlua::Lua>, mlua::Table) {
            let msg = sample_messages()
                .into_iter()
                .find(|m| m.event == wire)
                .expect("sampled");
            as_the_handler_sees_it(&decode(&msg).expect("decodes")).await
        }

        let (_lua, file) = seen("file_changed").await;
        assert_eq!(file.get::<String>("path").unwrap(), "/w/a.md");

        let (_lua, created) = seen(NOTE_CREATED_EVENT).await;
        assert_eq!(created.get::<String>("path").unwrap(), "Daily/a.md");
        assert_eq!(created.get::<String>("title").unwrap(), "A");

        let (_lua, deleted) = seen(NOTE_DELETED_EVENT).await;
        assert!(deleted.get::<bool>("existed").unwrap());

        let (_lua, renamed) = seen(NOTE_RENAMED_EVENT).await;
        assert_eq!(renamed.get::<String>("from").unwrap(), "Daily/a.md");
        assert_eq!(renamed.get::<String>("to").unwrap(), "Daily/b.md");

        let (_lua, webhook) = seen(WEBHOOK_RECEIVED_EVENT).await;
        assert_eq!(webhook.get::<String>("name").unwrap(), "ci");
        assert_eq!(webhook.get::<String>("body").unwrap(), "{}");
    }

    /// A handler that set `opts.pattern` filters on the identifier, and one
    /// whose glob does not match the note is not run.
    #[tokio::test]
    async fn a_pattern_filters_on_the_note_path() {
        let msg = message_for(&InternalSessionEvent::NoteModified {
            path: PathBuf::from("Daily/2026-08-18.md"),
            change_type: NoteChangeType::Content,
        })
        .expect("has a wire form");
        let hooked = decode(&msg).expect("decodes");

        let lua = Arc::new(mlua::Lua::new());
        let registry = LuaScriptHandlerRegistry::new();
        crucible_lua::register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("register crucible.on");
        lua.load(
            r#"
            crucible.on("note:modified", { pattern = "Daily/*" }, function() end)
            crucible.on("note:modified", { pattern = "Meta/*" }, function() end)
            "#,
        )
        .exec()
        .expect("register handlers");

        let matched =
            registry.runtime_handlers_for(hooked.hook.as_str(), hooked.identifier.as_deref());
        assert_eq!(
            matched.len(),
            1,
            "only the handler whose glob covers the note should run"
        );
    }

    /// An event with no row is dropped rather than dispatched under a guessed
    /// name.
    #[test]
    fn an_event_with_no_row_does_not_decode() {
        let msg = SessionEventMessage::new("s1", "message_complete", json!({}));
        assert!(decode(&msg).is_none());
        assert!(row_for_wire("message_complete").is_none());
    }

    /// A payload that does not decode fires nothing.
    ///
    /// `file_changed` requires `path`; the consumer this replaced read
    /// `data["path"]` before matching the name and would have fired every
    /// handler on an empty one.
    #[test]
    fn a_malformed_payload_fires_nothing() {
        let msg = SessionEventMessage::new(SYSTEM_SESSION, "file_changed", json!({}));
        assert!(decode(&msg).is_none());
        let msg = SessionEventMessage::new(SYSTEM_SESSION, NOTE_CREATED_EVENT, json!({}));
        assert!(decode(&msg).is_none());
    }

    /// A path the filesystem allows but UTF-8 does not is broadcast lossily
    /// rather than fatally.
    ///
    /// serde refuses to serialize a non-UTF-8 `PathBuf` and
    /// `SessionEventPayload::to_wire` treats serialization as infallible, so it
    /// panics. The hand-written bridge this replaced printed the path with
    /// `Path::display` and never had the problem; a single oddly named file
    /// under a watched directory would otherwise take the broadcast task down.
    #[cfg(unix)]
    #[test]
    fn a_non_utf8_path_is_broadcast_lossily() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let path = PathBuf::from(OsStr::from_bytes(b"/w/\xff.md"));
        let msg = message_for(&InternalSessionEvent::FileChanged {
            path: path.clone(),
            kind: FileChangeKind::Modified,
        })
        .expect("file_changed has a wire form");
        assert_eq!(
            msg.data.get("path").and_then(|v| v.as_str()),
            Some(path.to_string_lossy().as_ref()),
            "the wire path must be what `Path::display` prints"
        );
        assert!(decode(&msg).is_some(), "and it still reaches a handler");

        let moved = message_for(&InternalSessionEvent::FileMoved {
            from: path.clone(),
            to: PathBuf::from(OsStr::from_bytes(b"/w/\xfe.md")),
        })
        .expect("file_moved has a wire form");
        assert!(decode(&moved).is_some());
    }

    /// The internal events with no wire form stay internal.
    #[test]
    fn a_pipeline_only_event_has_no_message() {
        assert!(message_for(&InternalSessionEvent::EntityStored {
            entity_id: "e1".into(),
            entity_type: crucible_core::events::EntityType::Note,
        })
        .is_none());
    }
}
