//! The job, review, notification, workflow and system groups.
//!
//! These five are grouped in one file because none of them is big enough to
//! carry a module of its own, and every consumer that matches one of them
//! matches it in a single arm.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::events::session_event::{FileChangeKind, NoteChangeType};
use crate::workflow::AssessmentOutcome;

/// Serialize a path the way `Path::display` prints it: any byte that is not
/// valid UTF-8 becomes U+FFFD.
///
/// serde's own `PathBuf` impl *fails* on a non-UTF-8 path, and
/// [`SessionEventPayload::to_wire`](super::SessionEventPayload::to_wire) treats
/// a serialization failure as impossible and panics. The filesystem allows
/// those bytes and the watcher reports whatever it is handed, so one oddly
/// named file in a watched directory would take down the broadcast path. The
/// bridge that used to build these events by hand already printed the path with
/// `Path::display`; this keeps that exact wire output now that the typed
/// payload owns the conversion.
mod lossy_path {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::path::{Path, PathBuf};

    pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&path.to_string_lossy())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        String::deserialize(deserializer).map(PathBuf::from)
    }
}

/// Delegation and background-job lifecycle.
///
/// The `delegation_*` names predate the current delegation system and are
/// preserved for subscriber compatibility.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum JobPayload {
    /// `delegation_id` and `child_session_id` always hold the same value —
    /// `DelegationSpawned` documents `delegation_id == child_session_id`. Both
    /// are on the wire because a subscriber may read either, and dropping one
    /// would silently break it.
    DelegationSpawned {
        #[serde(default)]
        delegation_id: String,
        #[serde(default)]
        child_session_id: String,
        #[serde(default)]
        prompt: String,
        #[serde(default)]
        target_agent: Option<String>,
        #[serde(default)]
        parent_session_id: String,
    },
    DelegationCompleted {
        #[serde(default)]
        delegation_id: String,
        #[serde(default)]
        child_session_id: String,
        #[serde(default)]
        result_summary: String,
        #[serde(default)]
        parent_session_id: String,
    },
    DelegationFailed {
        #[serde(default)]
        delegation_id: String,
        #[serde(default)]
        child_session_id: String,
        #[serde(default)]
        error: String,
        #[serde(default)]
        parent_session_id: String,
    },
    BashJobSpawned {
        #[serde(default)]
        job_id: String,
        #[serde(default)]
        command: String,
    },
    BashJobCompleted {
        #[serde(default)]
        job_id: String,
        #[serde(default)]
        output: String,
        #[serde(default)]
        exit_code: Option<i32>,
    },
    BashJobFailed {
        #[serde(default)]
        job_id: String,
        #[serde(default)]
        error: String,
        #[serde(default)]
        exit_code: Option<i32>,
    },
    BackgroundJobCompleted {
        #[serde(default)]
        job_id: String,
        #[serde(default)]
        kind: String,
        #[serde(default)]
        summary: String,
    },
}

/// Review-gate and undo events.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum ReviewPayload {
    /// The session's composed diff moved: a hunk was accepted, rejected,
    /// reverted, commented on, or a comment was resolved.
    ///
    /// Deliberately carries only `reason` and no hunk identity. A hunk id is
    /// derived from its content *and* its range in `session_base`, so a change
    /// that re-aligns an ambiguous region moves the ids of hunks the user never
    /// touched — a client that patched a single row in place from this event
    /// would be showing a decision attached to different lines. The event says
    /// "re-list"; the listing is the truth.
    ///
    /// `reason` is advisory (`"accepted"`, `"rejected"`, `"commented"`,
    /// `"comment_resolved"`, `"external"`) and stays a `String`, not an enum:
    /// clients must not switch on it for correctness, and an enum would invite
    /// exactly that.
    ReviewChanged {
        #[serde(default)]
        reason: String,
    },
    ReviewGate {
        #[serde(default)]
        blocked: bool,
        #[serde(default)]
        tool: String,
        #[serde(default)]
        path: Option<String>,
    },
    SessionUndo {
        #[serde(default)]
        turns_undone: usize,
        #[serde(default)]
        messages_removed: usize,
    },
}

