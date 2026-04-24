//! Events emitted during workflow execution.
//!
//! The daemon bridges these into `SessionEventMessage` so clients
//! subscribe via the existing `session.subscribe` RPC. The engine itself
//! only knows about these domain events — wire translation is daemon's
//! job.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowEvent {
    StepStarted {
        step_id: String,
        title: String,
    },
    StepCompleted {
        step_id: String,
        /// Named output bound into the scope for this step, if any.
        output_name: Option<String>,
    },
    GateReached {
        gate_id: String,
        title: Option<String>,
        /// `step_id` of the step that owns this gate, or `"preamble"`.
        owner: String,
    },
    GateApproved {
        gate_id: String,
    },
    WorkflowCompleted,
    WorkflowFailed {
        reason: String,
        /// `step_id` where failure occurred, if applicable.
        at_step: Option<String>,
    },
    WorkflowCancelled,
}

/// Captured result of running a single validation command. The daemon
/// executes each runnable entry in `WorkflowDoc.validations` after a
/// workflow completes and ships the outcomes over the session event
/// stream as a `workflow.assessed` message. Kept in core so that
/// factory/consumer share one shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentOutcome {
    pub description: String,
    pub command: String,
    pub exit_code: i32,
    /// Truncated stdout (daemon-side cap).
    pub stdout: String,
    /// Truncated stderr (daemon-side cap).
    pub stderr: String,
    pub duration_ms: u64,
}
