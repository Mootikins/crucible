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
