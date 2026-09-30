//! Wire types of the `workflow` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

/// The body of `workflow.start`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WorkflowSource {
    /// Full markdown source of the workflow note (frontmatter + body).
    pub source: String,
    /// Optional path used for title fallback / error messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// The body of `workflow.approve_gate`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GateRef {
    pub gate_id: String,
}

/// Reply from `workflow.start` and `workflow.approve_gate`: the same shape
/// either way.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WorkflowRunReply {
    pub session_id: String,
    pub status: crate::workflow::WorkflowStatus,
}

/// Reply from `workflow.status`.
///
/// Moved here from `crucible-daemon`'s `workflow_registry` module: nothing
/// in it named a daemon-local type.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WorkflowStatusReply {
    pub status: crate::workflow::WorkflowStatus,
    pub completed_slots: usize,
    pub total_slots: usize,
    /// The workflow's own output variables. Each step's outputs are
    /// arbitrary JSON that the step itself defines, so the map's values stay
    /// open even though the map itself is now named. `schema`'s override
    /// names the same alias `crate::workflow::OutputScope` expands to:
    /// `utoipa`'s derive needs the concrete generic spelled out here because
    /// `serde_json::Value` has a hand-written `ToSchema` (above) rather than
    /// a derived one, and only a derived one composes into a container's
    /// schema without help.
    #[cfg_attr(
        feature = "openapi",
        schema(value_type = std::collections::HashMap<String, serde_json::Value>)
    )]
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
