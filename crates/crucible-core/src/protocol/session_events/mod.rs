//! The typed payload contract for `SessionEventMessage`.
//!
//! [`SessionEventMessage`](crate::protocol::SessionEventMessage) carries
//! `event: String` + `data: Value`. That pair is the *transport*; this module is
//! the *contract*. [`SessionEventPayload`] is a constructor and a view over the
//! same two fields, not a replacement for them:
//!
//! ```text
//! SessionEventMessage::typed(id, TurnPayload::TextDelta { .. })  →  {event, data}
//! msg.payload()                                                 →  SessionEventPayload
//! ```
//!
//! **Adjacent tagging is the whole trick.** Every group enum carries
//! `#[serde(tag = "event", content = "data")]`, so
//! `serde_json::to_value(&TurnPayload::TextDelta { content })` is exactly
//! `{"event":"text_delta","data":{"content":"…"}}` — the two envelope fields,
//! byte-identical to what the hand-written `json!` constructors produced. Wire
//! compatibility is therefore *structural*, not asserted: recorded fixtures,
//! persisted `session.jsonl`, and every existing subscriber keep working, and
//! there is no new format for a log to be in.
//!
//! # The groups
//!
//! | Group | Count | Module |
//! |---|---|---|
//! | [`TurnPayload`] | 15 | [`turn`] |
//! | [`SetupPayload`] | 8 | [`setup`] |
//! | [`SettingsPayload`] | 9 | [`settings`] |
//! | [`JobPayload`] | 7 | [`lifecycle`] |
//! | [`ReviewPayload`] | 2 | [`lifecycle`] |
//! | [`NotificationPayload`] | 2 | [`lifecycle`] |
//! | [`WorkflowPayload`] | 8 | [`lifecycle`] |
//! | [`SystemPayload`] | 20 | [`lifecycle`] |
//!
//! `assets/fixtures/golden/session_event_wire_names.txt` lists each name by
//! group, and a test compares it with the declarations.
//!
//! Groups rather than one flat 71-variant enum because exhaustive matching over
//! 71 variants in nine consumers is 639 arms — worse than the untyped code it
//! replaces. Grouping makes the *interesting* set exhaustive and the rest one
//! arm.
//!
//! The outer enum has **no serde derive**. `#[serde(untagged)]` would work and
//! would also let a malformed payload silently deserialize as a *different*
//! group, with a useless error message. [`SessionEventPayload::from_wire`]
//! dispatches on the name explicitly instead.
//!
//! Payloadless events use **empty struct variants** (`WorkflowCompleted {}`),
//! never unit variants: a unit variant under adjacent tagging omits `data`,
//! which surfaces as `null` where today's wire has `{}`.
//!
//! # Two vocabularies, not one
//!
//! This is the **transport** vocabulary. [`SessionEvent`](crate::events::SessionEvent)
//! is the **scripting** vocabulary — Lua tables and markdown
//! session logs. They are disjoint by design and neither is a subset of the
//! other:
//!
//! | | `SessionEventPayload` | `SessionEvent` |
//! |---|---|---|
//! | Serialization target | RPC socket, SSE, `session.jsonl`, `assets/fixtures/*.jsonl` | markdown session logs, Lua tables |
//! | Back-compat constraint | recorded fixtures and persisted sessions on disk | `handlers/*.lua` in user kilns |
//! | Vocabulary size | 71 (see the golden list) | 14 wire-ish + 41 internal |
//! | `tool_call` is called | `tool_call` | `tool_called` |
//! | `tool_result` is called | `tool_result` | `tool_completed` |
//! | `message_complete` is called | `message_complete` | `agent_responded` |
//!
//! Unifying them means either putting 41 internal-only variants on the wire or
//! taking 41 hooks away from Lua. [`TurnPayload::as_scripting_event`] makes the
//! nine-name overlap explicit and tested instead.
//!
//! # The reader stays tolerant; the writer becomes typed
//!
//! There is no version gate and no rewrite pass over
//! `~/.crucible/sessions/**/session.jsonl`. The typed writer introduces no new
//! format (see above), sessions are the user's plaintext data, and
//! `session.jsonl` legitimately holds two shapes — `LogEvent` lines from
//! `inject_context` and `fork`, wire-envelope lines from the broadcast path.
//! The asymmetry is correct, not debt.

