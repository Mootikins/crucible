//! Workflow execution RPC methods (Phase 3a).
//!
//! CLI-side bindings for `workflow.start`, `workflow.approve_gate`,
//! `workflow.status`, `workflow.cancel`. Progress events arrive via the
//! existing `session.subscribe` stream as `workflow.step_started`,
//! `workflow.gate_reached`, etc.

use anyhow::Result;

use super::DaemonClient;

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

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowSessionRequest {
    pub session_id: String,
}

impl DaemonClient {
    pub async fn workflow_start(&self, req: WorkflowStartRequest) -> Result<serde_json::Value> {
        self.call("workflow.start", serde_json::to_value(req)?)
            .await
    }

    pub async fn workflow_approve_gate(
        &self,
        req: WorkflowApproveGateRequest,
    ) -> Result<serde_json::Value> {
        self.call("workflow.approve_gate", serde_json::to_value(req)?)
            .await
    }

    pub async fn workflow_status(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "workflow.status",
            serde_json::to_value(WorkflowSessionRequest {
                session_id: session_id.to_string(),
            })?,
        )
        .await
    }

    pub async fn workflow_cancel(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "workflow.cancel",
            serde_json::to_value(WorkflowSessionRequest {
                session_id: session_id.to_string(),
            })?,
        )
        .await
    }
}
