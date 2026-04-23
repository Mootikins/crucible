//! RPC handlers for workflow execution (Phase 3a).
//!
//! Three methods:
//! - `workflow.start`: parse source, create execution, drive until the
//!   first gate or terminal status, emit events.
//! - `workflow.approve_gate`: resolve a pending gate, drive until the
//!   next gate or terminal status.
//! - `workflow.status`: non-mutating snapshot of the run.
//!
//! The driver is synchronous within one RPC call: we tick until the
//! status is no longer `Running`. Long-running async work (LLM turns)
//! isn't here yet — the current `default` stdlib handler is pure.
//! When real inline execution lands, individual ticks will become
//! async and this loop will still be valid.

use crate::protocol::{RpcError, SessionEventMessage, INTERNAL_ERROR, INVALID_PARAMS};
use crate::rpc::context::RpcContext;
use crate::rpc::dispatch::RpcResult;
use crate::rpc::params::parse_params;
use crate::workflow_registry::{ExecutionHandle, WorkflowStatusSnapshot};
use crucible_core::parser::types::{Frontmatter, FrontmatterFormat, ParsedNote, WorkflowDoc};
use crucible_core::protocol::Request;
use crucible_core::workflow::{stdlib_dispatch, WorkflowEvent, WorkflowExecution, WorkflowStatus};
use serde::Deserialize;
use std::path::PathBuf;

pub async fn handle_workflow_start(
    ctx: &RpcContext,
    req: &Request,
) -> RpcResult<serde_json::Value> {
    #[derive(Deserialize)]
    struct Params {
        session_id: String,
        /// Full markdown source for the workflow note.
        source: String,
        /// Optional path for title fallback and error messages.
        path: Option<String>,
    }
    let p: Params = parse_params(req)?;

    if ctx.workflows.get(&p.session_id).is_some() {
        return Err(RpcError {
            code: INVALID_PARAMS,
            message: format!(
                "Workflow already running for session '{}'. Cancel or await completion first.",
                p.session_id
            ),
            data: None,
        });
    }

    let path = p
        .path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("workflow.md"));
    let fm = extract_yaml_frontmatter(&p.source);
    let mut note = ParsedNote::new(path);
    note.frontmatter = fm;

    let doc = WorkflowDoc::from_parsed(&note, &p.source).ok_or_else(|| RpcError {
        code: INVALID_PARAMS,
        message: "Note does not declare `type: workflow` in its frontmatter.".into(),
        data: None,
    })?;

    let exec = WorkflowExecution::new(doc, stdlib_dispatch());
    let handle = ctx.workflows.insert(&p.session_id, exec);

    let status = drive(ctx, &p.session_id, &handle).await;
    if status.is_terminal() {
        ctx.workflows.remove(&p.session_id);
    }

    Ok(serde_json::json!({
        "session_id": p.session_id,
        "status": status,
    }))
}

pub async fn handle_workflow_approve_gate(
    ctx: &RpcContext,
    req: &Request,
) -> RpcResult<serde_json::Value> {
    #[derive(Deserialize)]
    struct Params {
        session_id: String,
        gate_id: String,
    }
    let p: Params = parse_params(req)?;

    let handle = ctx.workflows.get(&p.session_id).ok_or_else(|| RpcError {
        code: INVALID_PARAMS,
        message: format!("No active workflow for session '{}'", p.session_id),
        data: None,
    })?;

    {
        let mut guard = handle.lock().await;
        guard.approve_gate(&p.gate_id).map_err(|e| RpcError {
            code: INVALID_PARAMS,
            message: e.to_string(),
            data: None,
        })?;
        // Flush the GateApproved event before we start driving.
        drain_and_broadcast(ctx, &p.session_id, &mut guard);
    }

    let status = drive(ctx, &p.session_id, &handle).await;
    if status.is_terminal() {
        ctx.workflows.remove(&p.session_id);
    }

    Ok(serde_json::json!({
        "session_id": p.session_id,
        "status": status,
    }))
}

