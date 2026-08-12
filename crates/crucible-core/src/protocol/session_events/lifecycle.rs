//! The job, review, notification, workflow and system groups.
//!
//! These five are grouped in one file because none of them is big enough to
//! carry a module of its own, and every consumer that matches one of them
//! matches it in a single arm.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::events::session_event::FileChangeKind;
use crate::workflow::AssessmentOutcome;

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
        path: PathBuf,
        #[serde(default)]
        kind: FileChangeKind,
    },
    FileDeleted {
        path: PathBuf,
    },
    /// Carries `from`/`to` and **no** `path`. The consumer that rebuilt the
    /// typed event read `data["path"]` before matching the name, so this event
    /// was broadcast and could never reach a Lua handler.
    FileMoved {
        from: PathBuf,
        to: PathBuf,
    },
    ClassificationRequired {
        #[serde(default)]
        kiln_path: String,
    },
    /// The `type` key duplicates the envelope's `event` name inside `data`.
    /// Redundant, and on the wire since the first release, so it stays.
    ProcessStart {
        #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
        type_: Option<String>,
        #[serde(default)]
        batch_id: String,
        #[serde(default)]
        total: usize,
        #[serde(default)]
        kiln: String,
    },
    ProcessProgress {
        #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
        type_: Option<String>,
        #[serde(default)]
        batch_id: String,
        #[serde(default)]
        file: String,
        #[serde(default)]
        result: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error_msg: Option<String>,
    },
    /// Two producers, two shapes: the batch path carries `type`/`batch_id`, the
    /// single-kiln path carries `kiln`/`discovered`. Every field is therefore
    /// omissible. Two events wearing one name; recorded here rather than split,
    /// because splitting is a client-visible rename.
    ProcessComplete {
        #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
        type_: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        batch_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kiln: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        discovered: Option<usize>,
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
