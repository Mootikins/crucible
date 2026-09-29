//! Workflow execution RPC methods (Phase 3a).
//!
//! CLI-side bindings for `workflow.start`, `workflow.approve_gate`,
//! `workflow.status`, `workflow.cancel`. Progress events arrive via the
//! existing `session.subscribe` stream as `workflow.step_started`,
//! `workflow.gate_reached`, etc.

use anyhow::Result;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;

use super::DaemonClient;
use crucible_core::protocol::requests::Scoped;

impl DaemonClient {
    pub async fn workflow_start(&self, req: Scoped<WorkflowSource>) -> Result<serde_json::Value> {
        self.call(RpcMethod::WorkflowStart, serde_json::to_value(req)?)
            .await
    }

    pub async fn workflow_approve_gate(&self, req: Scoped<GateRef>) -> Result<serde_json::Value> {
        self.call(RpcMethod::WorkflowApproveGate, serde_json::to_value(req)?)
            .await
    }

    pub async fn workflow_status(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::WorkflowStatus,
            serde_json::to_value(Scoped::session(session_id.to_string()))?,
        )
        .await
    }

    pub async fn workflow_cancel(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::WorkflowCancel,
            serde_json::to_value(Scoped::session(session_id.to_string()))?,
        )
        .await
    }
}
