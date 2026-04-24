//! RPC handlers for workflow execution.
//!
//! Four methods:
//! - `workflow.start`: parse source, create execution, drive until the
//!   first gate or terminal status, emit events.
//! - `workflow.approve_gate`: resolve a pending gate, drive until the
//!   next gate or terminal status.
//! - `workflow.status`: non-mutating snapshot of the run.
//! - `workflow.cancel`: terminate a run early.
//!
//! `tick()` is async — the default stdlib handler is replaced with
//! [`DaemonInlineHandler`] which drives a full session turn via
//! `AgentManager::send_message`. The driver holds the registry mutex
//! across awaits, so `workflow.status` queued during a turn will wait
//! rather than racing.
//!
//! On `Completed` the handler runs each runnable `## Validation` entry
//! via `bash -c` and emits a `workflow.assessed` event before pruning
//! the run from the registry. On any non-terminal state change we
//! persist a [`WorkflowSnapshot`] next to the session metadata so a
//! daemon restart can transparently pick the run up where it paused —
//! the per-handler lookup goes through [`resolve_or_rehydrate`].

use crate::protocol::{RpcError, SessionEventMessage, INTERNAL_ERROR, INVALID_PARAMS};
use crate::rpc::context::RpcContext;
use crate::rpc::dispatch::RpcResult;
use crate::rpc::params::parse_params;
use crate::session_storage::FileSessionStorage;
use crate::workflow_handlers::DaemonInlineHandler;
use crate::workflow_registry::{ExecutionHandle, WorkflowStatusSnapshot};
use crucible_core::parser::types::{Frontmatter, FrontmatterFormat, ParsedNote, WorkflowDoc};
use crucible_core::protocol::Request;
use crucible_core::workflow::{
    DefaultHandler, DispatchTable, GateHandler, WorkflowEvent, WorkflowExecution, WorkflowSnapshot,
    WorkflowStatus,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};