pub async fn handle_workflow_status(
    ctx: &RpcContext,
    req: &Request,
) -> RpcResult<serde_json::Value> {
    #[derive(Deserialize)]
    struct Params {
        session_id: String,
    }
    let p: Params = parse_params(req)?;

    let handle = ctx.workflows.get(&p.session_id).ok_or_else(|| RpcError {
        code: INVALID_PARAMS,
        message: format!("No active workflow for session '{}'", p.session_id),
        data: None,
    })?;

    let guard = handle.lock().await;
    let snapshot = WorkflowStatusSnapshot {
        status: guard.status().clone(),
        completed_slots: guard.completed_slots(),
        total_slots: guard.total_slots(),
        scope: serde_json::to_value(guard.scope()).map_err(|e| RpcError {
            code: INTERNAL_ERROR,
            message: format!("scope serialization: {}", e),
            data: None,
        })?,
    };
    serde_json::to_value(snapshot).map_err(|e| RpcError {
        code: INTERNAL_ERROR,
        message: format!("snapshot serialization: {}", e),
        data: None,
    })
}

pub async fn handle_workflow_cancel(
    ctx: &RpcContext,
    req: &Request,
) -> RpcResult<serde_json::Value> {
    #[derive(Deserialize)]
    struct Params {
        session_id: String,
    }
    let p: Params = parse_params(req)?;

    let handle = match ctx.workflows.get(&p.session_id) {
        Some(h) => h,
        None => {
            return Ok(serde_json::json!({
                "session_id": p.session_id,
                "status": "not_found",
            }));
        }
    };

    {
        let mut guard = handle.lock().await;
        guard.cancel();
        drain_and_broadcast(ctx, &p.session_id, &mut guard);
    }
    ctx.workflows.remove(&p.session_id);

    Ok(serde_json::json!({
        "session_id": p.session_id,
        "status": "cancelled",
    }))
}

// ---------- driver ----------

async fn drive(ctx: &RpcContext, session_id: &str, handle: &ExecutionHandle) -> WorkflowStatus {
    let mut guard = handle.lock().await;
    loop {
        let status = guard.tick().clone();
        drain_and_broadcast(ctx, session_id, &mut guard);
        if !matches!(&status, WorkflowStatus::Running) {
            return status;
        }
    }
}

fn drain_and_broadcast(ctx: &RpcContext, session_id: &str, exec: &mut WorkflowExecution) {
    for event in exec.drain_events() {
        let msg = workflow_event_to_message(session_id, event);
        let _ = ctx.event_tx.send(msg);
    }
}

fn workflow_event_to_message(session_id: &str, ev: WorkflowEvent) -> SessionEventMessage {
    match ev {
        WorkflowEvent::StepStarted { step_id, title } => {
            SessionEventMessage::workflow_step_started(session_id, step_id, title)
        }
        WorkflowEvent::StepCompleted {
            step_id,
            output_name,
        } => SessionEventMessage::workflow_step_completed(session_id, step_id, output_name),
        WorkflowEvent::GateReached {
            gate_id,
            title,
            owner,
        } => SessionEventMessage::workflow_gate_reached(session_id, gate_id, title, owner),
        WorkflowEvent::GateApproved { gate_id } => {
            SessionEventMessage::workflow_gate_approved(session_id, gate_id)
        }
        WorkflowEvent::WorkflowCompleted => SessionEventMessage::workflow_completed(session_id),
        WorkflowEvent::WorkflowFailed { reason, at_step } => {
            SessionEventMessage::workflow_failed(session_id, reason, at_step)
        }
        WorkflowEvent::WorkflowCancelled => SessionEventMessage::workflow_cancelled(session_id),
    }
}

fn extract_yaml_frontmatter(source: &str) -> Option<Frontmatter> {
    let rest = source.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    Some(Frontmatter::new(
        rest[..end].to_string(),
        FrontmatterFormat::Yaml,
    ))
}
