//! Wire types of the `workflow` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

/// The body of `workflow.start`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowSource {
    /// Full markdown source of the workflow note (frontmatter + body).
    pub source: String,
    /// Optional path used for title fallback / error messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// The body of `workflow.approve_gate`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GateRef {
    pub gate_id: String,
}

/// Reply from `workflow.start` and `workflow.approve_gate`: the same shape
/// either way.
// No `ToSchema` here: `WorkflowStatus` (below) is `#[serde(tag = "kind")]`
// over a case with a nested struct, and giving it one cascades into
// `PendingGate` and the parser's `WorkflowDoc` for no reader that needs it
// yet (the web route of step 19's own "not started" part is what would).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowRunReply {
    pub session_id: String,
    pub status: crate::workflow::WorkflowStatus,
}

/// Reply from `workflow.status`.
///
/// Moved here from `crucible-daemon`'s `workflow_registry` module: nothing
/// in it named a daemon-local type. No `ToSchema`, for the reason on
/// [`WorkflowRunReply`] plus `scope`'s open `serde_json::Value`s.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct WorkflowStatusReply {
    pub status: crate::workflow::WorkflowStatus,
    pub completed_slots: usize,
    pub total_slots: usize,
    /// The workflow's own output variables. Each step's outputs are
    /// arbitrary JSON that the step itself defines, so the map's values stay
    /// open even though the map itself is now named.
    pub scope: crate::workflow::OutputScope,
}

/// Reply from `workflow.cancel`.
///
/// `status` is `"cancelled"`, or `"not_found"` when the session had no
/// active or persisted run — not a [`crate::workflow::WorkflowStatus`]
/// variant, since "there was never a run" is not a state a run can be in.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WorkflowCancelReply {
    pub session_id: String,
    pub status: String,
}
