//! Wire types of the `workflow` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowStartRequest {
    pub session_id: String,
    /// Full markdown source of the workflow note (frontmatter + body).
    pub source: String,
    /// Optional path used for title fallback / error messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowApproveGateRequest {
    pub session_id: String,
    pub gate_id: String,
}
