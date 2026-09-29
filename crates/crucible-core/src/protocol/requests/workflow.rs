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
