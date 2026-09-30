//! Workflow execution RPC methods (Phase 3a).
//!
//! CLI-side bindings for `workflow.start`, `workflow.approve_gate`,
//! `workflow.status`, `workflow.cancel`. Progress events arrive via the
//! existing `session.subscribe` stream as `workflow.step_started`,
//! `workflow.gate_reached`, etc.

use anyhow::Result;
use crucible_core::protocol::RpcMethod;

use super::DaemonClient;
use crucible_core::protocol::requests::Scoped;

impl DaemonClient {
    // `workflow_start`/`workflow_approve_gate` used to live here as thin
    // forwarders (the row's own `Scoped<...>` in, `serde_json::Value` out,
    // no argument reshaping). `client.rpc_workflow_start(req)` /
    // `.rpc_workflow_approve_gate(req)` replace them, bound to their
    // `rpc_methods!` row instead of choosing `Req`/`Resp` freely — see
    // `rpc_client::client::generated` and gap 1 of step 19 in
    // `docs/Meta/Architecture/Simplification Plan.md`.

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