/// Session notification list changes. Both carry only the id — the list itself
/// is fetched, so the event says "re-read" rather than shipping a projection
/// that can go stale.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum NotificationPayload {
    NotificationAdded {
        #[serde(default)]
        notification_id: String,
    },
    NotificationDismissed {
        #[serde(default)]
        notification_id: String,
    },
}

/// Workflow-engine progress.
///
/// Every name needs an explicit `rename`: the wire uses a `workflow.` prefix
/// that `rename_all = "snake_case"` cannot produce.
///
/// `WorkflowCompleted` and `WorkflowCancelled` are **empty struct variants, not
/// unit variants**. Under adjacent tagging a unit variant omits `data`
/// entirely, which `to_wire` then reports as `null`; today's producers emit
/// `{}`. `{}` and `null` are different JSON.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum WorkflowPayload {
    #[serde(rename = "workflow.step_started")]
    WorkflowStepStarted {
        #[serde(default)]
        step_id: String,
        #[serde(default)]
        title: String,
    },
    #[serde(rename = "workflow.step_completed")]
    WorkflowStepCompleted {
        #[serde(default)]
        step_id: String,
        #[serde(default)]
        output_name: Option<String>,
    },
    #[serde(rename = "workflow.gate_reached")]
    WorkflowGateReached {
        #[serde(default)]
        gate_id: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        owner: String,
    },
    #[serde(rename = "workflow.gate_approved")]
    WorkflowGateApproved {
        #[serde(default)]
        gate_id: String,
    },
    #[serde(rename = "workflow.completed")]
    WorkflowCompleted {},
    #[serde(rename = "workflow.assessed")]
    WorkflowAssessed {
        #[serde(default)]
        runnable_passed: Vec<AssessmentOutcome>,
        #[serde(default)]
        runnable_failed: Vec<AssessmentOutcome>,
        #[serde(default)]
        manual_entries: Vec<String>,
    },
    #[serde(rename = "workflow.failed")]
    WorkflowFailed {
        #[serde(default)]
        reason: String,
        #[serde(default)]
        at_step: Option<String>,
    },
    #[serde(rename = "workflow.cancelled")]
    WorkflowCancelled {},
}