// Each wire name is written once: serde, routing and the optional name const
// expand from the same literal. Payload fields remain ordinary Rust/serde
// declarations. `"name" as CONST =>` also declares `Self::CONST` for a crate
// that must name the event rather than build one.
macro_rules! event_payload {
    (
        $(#[$enum_meta:meta])*
        $visibility:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $wire:literal $(as $const_name:ident)? => $variant:ident $fields:tt
            ),* $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        $visibility enum $name {
            $(
                $(#[$variant_meta])*
                #[serde(rename = $wire)]
                $variant $fields,
            )*
        }
        impl $name {
            pub const WIRE_NAMES: &'static [&'static str] = &[$($wire),*];
            $($(pub const $const_name: &'static str = $wire;)?)*

            /// Whether this enum declares `event`. A `match` on the literals,
            /// so a decode does not scan the name list.
            pub fn declares(event: &str) -> bool {
                matches!(event, $($wire)|*)
            }
        }
    };
}

pub mod lifecycle;
pub mod settings;
pub mod setup;
pub mod turn;

#[cfg(test)]
mod tests;

pub use lifecycle::{
    JobPayload, NotificationPayload, ReviewPayload, SystemPayload, WorkflowPayload,
};
pub use settings::SettingsPayload;
pub use setup::{
    ContextLimitResolvedPayload, ContextLimitSource, KilnNotesIndexedPayload,
    McpServersReadyPayload, PluginsDiscoveredPayload, ProvidersListedPayload,
    SessionInitializedPayload, SetupPayload, WorkspaceIndexedPayload,
};
pub use turn::{ToolResultBody, TurnPayload};

use serde_json::Value;

/// A decoded session-event payload: one of eight groups.
///
/// Construct with [`SessionEventMessage::typed`](crate::protocol::SessionEventMessage::typed);
/// read with [`SessionEventMessage::payload`](crate::protocol::SessionEventMessage::payload).
#[derive(Clone, Debug)]
pub enum SessionEventPayload {
    Turn(TurnPayload),
    Setup(SetupPayload),
    Settings(SettingsPayload),
    Job(JobPayload),
    Review(ReviewPayload),
    Notification(NotificationPayload),
    Workflow(WorkflowPayload),
    System(SystemPayload),
}

/// Which group an event name belongs to.
///
/// The one place a wire name maps to a Rust type. `Group::of` returning `None`
/// is the forward-compatibility path: an event a newer daemon minted, or a name
/// only a recording contains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumIter)]
pub enum Group {
    Turn,
    Setup,
    Settings,
    Job,
    Review,
    Notification,
    Workflow,
    System,
}

impl Group {
    /// Names declared by this group's payload enum.
    pub fn wire_names(self) -> &'static [&'static str] {
        match self {
            Self::Turn => TurnPayload::WIRE_NAMES,
            Self::Setup => SetupPayload::WIRE_NAMES,
            Self::Settings => SettingsPayload::WIRE_NAMES,
            Self::Job => JobPayload::WIRE_NAMES,
            Self::Review => ReviewPayload::WIRE_NAMES,
            Self::Notification => NotificationPayload::WIRE_NAMES,
            Self::Workflow => WorkflowPayload::WIRE_NAMES,
            Self::System => SystemPayload::WIRE_NAMES,
        }
    }

    /// The group that declares `event`. Each group answers with a `match`
    /// on its literals; see [`TurnPayload::declares`].
    pub fn of(event: &str) -> Option<Self> {
        use strum::IntoEnumIterator;
        Self::iter().find(|group| group.declares(event))
    }

    fn declares(self, event: &str) -> bool {
        match self {
            Self::Turn => TurnPayload::declares(event),
            Self::Setup => SetupPayload::declares(event),
            Self::Settings => SettingsPayload::declares(event),
            Self::Job => JobPayload::declares(event),
            Self::Review => ReviewPayload::declares(event),
            Self::Notification => NotificationPayload::declares(event),
            Self::Workflow => WorkflowPayload::declares(event),
            Self::System => SystemPayload::declares(event),
        }
    }
}

/// Why a payload would not decode.
///
/// thiserror at the boundary, per the crate's error convention: callers match on
/// `UnknownEvent` (pass the raw `{event, data}` through) versus
/// `MalformedPayload` (warn) and behave differently. Both carry the raw name, so
/// nothing an unknown event knows is lost — which a `#[serde(other)]` unit
/// variant could not manage.
#[derive(Debug, thiserror::Error)]
pub enum EventDecodeError {
    #[error("unknown session event `{event}`")]
    UnknownEvent { event: String },
    #[error("malformed payload for session event `{event}`")]
    MalformedPayload {
        event: String,
        #[source]
        source: serde_json::Error,
    },
}

impl SessionEventPayload {
    /// The `{event, data}` pair this payload is.
    ///
    /// Infallible by construction: adjacent tagging always produces an object
    /// with a string tag, and no group nests a map with non-string keys.
    pub fn to_wire(&self) -> (String, Value) {
        let v = match self {
            Self::Turn(p) => serde_json::to_value(p),
            Self::Setup(p) => serde_json::to_value(p),
            Self::Settings(p) => serde_json::to_value(p),
            Self::Job(p) => serde_json::to_value(p),
            Self::Review(p) => serde_json::to_value(p),
            Self::Notification(p) => serde_json::to_value(p),
            Self::Workflow(p) => serde_json::to_value(p),
            Self::System(p) => serde_json::to_value(p),
        }
        .expect("payload groups serialize infallibly: no maps with non-string keys");

        let mut obj = match v {
            Value::Object(o) => o,
            other => unreachable!("adjacent tagging always yields an object, got {other}"),
        };
        let event = obj
            .remove("event")
            .and_then(|e| match e {
                Value::String(s) => Some(s),
                _ => None,
            })
            .expect("adjacent tag is always a string");
        // `None` only for a unit variant, which this module forbids — empty
        // struct variants exist so `data` stays `{}` rather than `null`.
        let data = obj.remove("data").unwrap_or(Value::Null);
        (event, data)
    }

    /// Decode a wire `{event, data}` pair.
    pub fn from_wire(event: &str, data: &Value) -> Result<Self, EventDecodeError> {
        if let Some((event, data)) = migrate(event, data) {
            return Self::from_wire(event, &data);
        }
        let group = Group::of(event).ok_or_else(|| EventDecodeError::UnknownEvent {
            event: event.to_string(),
        })?;
        let composite = serde_json::json!({ "event": event, "data": data });
        let malformed = |source: serde_json::Error| EventDecodeError::MalformedPayload {
            event: event.to_string(),
            source,
        };
        Ok(match group {
            Group::Turn => Self::Turn(serde_json::from_value(composite).map_err(malformed)?),
            Group::Setup => Self::Setup(serde_json::from_value(composite).map_err(malformed)?),
            Group::Settings => {
                Self::Settings(serde_json::from_value(composite).map_err(malformed)?)
            }
            Group::Job => Self::Job(serde_json::from_value(composite).map_err(malformed)?),
            Group::Review => Self::Review(serde_json::from_value(composite).map_err(malformed)?),
            Group::Notification => {
                Self::Notification(serde_json::from_value(composite).map_err(malformed)?)
            }
            Group::Workflow => {
                Self::Workflow(serde_json::from_value(composite).map_err(malformed)?)
            }
            Group::System => Self::System(serde_json::from_value(composite).map_err(malformed)?),
        })
    }

    /// Does this event belong in `session.jsonl`?
    ///
    /// The Turn and Settings groups make a per-variant decision (see
    /// [`TurnPayload::is_persisted`] and [`SettingsPayload::is_persisted`]); the
    /// rest are live notifications whose state is recoverable from the session
    /// record.
    ///
    /// `session_initialized` is here because a session's starting model is
    /// otherwise unrecoverable on resume: it is emitted, but was never
    /// persisted, while `model_switched` was — so a session that switched models
    /// had an attributable second half and an unattributable first half.
    ///
    /// It is persisted **only when the model is known**. The setup task runs
    /// before `session.configure_agent`, so it "almost always observes `None`
    /// here and the event carries empty strings"
    /// (`server/session/mod.rs`) — `assets/fixtures/demo.jsonl` records
    /// `"model":""`. Persisting that is worse than persisting nothing, because
    /// an empty model looks like an answer. Emitting the event after the model
    /// resolves is the real fix and belongs to the setup task, not here.
    pub fn is_persisted(&self) -> bool {
        match self {
            Self::Turn(t) => t.is_persisted(),
            Self::Settings(s) => s.is_persisted(),
            Self::Setup(SetupPayload::SessionInitialized(p)) => !p.model.is_empty(),
            Self::Setup(_)
            | Self::Job(_)
            | Self::Review(_)
            | Self::Notification(_)
            | Self::Workflow(_)
            | Self::System(_) => false,
        }
    }
}

macro_rules! impl_from_group {
    ($($ty:ident => $variant:ident),* $(,)?) => {
        $(impl From<$ty> for SessionEventPayload {
            fn from(p: $ty) -> Self {
                Self::$variant(p)
            }
        })*
    };
}

impl_from_group! {
    TurnPayload => Turn,
    SetupPayload => Setup,
    SettingsPayload => Settings,
    JobPayload => Job,
    ReviewPayload => Review,
    NotificationPayload => Notification,
    WorkflowPayload => Workflow,
    SystemPayload => System,
}

/// The current form of an event from an older transcript. `None` when the
/// event already has the current form.
///
/// [`SessionEventPayload::from_wire`] reads each event through it, and the
/// daemon sends a stored transcript through [`migrate_history`]. So a client
/// never reads an old form, and no client keeps its own copy of this table.
///
/// - `tool_call` with a top-level `diffs`, a `lua_primary_arg` or a display
///   `{kind, primary}`: the diffs go into the display, and the old primary
///   becomes the render line.
/// - `tool_call_args_update`: the old name of `tool_call_update`.
/// - `tool_call_diff_update`: a `tool_call_update` with only diffs. Its
///   display has no render, so a client keeps the line of the card.
/// - `ended`: the `turn_finished` that it stood for.
/// - `user_message` with a flat `origin` and `plugin`: the nested origin.
pub fn migrate(event: &str, data: &Value) -> Option<(&'static str, Value)> {
    let text = |key: &str| data.get(key).and_then(Value::as_str).unwrap_or_default();
    match event {
        "tool_call" => old_tool_call(data).map(|data| ("tool_call", data)),
        "tool_call_args_update" => Some(("tool_call_update", data.clone())),
        "tool_call_diff_update" => Some((
            "tool_call_update",
            serde_json::json!({
                "call_id": text("call_id"),
                "display": { "kind": "tool", "tool": "", "diffs": data.get("diffs") },
            }),
        )),
        "ended" => {
            let reason = text("reason");
            let finished = match reason.strip_prefix("error: ") {
                Some(error) => serde_json::json!({ "status": "failed", "error": error }),
                None if reason.starts_with("cancelled") => {
                    serde_json::json!({ "status": "cancelled" })
                }
                None => serde_json::json!({ "status": "completed" }),
            };
            Some(("turn_finished", finished))
        }
        "user_message" if data.get("origin").is_some_and(Value::is_string) => {
            let mut data = data.as_object()?.clone();
            let mut origin = serde_json::json!({ "kind": data.remove("origin") });
            if let Some(name) = data.remove("plugin") {
                origin["name"] = name;
            }
            data.insert("origin".into(), origin);
            Some(("user_message", Value::Object(data)))
        }
        _ => None,
    }
}

/// A `tool_call` from before the render, in its current form. `None` for a
/// current one. The result has no old key, so [`migrate`] never repeats.
fn old_tool_call(data: &Value) -> Option<Value> {
    let primary = data.pointer("/display/primary").cloned();
    if data.get("diffs").is_none() && data.get("lua_primary_arg").is_none() && primary.is_none() {
        return None;
    }
    let mut data = data.as_object()?.clone();
    let diffs = data.remove("diffs");
    let line = data.remove("lua_primary_arg").or(primary);
    let tool = data
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let display = data
        .entry("display")
        .or_insert_with(|| serde_json::json!({ "kind": "tool", "tool": tool }));
    let display = display.as_object_mut()?;
    display.remove("primary");
    if let Some(diffs) = diffs {
        display.insert("diffs".into(), diffs);
    }
    if let Some(line) = line.filter(Value::is_string) {
        display
            .entry("render")
            .or_insert_with(|| serde_json::json!({ "line": line }));
    }
    Some(Value::Object(data))
}

/// A stored transcript in its current form: each event through [`migrate`].
///
/// An old log has an `ended` event where a turn stopped. A log from the
/// time when both events existed also has the `turn_finished` of that turn,
/// after the `ended`. Then the `ended` is dropped, so that no client ends
/// the turn two times.
pub fn migrate_history(events: Vec<Value>) -> Vec<Value> {
    let name = |e: &Value| e.get("event").and_then(Value::as_str).map(str::to_string);
    let mut finished = false;
    let mut duplicate = vec![false; events.len()];
    for (i, event) in events.iter().enumerate().rev() {
        match name(event).as_deref() {
            Some("turn_finished") => finished = true,
            Some("user_message") => finished = false,
            Some("ended") => duplicate[i] = finished,
            _ => {}
        }
    }
    events
        .into_iter()
        .zip(duplicate)
        .filter(|(_, duplicate)| !duplicate)
        .map(|(mut event, _)| {
            let current = name(&event)
                .zip(event.get("data"))
                .and_then(|(n, d)| migrate(&n, d));
            if let Some((name, data)) = current {
                event["event"] = name.into();
                event["data"] = data;
            }
            event
        })
        .collect()
}
