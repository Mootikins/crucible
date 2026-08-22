//! Daemon-side registry of active workflow executions.
//!
//! Thin wrapper around a `DashMap` that holds one
//! [`WorkflowExecution`] per active workflow session. Every
//! workflow-session RPC handler goes through this — `workflow.start`
//! inserts, `workflow.approve_gate`/`workflow.status` look up,
//! `workflow.cancel` or the normal session.end path removes.

use crucible_core::workflow::{WorkflowExecution, WorkflowStatus};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Handle to the per-session execution. We wrap in `Arc<Mutex<_>>` so
/// concurrent subscribers and the driver task all share one state
/// without copying.
pub type ExecutionHandle = Arc<Mutex<WorkflowExecution>>;

#[derive(Default)]
pub struct WorkflowRegistry {
    inner: DashMap<String, ExecutionHandle>,
}

impl WorkflowRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &self,
        session_id: impl Into<String>,
        exec: WorkflowExecution,
    ) -> ExecutionHandle {
        let handle = Arc::new(Mutex::new(exec));
        self.inner.insert(session_id.into(), handle.clone());
        handle
    }

    pub fn get(&self, session_id: &str) -> Option<ExecutionHandle> {
        self.inner.get(session_id).map(|e| e.clone())
    }

    pub fn remove(&self, session_id: &str) -> Option<ExecutionHandle> {
        self.inner.remove(session_id).map(|(_, handle)| handle)
    }
}

/// Shorthand for what a handler returns on `workflow.status`.
#[derive(Debug, serde::Serialize)]
pub struct WorkflowStatusSnapshot {
    pub status: WorkflowStatus,
    pub completed_slots: usize,
    pub total_slots: usize,
    pub scope: serde_json::Value,
}