/// Daemon-wide events that are not scoped to one turn: file-watch, kiln
/// processing, UI config, webhooks, replay.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum SystemPayload {
    /// `kind` is a typed [`FileChangeKind`] rather than a string built with
    /// `format!("{kind}")` and matched with two string arms — the old consumer
    /// mapped everything that was not `created` to `Modified`, silently.
    /// `path` is required, not defaulted: a file event without a path is
    /// meaningless, and the consumer this replaces dropped such an event rather
    /// than firing a handler on an empty path. `kind` defaults to `Modified`,
    /// matching the two-arm string match it replaces.
    FileChanged {
        #[serde(with = "lossy_path")]
        path: PathBuf,
        #[serde(default)]
        kind: FileChangeKind,
    },
    FileDeleted {
        #[serde(with = "lossy_path")]
        path: PathBuf,
    },
    /// Carries `from`/`to` and **no** `path`. The consumer that rebuilt the
    /// typed event read `data["path"]` before matching the name, so this event
    /// was broadcast and could never reach a Lua handler.
    FileMoved {
        #[serde(with = "lossy_path")]
        from: PathBuf,
        #[serde(with = "lossy_path")]
        to: PathBuf,
    },
    ClassificationRequired {
        /// The kiln's registry name, absent when no entry claims it. Never a
        /// path — see the internal `SessionEvent` variant for why.
        #[serde(default)]
        kiln: Option<crate::config::KilnName>,
    },
    /// One producer: `handle_kiln_open` (`crucible-daemon/src/server/kiln.rs`),
    /// once a `kiln.open { process: true }` has finished indexing a kiln. The
    /// second producer this name used to have — the `process_batch` RPC, which
    /// sent `type`/`batch_id` and no `kiln` — is gone, along with the
    /// `process_start`/`process_progress` events beside it; they reported per-file
    /// work on a batch of one and nothing ever read them.
    ///
    /// Every field keeps `#[serde(default)]` even though the surviving producer
    /// writes all five: a daemon older than that deletion still sends the batch
    /// shape, and it must decode as zeroes rather than as `MalformedPayload`.
    ProcessComplete {
        #[serde(default)]
        kiln: String,
        #[serde(default)]
        discovered: usize,
        #[serde(default)]
        processed: usize,
        #[serde(default)]
        skipped: usize,
        #[serde(default)]
        errors: usize,
    },
    /// The payload is the `ui.config` RPC result, produced by
    /// `rpc::ui::style_payload` (theme, geometry, bars) or
    /// `rpc::ui::expr_payload` (one session's statusline values). The two
    /// genuinely differ in shape, so this stays a `Value`: a client applies it
    /// with the same code path it uses at attach, and narrowing the type here
    /// would only move the drift to `rpc::ui`.
    UiStyleChanged(Value),
    /// A subscriber fell far enough behind the broadcast ring that events were
    /// overwritten before it read them, and `dropped` of them are gone for good.
    ///
    /// This is a *transport* fact rather than something that happened in the
    /// session, and it is the one event the daemon emits about its own delivery.
    /// It is in this vocabulary anyway, deliberately: it reaches clients over the
    /// same channel as everything else and both surfaces render it, so leaving it
    /// outside would mean exactly the untyped `{event, data}` pair this enum
    /// exists to retire — and a marker announcing lost data is a poor thing to
    /// leave unvalidated. Consumers should treat it as "your transcript has a hole
    /// here", not as session content.
    StreamGap {
        #[serde(default)]
        dropped: u64,
    },
    /// A note reached the index for the first time.
    ///
    /// The note events are the *knowledge* half of the file events above: a
    /// file event says a path changed on disk, this says the note pipeline
    /// parsed it and wrote it to the store. `path` is kiln-relative, which is
    /// the spelling every other note API uses and the one a handler's
    /// `opts.pattern` glob is written against.
    ///
    /// Colon-namespaced rather than `note_created`, matching `webhook:received`
    /// below: these names are also the names Lua handlers register with
    /// (`crucible-daemon/src/event_map.rs`), and the colon marks the ones that
    /// are a designed hook surface rather than a Rust variant name.
    #[serde(rename = "note:created")]
    NoteCreated {
        path: String,
        #[serde(default)]
        title: Option<String>,
    },
    /// An already-indexed note was written again.
    #[serde(rename = "note:modified")]
    NoteModified {
        path: String,
        #[serde(default)]
        change_type: NoteChangeType,
    },
    /// A note left the index.
    ///
    /// `existed` is false when the delete found nothing to remove — the
    /// reconciliation sweep asks for paths it is not sure about.
    #[serde(rename = "note:deleted")]
    NoteDeleted {
        path: String,
        #[serde(default)]
        existed: bool,
    },
    /// A note moved, with its inbound links repointed.
    ///
    /// Emitted by the `note.rename` refactor only, which is the one place that
    /// knows the two paths are the same note. The reindex underneath it is a
    /// delete followed by an insert, so `note:deleted` and `note:created` fire
    /// for the same operation; this is the event that says they were a move.
    #[serde(rename = "note:renamed")]
    NoteRenamed { from: String, to: String },
    #[serde(rename = "webhook:received")]
    WebhookReceived {
        #[serde(default)]
        name: String,
        #[serde(default)]
        headers: serde_json::Map<String, Value>,
        #[serde(default)]
        body: String,
    },
    ReplayComplete {
        #[serde(default)]
        status: String,
        #[serde(default)]
        total_events: usize,
    },
}
