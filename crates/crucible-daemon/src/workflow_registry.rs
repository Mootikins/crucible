//! Daemon-side registry of active workflow executions.
//!
//! Thin wrapper around a `DashMap` that holds one
//! [`WorkflowExecution`] per active workflow session. Every
//! workflow-session RPC handler goes through this — `workflow.start`
//! inserts, `workflow.approve_gate`/`workflow.status` look up,
//! `workflow.cancel` or the normal session.end path removes.

use crucible_core::workflow::WorkflowExecution;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Handle to the per-session execution. We wrap in `Arc<Mutex<_>>` so
/// concurrent subscribers and the driver task all share one state
/// without copying.
pub type ExecutionHandle = Arc<Mutex<WorkflowExecution>>;

/// Each run with its cancel. The driver holds the execution lock across
/// its steps, so `workflow.cancel` sets the token without the lock, and the
/// driver reads it before each step.
#[derive(Default)]
pub struct WorkflowRegistry {
    inner: DashMap<String, (ExecutionHandle, CancellationToken)>,
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
        self.inner.insert(
            session_id.into(),
            (handle.clone(), CancellationToken::new()),
        );
        handle
    }

    pub fn get(&self, session_id: &str) -> Option<ExecutionHandle> {
        self.inner.get(session_id).map(|e| e.0.clone())
    }

    pub fn cancel_token(&self, session_id: &str) -> Option<CancellationToken> {
        self.inner.get(session_id).map(|e| e.1.clone())
    }

    pub fn remove(&self, session_id: &str) -> Option<ExecutionHandle> {
        self.inner.remove(session_id).map(|(_, (handle, _))| handle)
    }
}