const DRY_RUN_ENV: &str = "CRUCIBLE_WORKFLOW_DRY_RUN";
const WORKFLOW_STATE_FILE: &str = "workflow.json";

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

    // Reject both live executions and persisted snapshots from prior
    // runs that haven't reached a terminal state — otherwise we'd
    // silently clobber an in-flight workflow that the user is still
    // about to approve.
    if resolve_or_rehydrate(ctx, &p.session_id).await.is_some() {
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

    let dispatch = build_dispatch(ctx, &p.session_id);
    let exec = WorkflowExecution::new(doc, dispatch);
    let handle = ctx.workflows.insert(&p.session_id, exec);

    // Persist the initial snapshot before driving so a crash mid-turn
    // on the very first RPC is still recoverable — finalize only runs
    // after drive() returns, so without this we'd lose everything up
    // to and including the first gate if the daemon dies during the
    // first step's LLM turn.
    {
        let guard = handle.lock().await;
        let snap = guard.snapshot();
        drop(guard);
        persist_snapshot(ctx, &p.session_id, &snap).await;
    }

    let status = drive(ctx, &p.session_id, &handle).await;
    finalize(ctx, &p.session_id, &handle, &status).await;

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

    let handle = resolve_or_rehydrate(ctx, &p.session_id)
        .await
        .ok_or_else(|| RpcError {
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
    finalize(ctx, &p.session_id, &handle, &status).await;

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

    let handle = resolve_or_rehydrate(ctx, &p.session_id)
        .await
        .ok_or_else(|| RpcError {
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

    let handle = match resolve_or_rehydrate(ctx, &p.session_id).await {
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
    remove_snapshot(ctx, &p.session_id).await;

    Ok(serde_json::json!({
        "session_id": p.session_id,
        "status": "cancelled",
    }))
}

// ---------- dispatch setup ----------

/// Build the dispatch table used for one workflow run. `default` points
/// at the real [`DaemonInlineHandler`] that drives an LLM turn, unless
/// `CRUCIBLE_WORKFLOW_DRY_RUN=1` is set — in which case the pure
/// placeholder ships through so tests and demos don't hit the model.
fn build_dispatch(ctx: &RpcContext, session_id: &str) -> DispatchTable {
    let dry_run = std::env::var(DRY_RUN_ENV)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let default: Box<dyn crucible_core::workflow::StepHandler> = if dry_run {
        Box::new(DefaultHandler)
    } else {
        Box::new(DaemonInlineHandler::new(
            session_id,
            ctx.agents.clone(),
            ctx.event_tx.clone(),
        ))
    };

    let mut table = DispatchTable::new(default);
    table.register("gate", Box::new(GateHandler));
    table
}

// ---------- driver ----------

async fn drive(ctx: &RpcContext, session_id: &str, handle: &ExecutionHandle) -> WorkflowStatus {
    let mut guard = handle.lock().await;
    loop {
        let status = guard.tick().await.clone();
        drain_and_broadcast(ctx, session_id, &mut guard);
        if !matches!(&status, WorkflowStatus::Running) {
            return status;
        }
    }
}

/// Terminal cleanup shared by `start` and `approve_gate` handlers. On
/// `Completed`, runs the workflow's `## Validation` commands, ships the
/// outcome as a `workflow.assessed` event, and then prunes the
/// registry. Other terminal states (`Failed`, `Cancelled`) only prune.
/// Also persists (non-terminal) or removes (terminal) the on-disk
/// snapshot so a daemon restart can rehydrate an in-flight run.
async fn finalize(
    ctx: &RpcContext,
    session_id: &str,
    handle: &ExecutionHandle,
    status: &WorkflowStatus,
) {
    if matches!(status, WorkflowStatus::Completed) {
        let validations = {
            let guard = handle.lock().await;
            guard.doc().validations.clone()
        };
        run_and_emit_assessment(ctx, session_id, &validations).await;
    }
    if status.is_terminal() {
        ctx.workflows.remove(session_id);
        remove_snapshot(ctx, session_id).await;
    } else {
        let snap = {
            let guard = handle.lock().await;
            guard.snapshot()
        };
        persist_snapshot(ctx, session_id, &snap).await;
    }
}

// ---------- snapshot persistence ----------

/// Look up the execution handle for a session; if absent, try to
/// rehydrate from an on-disk snapshot. Returns `None` only when the
/// session genuinely has no live or persisted workflow state.
async fn resolve_or_rehydrate(ctx: &RpcContext, session_id: &str) -> Option<ExecutionHandle> {
    if let Some(h) = ctx.workflows.get(session_id) {
        return Some(h);
    }
    let session = ctx.sessions.get_session(session_id)?;
    let path = FileSessionStorage::session_dir_for(&session).join(WORKFLOW_STATE_FILE);
    let snapshot = read_snapshot(&path).await?;
    if snapshot.status.is_terminal() {
        // Terminal state shouldn't be on disk — clean it up so a later
        // start on this session isn't blocked by the phantom entry.
        let _ = tokio::fs::remove_file(&path).await;
        return None;
    }
    let dispatch = build_dispatch(ctx, session_id);
    let exec = WorkflowExecution::rehydrate(snapshot, dispatch);
    Some(ctx.workflows.insert(session_id, exec))
}

async fn read_snapshot(path: &Path) -> Option<WorkflowSnapshot> {
    let bytes = tokio::fs::read(path).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

async fn persist_snapshot(ctx: &RpcContext, session_id: &str, snapshot: &WorkflowSnapshot) {
    let Some(session) = ctx.sessions.get_session(session_id) else {
        return;
    };
    let dir = FileSessionStorage::session_dir_for(&session);
    let path = dir.join(WORKFLOW_STATE_FILE);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::warn!(session_id = %session_id, error = %e, "failed to create session dir for workflow snapshot");
        return;
    }
    let json = match serde_json::to_vec_pretty(snapshot) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!(session_id = %session_id, error = %e, "failed to serialize workflow snapshot");
            return;
        }
    };
    if let Err(e) = tokio::fs::write(&path, &json).await {
        tracing::warn!(session_id = %session_id, path = %path.display(), error = %e, "failed to persist workflow snapshot");
    }
}

async fn remove_snapshot(ctx: &RpcContext, session_id: &str) {
    let Some(session) = ctx.sessions.get_session(session_id) else {
        return;
    };
    let path = FileSessionStorage::session_dir_for(&session).join(WORKFLOW_STATE_FILE);
    let _ = tokio::fs::remove_file(path).await;
}

async fn run_and_emit_assessment(
    ctx: &RpcContext,
    session_id: &str,
    validations: &[crucible_core::parser::types::ValidationEntry],
) {
    let mut passed = Vec::new();
    let mut failed = Vec::new();
    let mut manual = Vec::new();

    for entry in validations {
        match &entry.command {
            Some(cmd) => {
                let outcome = run_validation_command(&entry.description, cmd).await;
                if outcome.exit_code == 0 {
                    passed.push(outcome);
                } else {
                    failed.push(outcome);
                }
            }
            None => manual.push(entry.description.clone()),
        }
    }

    let msg = SessionEventMessage::workflow_assessed(session_id, &passed, &failed, &manual);
    let _ = ctx.event_tx.send(msg);
}

const VALIDATION_OUTPUT_CAP: usize = 4096;
const VALIDATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

async fn run_validation_command(
    description: &str,
    command: &str,
) -> crucible_core::workflow::AssessmentOutcome {
    use crucible_core::workflow::AssessmentOutcome;
    use std::process::Stdio;
    use std::time::Instant;
    use tokio::process::Command;

    let started = Instant::now();
    let result = tokio::time::timeout(
        VALIDATION_TIMEOUT,
        Command::new("bash")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            AssessmentOutcome {
                description: description.to_string(),
                command: command.to_string(),
                exit_code,
                stdout: truncate_utf8_lossy(&output.stdout, VALIDATION_OUTPUT_CAP),
                stderr: truncate_utf8_lossy(&output.stderr, VALIDATION_OUTPUT_CAP),
                duration_ms: started.elapsed().as_millis() as u64,
            }
        }
        Ok(Err(err)) => AssessmentOutcome {
            description: description.to_string(),
            command: command.to_string(),
            exit_code: -1,
            stdout: String::new(),
            stderr: format!("spawn error: {err}"),
            duration_ms: started.elapsed().as_millis() as u64,
        },
        Err(_) => AssessmentOutcome {
            description: description.to_string(),
            command: command.to_string(),
            exit_code: -1,
            stdout: String::new(),
            stderr: format!("timed out after {}s", VALIDATION_TIMEOUT.as_secs()),
            duration_ms: started.elapsed().as_millis() as u64,
        },
    }
}

fn truncate_utf8_lossy(bytes: &[u8], cap: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    if s.len() <= cap {
        return s.into_owned();
    }
    let mut end = cap;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = s[..end].to_string();
    out.push_str("…[truncated]");
    out
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
