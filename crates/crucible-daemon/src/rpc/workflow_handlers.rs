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
//! the run from the registry. The command text comes out of a workflow
//! note, so it is attacker-supplied — anything that can write a
//! `type: workflow` note into a kiln chooses it, `create_note` included.
//! Every entry therefore goes through the same two fail-closed gates as
//! `cru.tools.call`, in the same order — the session's isolation claim
//! (`tools_bridge::isolated_session_refusal`) and then the session's
//! `[permissions]` rules (`tools_bridge::unattended_refusal`) — before
//! it reaches a shell; a refused entry is reported as a failed
//! assessment with [`VALIDATION_REFUSED_EXIT_CODE`] rather than run.
//! On any non-terminal state change we
//! persist a [`WorkflowSnapshot`] next to the session metadata so a
//! daemon restart can transparently pick the run up where it paused —
//! the per-handler lookup goes through [`resolve_or_rehydrate`].

use crate::protocol::{RpcError, SessionEventMessage, INTERNAL_ERROR, INVALID_PARAMS};
use crate::rpc::context::RpcContext;
use crate::rpc::dispatch::RpcResult;
use crate::rpc::params::parse_params;
use crate::workflow_handlers::DaemonInlineHandler;
use crate::workflow_registry::{ExecutionHandle, WorkflowStatusSnapshot};
use crucible_core::config::components::permissions::PermissionEngine;
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
    // The path comes off the resident session's own validated id, never off
    // the caller's string.
    let session = ctx.sessions.get_session(session_id)?;
    let path = ctx
        .sessions
        .session_dir(&session.id)
        .join(WORKFLOW_STATE_FILE);
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
    let dir = ctx.sessions.session_dir(&session.id);
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
    let path = ctx
        .sessions
        .session_dir(&session.id)
        .join(WORKFLOW_STATE_FILE);
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

    // The rules that apply to *this session*, read once per assessment: the
    // session's agent profile permissions where it has them, the daemon-global
    // `[permissions]` otherwise. Resolved by the same helper the agent
    // dispatch path uses, so a session stricter than global stays stricter
    // here. See `AgentManager::session_permission_config` for the one input it
    // cannot see (the per-turn `permission_mode` override).
    let permissions =
        PermissionEngine::new(ctx.agents.session_permission_config(session_id).as_ref());
    // The plugin isolation claims, for the second half of the gate. A
    // validation command spawns `bash` on the host, so a session a plugin
    // sandboxed must not reach it.
    let isolation = ctx.agents.isolation();

    for entry in validations {
        match &entry.command {
            Some(cmd) => {
                let outcome = run_validation_command(
                    &permissions,
                    isolation.as_ref(),
                    session_id,
                    &entry.description,
                    cmd,
                )
                .await;
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
    crate::event_emitter::emit_event(&ctx.event_tx, msg);
}

const VALIDATION_OUTPUT_CAP: usize = 4096;
const VALIDATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// `exit_code` reported for an entry a gate refused before it reached a shell.
///
/// Distinct from the `-1` the spawn-error and timeout branches use — and from
/// the `-1` a signal-killed shell reports — so a `workflow.assessed` consumer
/// can tell "refused by policy" from "bash was missing". A real shell never
/// produces it: `bash -c` exits 0..=255, and `ExitStatus::code()` is `None`
/// only on a signal, which the spawn branch already maps to `-1`.
const VALIDATION_REFUSED_EXIT_CODE: i32 = -2;

/// Run one `## Validation` command, if both gates permit it.
///
/// Two gates, in the order `cru.tools.call` applies them.
///
/// **Isolation** (`tools_bridge::isolated_session_refusal`): this spawns
/// `bash` on the host, so a session a plugin sandboxed must not reach it —
/// otherwise turning on the container held for the agent and not for the
/// assessment that runs after it. The session is always stated here (the
/// assessment is *for* a session), so the gate's not-stated fallback never
/// applies and nothing is softened to get there.
///
/// **Permissions** (`tools_bridge::unattended_refusal`), asked the same
/// question `cru.tools.call("bash", { command = ... })` asks: a `deny` is
/// absolute, an `allow` runs, and an `ask` refuses because a workflow
/// assessment has no user attached to prompt. With the shipped default
/// (`default = ask`, and `bash` is not read-only) that means an unconfigured
/// daemon runs nothing here until an `allow` rule covers the command — which
/// is the point. The command text is chosen by whoever wrote the note.
///
/// A refusal is an `AssessmentOutcome` with
/// [`VALIDATION_REFUSED_EXIT_CODE`], so it lands in the `failed` bucket of
/// `workflow.assessed`, the operator sees why in `stderr`, and a consumer can
/// tell it from a shell that failed to start.
async fn run_validation_command(
    permissions: &PermissionEngine,
    isolation: Option<&crucible_lua::IsolationRegistry>,
    session_id: &str,
    description: &str,
    command: &str,
) -> crucible_core::workflow::AssessmentOutcome {
    use crucible_core::workflow::AssessmentOutcome;
    use std::process::Stdio;
    use std::time::Instant;
    use tokio::process::Command;

    let started = Instant::now();

    let refusal = isolation
        .and_then(|isolation| {
            crate::tools_bridge::isolated_session_refusal(
                isolation,
                "bash",
                session_id,
                // Stated, not asked of an executor: this function spawns the
                // process itself, so `Host` is a fact about the code below.
                crucible_core::traits::tools::ToolSurface::Host,
                "a workflow validation command",
            )
        })
        .or_else(|| {
            crate::tools_bridge::unattended_refusal(
                permissions,
                "bash",
                &serde_json::json!({ "command": command }),
                "a workflow validation command",
            )
        });

    if let Some(reason) = refusal {
        tracing::warn!(%command, %reason, "refused workflow validation command");
        return AssessmentOutcome {
            description: description.to_string(),
            command: command.to_string(),
            exit_code: VALIDATION_REFUSED_EXIT_CODE,
            stdout: String::new(),
            stderr: format!("Permission denied: {reason}"),
            duration_ms: started.elapsed().as_millis() as u64,
        };
    }

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
        crate::event_emitter::emit_event(&ctx.event_tx, msg);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::components::permissions::{PermissionConfig, PermissionMode};

    fn engine(config: PermissionConfig) -> PermissionEngine {
        PermissionEngine::new(Some(&config))
    }

    /// A marker path plus the command that would create it. The command is the
    /// payload: if the marker exists afterwards, a shell ran the note's text.
    fn payload(tmp: &tempfile::TempDir) -> (std::path::PathBuf, String) {
        let marker = tmp.path().join("pwned");
        let command = format!("touch {}", marker.display());
        (marker, command)
    }

    /// The permission cases, on a daemon with no sandboxing plugin loaded —
    /// no isolation registry is bound at all, which is the ordinary case.
    async fn run_unsandboxed(
        permissions: &PermissionEngine,
        description: &str,
        command: &str,
    ) -> crucible_core::workflow::AssessmentOutcome {
        run_validation_command(permissions, None, "s-workflow", description, command).await
    }

    /// The `[acp.agents.*]` entry a fixture session's agent names.
    const PROFILE_NAME: &str = "strict-agent";

    /// Permissions wide open for the payload, so any refusal below is the
    /// isolation gate and not the permission one.
    fn permissive() -> PermissionConfig {
        PermissionConfig {
            default: PermissionMode::Allow,
            allow: vec!["bash:touch *".to_string()],
            ..Default::default()
        }
    }

    fn isolation_claiming(session: &str) -> crucible_lua::IsolationRegistry {
        let registry = crucible_lua::IsolationRegistry::new();
        registry.claim(
            session,
            crucible_lua::IsolationClaim {
                plugin: "oci".to_string(),
                exempt: Default::default(),
                exec: Default::default(),
            },
        );
        registry
    }

    /// A workflow note's `## Validation` command is attacker-supplied text:
    /// anything that can write a `type: workflow` note into a kiln — an agent
    /// with `create_note` included — decides what this runs. It went straight
    /// to `bash -c` with no gate at all, so the next run of that workflow was
    /// arbitrary host execution.
    ///
    /// The shipped default is `default = ask` and `bash` is not read-only, so
    /// an unconfigured daemon must refuse.
    #[tokio::test]
    async fn an_unpermitted_validation_command_does_not_run() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (marker, command) = payload(&tmp);

        let outcome = run_unsandboxed(
            &engine(PermissionConfig::default()),
            "prove the host is reachable",
            &command,
        )
        .await;

        assert!(
            !marker.exists(),
            "an ungated `## Validation` command executed on the host"
        );
        assert_ne!(outcome.exit_code, 0, "a refused command must not pass");
        assert!(
            outcome.stderr.contains("Permission denied"),
            "the refusal must say why, got: {}",
            outcome.stderr
        );
    }

    /// An operator `deny` is absolute — it outranks a permissive default, the
    /// way it does on every other path.
    #[tokio::test]
    async fn an_operator_deny_outranks_a_permissive_default() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (marker, command) = payload(&tmp);

        let outcome = run_unsandboxed(
            &engine(PermissionConfig {
                default: PermissionMode::Allow,
                deny: vec!["bash:touch *".to_string()],
                ..Default::default()
            }),
            "denied",
            &command,
        )
        .await;

        assert!(!marker.exists(), "a denied command executed anyway");
        assert_ne!(outcome.exit_code, 0);
    }

    /// An `ask` rule names this command on purpose, and a workflow assessment
    /// has nobody to prompt. Being asked about it therefore means refuse —
    /// the same call the Lua bridge makes, and not the read-only exemption,
    /// which is only for commands no rule decided.
    #[tokio::test]
    async fn an_ask_rule_refuses_because_there_is_nobody_to_prompt() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (marker, command) = payload(&tmp);

        let outcome = run_unsandboxed(
            &engine(PermissionConfig {
                default: PermissionMode::Allow,
                ask: vec!["bash:touch *".to_string()],
                ..Default::default()
            }),
            "asked about",
            &command,
        )
        .await;

        assert!(!marker.exists(), "an `ask` command executed unprompted");
        assert!(
            outcome.stderr.contains("no prompt"),
            "got: {}",
            outcome.stderr
        );
    }

    /// ...and the feature still works. A command the operator allowed runs and
    /// reports its real exit code, so gating it did not turn validation off.
    #[tokio::test]
    async fn an_allowed_validation_command_still_runs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (marker, command) = payload(&tmp);

        let outcome = run_unsandboxed(
            &engine(PermissionConfig {
                allow: vec!["bash:touch *".to_string()],
                ..Default::default()
            }),
            "allowed",
            &command,
        )
        .await;

        assert_eq!(outcome.exit_code, 0, "stderr: {}", outcome.stderr);
        assert!(marker.exists(), "an allowed command must actually run");
    }

    /// A command the rules leave undecided is still refused, because `bash`
    /// can modify state. The read-only exemption is what makes the gate usable
    /// elsewhere; it must not open this one.
    #[tokio::test]
    async fn an_allow_rule_for_another_command_does_not_cover_this_one() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (marker, command) = payload(&tmp);

        let outcome = run_unsandboxed(
            &engine(PermissionConfig {
                allow: vec!["bash:cargo test *".to_string()],
                ..Default::default()
            }),
            "not the allowed command",
            &command,
        )
        .await;

        assert!(!marker.exists(), "an unrelated allow rule opened the shell");
        assert_ne!(outcome.exit_code, 0);
    }

    /// The permission gate is only half of what `cru.tools.call` applies.
    ///
    /// A workflow assessment spawns `bash` on the *host*. On a session a
    /// plugin sandboxed, that is the sandbox escape the isolation claim exists
    /// to stop: the container held for the agent's own tool calls and then the
    /// assessment ran beside it, on the host, with the note's text.
    ///
    /// Permissions are wide open here on purpose, so only the isolation gate
    /// can be what refuses.
    #[tokio::test]
    async fn a_sandboxed_session_cannot_reach_the_host_through_a_validation_command() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (marker, command) = payload(&tmp);
        let isolation = isolation_claiming("s-sandboxed");

        let outcome = run_validation_command(
            &engine(permissive()),
            Some(&isolation),
            "s-sandboxed",
            "prove the container is escapable",
            &command,
        )
        .await;

        assert!(
            !marker.exists(),
            "a validation command ran on the host inside an isolated session"
        );
        assert!(
            outcome.stderr.contains("isolated") && outcome.stderr.contains("oci"),
            "the refusal must name the claiming plugin, got: {}",
            outcome.stderr
        );
    }

    /// ...and isolation is per session, so a session nobody claimed still
    /// runs its validation. The gate must not become a wall the moment any
    /// sandbox is live.
    #[tokio::test]
    async fn an_unclaimed_session_still_runs_its_validation_while_another_is_sandboxed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (marker, command) = payload(&tmp);
        let isolation = isolation_claiming("s-sandboxed");

        let outcome = run_validation_command(
            &engine(permissive()),
            Some(&isolation),
            "s-free",
            "ordinary validation",
            &command,
        )
        .await;

        assert_eq!(outcome.exit_code, 0, "stderr: {}", outcome.stderr);
        assert!(marker.exists(), "an unclaimed session must still validate");
    }

    /// An `RpcContext` whose global `[permissions]` are `config`, plus the id
    /// of a session registered in it.
    fn assessment_context(config: PermissionConfig) -> (RpcContext, String) {
        assessment_context_with_profile(config, None)
    }

    /// As [`assessment_context`], with `profile` bound as an `[acp.agents.*]`
    /// entry that the session's agent names — the shape an operator uses to
    /// give one session stricter rules than the daemon.
    fn assessment_context_with_profile(
        config: PermissionConfig,
        profile: Option<PermissionConfig>,
    ) -> (RpcContext, String) {
        use crate::agent_manager::{AgentManager, AgentManagerParams};
        use crate::background_manager::BackgroundJobManager;
        use crate::kiln_manager::KilnManager;
        use crate::project_manager::ProjectManager;
        use std::sync::Arc;

        let (event_tx, _rx) = tokio::sync::broadcast::channel(16);
        let kiln_manager = Arc::new(KilnManager::new());
        let session_manager = crate::test_support::temp_session_manager();
        let acp_config = profile.map(|permissions| {
            let mut agents = std::collections::HashMap::new();
            agents.insert(
                PROFILE_NAME.to_string(),
                crucible_core::config::components::acp::AgentProfile {
                    permissions: Some(permissions),
                    ..Default::default()
                },
            );
            crucible_core::config::components::acp::AcpConfig {
                agents,
                ..Default::default()
            }
        });
        let names_profile = acp_config.is_some();
        let agents = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: kiln_manager.clone(),
            session_manager: session_manager.clone(),
            background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
            mcp_gateway: None,
            llm_config: None,
            acp_config,
            context_config: None,
            permission_config: Some(config),
            plugin_loader: None,
        }));

        let mut session = crucible_core::session::Session::new(
            crucible_core::session::SessionType::Chat,
            Vec::new(),
        );
        if names_profile {
            session.agent = Some(crucible_core::session::SessionAgent::from_profile(
                &crucible_core::config::components::acp::AgentProfile::default(),
                PROFILE_NAME,
            ));
        }
        let session_id = session.id.to_string();
        session_manager.register_transient(session);

        let data_home = tempfile::tempdir().expect("tempdir").keep();
        let ctx = RpcContext::for_test(
            kiln_manager,
            session_manager,
            agents,
            Arc::new(ProjectManager::new(data_home.join("projects.json"))),
            event_tx,
            None,
            data_home,
        );
        (ctx, session_id)
    }

    /// The gate has to be handed the session the assessment is *for*.
    ///
    /// `run_validation_command` can be correct and the feature still broken:
    /// `run_and_emit_assessment` is where the session id and the isolation
    /// registry are actually bound, and a handler that binds neither runs the
    /// note's text on the host anyway. Driven through a real `RpcContext`, so
    /// what is under test is the wiring rather than the helper.
    ///
    /// Global permissions are wide open here, so only the isolation gate can
    /// be what refuses.
    #[tokio::test]
    async fn the_assessment_path_hands_the_gate_its_own_session() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (marker, command) = payload(&tmp);

        let (ctx, session_id) = assessment_context(permissive());
        ctx.agents.set_isolation(isolation_claiming(&session_id));
        let mut events = ctx.event_tx.subscribe();

        run_and_emit_assessment(
            &ctx,
            &session_id,
            &[crucible_core::parser::types::ValidationEntry {
                description: "prove the container is escapable".to_string(),
                command: Some(command),
                offset: 0,
            }],
        )
        .await;

        assert!(
            !marker.exists(),
            "the assessment path ran a command on the host inside an isolated session"
        );

        let msg = events
            .try_recv()
            .expect("workflow.assessed must be emitted");
        assert_eq!(msg.event, "workflow.assessed");
        let failed = msg.data["runnable_failed"]
            .as_array()
            .expect("runnable_failed array");
        assert_eq!(failed.len(), 1, "the refusal must be reported, not dropped");
        assert_eq!(
            failed[0]["exit_code"].as_i64(),
            Some(VALIDATION_REFUSED_EXIT_CODE as i64)
        );
        assert!(
            failed[0]["stderr"]
                .as_str()
                .is_some_and(|e| e.contains("isolated")),
            "got: {}",
            failed[0]["stderr"]
        );
    }

    /// The gate reads the *session's* rules, not just the daemon's.
    ///
    /// An operator who gave one session a stricter agent profile expects it to
    /// hold everywhere; reading `permission_config()` handed the assessment the
    /// permissive global rules instead, so the session the operator locked down
    /// was the one that ran the note's text.
    #[tokio::test]
    async fn the_assessment_path_honours_a_session_stricter_than_the_daemon() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (marker, command) = payload(&tmp);

        let (ctx, session_id) = assessment_context_with_profile(
            // Global: this command is explicitly allowed.
            permissive(),
            // The session's own profile: nothing is.
            Some(PermissionConfig {
                default: PermissionMode::Deny,
                ..Default::default()
            }),
        );
        let mut events = ctx.event_tx.subscribe();

        run_and_emit_assessment(
            &ctx,
            &session_id,
            &[crucible_core::parser::types::ValidationEntry {
                description: "denied by the session's own profile".to_string(),
                command: Some(command),
                offset: 0,
            }],
        )
        .await;

        assert!(
            !marker.exists(),
            "the session's own profile denied this and it ran anyway"
        );
        let msg = events
            .try_recv()
            .expect("workflow.assessed must be emitted");
        assert_eq!(
            msg.data["runnable_failed"]
                .as_array()
                .map(|f| f.len())
                .unwrap_or(0),
            1,
            "the refusal must be reported: {}",
            msg.data
        );
    }

    /// A `workflow.assessed` consumer has to be able to tell "refused by
    /// policy" from "the shell would not start". Both used `-1`, which is also
    /// what the timeout branch and a signal-killed shell report, so the three
    /// were indistinguishable.
    #[tokio::test]
    async fn a_refusal_is_distinguishable_from_a_shell_that_would_not_start() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (_marker, command) = payload(&tmp);

        let outcome = run_unsandboxed(
            &engine(PermissionConfig::default()),
            "refused by policy",
            &command,
        )
        .await;

        assert_eq!(outcome.exit_code, VALIDATION_REFUSED_EXIT_CODE);
        assert_ne!(
            outcome.exit_code, -1,
            "the spawn-error and timeout branches both report -1, so a refusal \
             must not"
        );
    }
}
