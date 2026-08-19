//! What every session-taking method answers when the session does not exist.
//!
//! S3 of the despaghettification plan proposed to resolve the session once, at
//! the routing step, and hand each handler a resolved value. This table is the
//! measurement that says a single resolver cannot ship without changing the
//! wire contract: **the routed methods give eight different answers to "that
//! session is not here", and six of them answer with success.**
//!
//! The eight, as recorded below:
//!
//! 1. `INVALID_PARAMS` + `Session not found: {id}` — 31 methods.
//! 2. `INVALID_PARAMS` + `Operation '{op}' not allowed in current state` — the
//!    nine lifecycle methods. The message never names the session: an unknown
//!    id and a paused session that cannot pause again are one answer.
//! 3. `INVALID_PARAMS` + `session {id} has no review ledger` — the review
//!    family, which owns a ledger keyed by session and not the session.
//! 4. `INVALID_PARAMS` + a *wrapped* `Session not found` — `session.set_title`
//!    and `session.generate_title` prefix it with their own failure text.
//! 5. success, reporting the work was not done — `session.cancel` →
//!    `cancelled: false`.
//! 6. success with a zero aggregate — `session.cache_stats`.
//! 7. success with an empty collection — `session.load_events`,
//!    `session.render_markdown`, `session.status`, `review.list_hunks`.
//! 8. success that never consulted the session at all —
//!    `session.test_interaction`.
//!
//! Groups 5 through 8 are the ones that matter. A resolver at the routing step
//! that refuses an unknown session turns six successes into errors; one that
//! does not refuse leaves every handler its own second check and buys nothing.
//! Preserving all eight from one place means the layer carries an
//! eight-way per-method policy table, which is the match arm it was meant to
//! delete, moved one file over.
//!
//! So this file is the contract, not a step toward one. Change an answer here
//! only on purpose, and say which client wanted it changed.

use super::{RpcContext, RpcDispatcher};
use crate::protocol::{Request, RequestId, INVALID_PARAMS};
use crate::subscription::ClientId;
use serde_json::json;
use std::sync::Arc;

/// A session id that is well-formed (so `SessionId::parse` accepts it) and
/// belongs to nothing.
const GHOST: &str = "ghost-session-0000";

/// What a method is expected to answer for [`GHOST`].
enum Answer {
    /// `INVALID_PARAMS` with exactly this message.
    Refuses(&'static str),
    /// A success whose result is exactly this JSON.
    Succeeds(serde_json::Value),
    /// A success whose result only has to satisfy this predicate — for the one
    /// method whose reply carries a fresh uuid.
    SucceedsWith(fn(&serde_json::Value) -> bool),
}

fn make_request(method: &str, params: serde_json::Value) -> Request {
    Request {
        jsonrpc: "2.0".to_string(),
        id: Some(RequestId::Number(1)),
        method: method.to_string(),
        params,
    }
}

fn test_context(data_home: &std::path::Path, kiln: &std::path::Path) -> Arc<RpcContext> {
    use crate::agent_manager::{AgentManager, AgentManagerParams};
    use crate::background_manager::BackgroundJobManager;
    use crate::kiln_manager::KilnManager;
    use crate::project_manager::ProjectManager;
    use tokio::sync::broadcast;

    let (event_tx, _) = broadcast::channel(16);
    let kiln_manager = Arc::new(KilnManager::new());
    let session_manager = crate::test_support::temp_session_manager_with_kilns(&[("kiln", kiln)]);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
    let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: kiln_manager.clone(),
        session_manager: session_manager.clone(),
        background_manager,
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
    }));

    Arc::new(RpcContext::for_test(
        kiln_manager,
        session_manager,
        agent_manager,
        Arc::new(ProjectManager::new(data_home.join("projects.json"))),
        event_tx,
        None,
        data_home.to_path_buf(),
    ))
}

/// `Session not found: ghost-session-0000` — the answer 31 methods give.
fn not_found() -> Answer {
    Answer::Refuses("Session not found: ghost-session-0000")
}

/// `Operation '{op}' not allowed in current state`.
macro_rules! bad_state {
    ($op:literal) => {
        Answer::Refuses(concat!(
            "Operation '",
            $op,
            "' not allowed in current state"
        ))
    };
}

/// `session ghost-session-0000 has no review ledger`.
fn no_ledger() -> Answer {
    Answer::Refuses("session ghost-session-0000 has no review ledger")
}

fn cases(ws: &std::path::Path) -> Vec<(&'static str, serde_json::Value, Answer)> {
    let workspace = ws.to_string_lossy().to_string();
    let out = ws.join("export.md").to_string_lossy().to_string();
    vec![
        // ── 1. refuses, naming the session ──────────────────────────────────
        ("session.get", json!({}), not_found()),
        ("session.fork", json!({}), not_found()),
        (
            "session.switch_model",
            json!({"model_id": "m"}),
            not_found(),
        ),
        ("session.list_models", json!({}), not_found()),
        ("session.list_modes", json!({}), not_found()),
        ("session.connect_kiln", json!({"kiln": "kiln"}), not_found()),
        (
            "session.disconnect_kiln",
            json!({"kiln": "kiln"}),
            not_found(),
        ),
        (
            "session.set_workspace",
            json!({"workspace": workspace}),
            not_found(),
        ),
        (
            "session.inject_context",
            json!({"role": "user", "content": "c"}),
            not_found(),
        ),
        ("session.list_notifications", json!({}), not_found()),
        (
            "session.dismiss_notification",
            json!({"notification_id": "n"}),
            not_found(),
        ),
        ("session.undo", json!({}), not_found()),
        ("session.can_undo", json!({}), not_found()),
        ("session.undo_depth", json!({}), not_found()),
        // the pinned `require_param!` setters and every generated config knob
        (
            "session.set_mode",
            json!({"mode_id": "normal"}),
            not_found(),
        ),
        ("session.get_mode", json!({}), not_found()),
        (
            "session.set_thinking_budget",
            json!({"thinking_budget": 100}),
            not_found(),
        ),
        ("session.get_thinking_budget", json!({}), not_found()),
        (
            "session.set_temperature",
            json!({"temperature": 0.5}),
            not_found(),
        ),
        ("session.get_temperature", json!({}), not_found()),
        (
            "session.set_max_tokens",
            json!({"max_tokens": 10}),
            not_found(),
        ),
        ("session.get_max_tokens", json!({}), not_found()),
        (
            "session.set_max_iterations",
            json!({"max_iterations": 3}),
            not_found(),
        ),
        (
            "session.set_execution_timeout",
            json!({"execution_timeout": 3}),
            not_found(),
        ),
        (
            "session.set_context_budget",
            json!({"context_budget": 3}),
            not_found(),
        ),
        (
            "session.set_context_window",
            json!({"context_window": 3}),
            not_found(),
        ),
        (
            "session.set_context_strategy",
            json!({"context_strategy": "truncate"}),
            not_found(),
        ),
        ("session.get_context_strategy", json!({}), not_found()),
        (
            "session.set_output_validation",
            json!({"output_validation": "off"}),
            not_found(),
        ),
        ("session.get_output_validation", json!({}), not_found()),
        (
            "session.set_validation_retries",
            json!({"validation_retries": 3}),
            not_found(),
        ),
        (
            "session.set_system_prompt",
            json!({"system_prompt": "x"}),
            not_found(),
        ),
        ("session.get_system_prompt", json!({}), not_found()),
        (
            "session.set_precognition",
            json!({"precognition": true}),
            not_found(),
        ),
        ("session.get_precognition", json!({}), not_found()),
        (
            "session.set_precognition_results",
            json!({"precognition_results": 3}),
            not_found(),
        ),
        (
            "session.set_autocompact_threshold",
            json!({"autocompact_threshold": 0.5}),
            not_found(),
        ),
        ("session.get_autocompact_threshold", json!({}), not_found()),
        // ── 2. refuses, naming the OPERATION and never the session ──────────
        // The session-manager state machine answers before anything reports a
        // missing session, so a client cannot tell "no such session" from
        // "wrong state" on any of these.
        ("session.pause", json!({}), bad_state!("pause")),
        ("session.resume", json!({}), bad_state!("resume")),
        (
            "session.resume_from_storage",
            json!({}),
            bad_state!("resume_from_storage"),
        ),
        ("session.end", json!({}), bad_state!("end")),
        ("session.archive", json!({}), bad_state!("archive")),
        ("session.unarchive", json!({}), bad_state!("unarchive")),
        ("session.delete", json!({}), bad_state!("delete")),
        ("session.compact", json!({}), bad_state!("compact")),
        // ── 3. refuses, naming the LEDGER ───────────────────────────────────
        ("review.rebase", json!({}), no_ledger()),
        (
            "review.set_state",
            json!({"hunk_id": "h", "state": "accepted"}),
            no_ledger(),
        ),
        (
            "review.comment",
            json!({"path": "p", "body": "b", "line_start": 1}),
            no_ledger(),
        ),
        (
            "review.resolve_comment",
            json!({"comment_id": "c"}),
            no_ledger(),
        ),
        // ── 4. refuses, WRAPPING the not-found text in its own ──────────────
        (
            "session.set_title",
            json!({"title": "t"}),
            Answer::Refuses("Failed to set title: Session not found: ghost-session-0000"),
        ),
        (
            "session.generate_title",
            json!({}),
            Answer::Refuses("Failed to generate title: Session not found: ghost-session-0000"),
        ),
        // ── 5-8. SUCCEEDS. A resolver that refuses would break these four. ──
        (
            "session.cancel",
            json!({}),
            Answer::Succeeds(json!({"session_id": GHOST, "cancelled": false})),
        ),
        (
            "session.cache_stats",
            json!({}),
            Answer::Succeeds(json!({
                "session_id": GHOST,
                "hits": 0, "misses": 0,
                "read_tokens": 0, "creation_tokens": 0,
                "prompt_tokens": 0, "completion_tokens": 0,
                "hit_rate": serde_json::Value::Null,
            })),
        ),
        (
            "session.load_events",
            json!({}),
            Answer::Succeeds(json!([])),
        ),
        (
            "session.render_markdown",
            json!({}),
            Answer::Succeeds(json!({"markdown": ""})),
        ),
        (
            "session.status",
            json!({}),
            Answer::Succeeds(json!({"status": []})),
        ),
        (
            "review.list_hunks",
            json!({}),
            Answer::Succeeds(json!({
                "session_id": GHOST,
                "hunks": [], "comments": [], "degraded": [],
                "integrity": {"skips": []},
                "gate": serde_json::Value::Null,
            })),
        ),
        (
            "session.export_to_file",
            json!({"output_path": out}),
            Answer::Succeeds(
                json!({"status": "ok", "output_path": ws.join("export.md").to_string_lossy()}),
            ),
        ),
        // Never looks at the session at all: it mints a request id and returns.
        (
            "session.test_interaction",
            json!({"kind": "toast"}),
            Answer::SucceedsWith(|v| {
                v["session_id"] == json!(GHOST)
                    && v["request_id"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("test-"))
            }),
        ),
    ]
}

/// Every session-taking method, asked about a session that is not there.
///
/// A failure here is a wire-contract change. Read the diff before touching the
/// expectation: some of these answers are load-bearing for a client that polls
/// (`session.status`, `review.list_hunks`) and would start erroring on every
/// tick if the answer became a refusal.
#[tokio::test]
async fn every_session_method_keeps_its_own_answer_for_a_missing_session() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ws = tmp.path().join("ws");
    let kiln = tmp.path().join("kiln");
    std::fs::create_dir_all(&ws).expect("ws");
    std::fs::create_dir_all(&kiln).expect("kiln");
    let dispatcher = RpcDispatcher::new(test_context(tmp.path(), &kiln));

    let mut failures = Vec::new();
    for (method, extra, expected) in cases(&ws) {
        let mut params = extra;
        params["session_id"] = json!(GHOST);
        let resp = dispatcher
            .dispatch(ClientId::new(), make_request(method, params))
            .await;

        match expected {
            Answer::Refuses(msg) => match resp.error {
                None => failures.push(format!(
                    "{method}: expected refusal {msg:?}, got success {:?}",
                    resp.result
                )),
                Some(e) => {
                    if e.code != INVALID_PARAMS || e.message != msg {
                        failures.push(format!(
                            "{method}: expected ({INVALID_PARAMS}, {msg:?}), got ({}, {:?})",
                            e.code, e.message
                        ));
                    }
                }
            },
            Answer::Succeeds(want) => match resp.error {
                Some(e) => failures.push(format!(
                    "{method}: expected success {want}, got error ({}, {:?})",
                    e.code, e.message
                )),
                None => {
                    let got = resp.result.unwrap_or(serde_json::Value::Null);
                    if got != want {
                        failures.push(format!("{method}: expected {want}, got {got}"));
                    }
                }
            },
            Answer::SucceedsWith(pred) => match resp.error {
                Some(e) => failures.push(format!(
                    "{method}: expected success, got error ({}, {:?})",
                    e.code, e.message
                )),
                None => {
                    let got = resp.result.unwrap_or(serde_json::Value::Null);
                    if !pred(&got) {
                        failures.push(format!("{method}: success shape rejected: {got}"));
                    }
                }
            },
        }
    }

    assert!(
        failures.is_empty(),
        "the missing-session answer changed for {} method(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// The four handlers that obtain a `Session` value do not agree on what an
/// absent one means, which is the second half of why S3 cannot be one layer.
///
/// `session.get` and `session.fork` refuse. `session.set_workspace` reads the
/// session only to learn its kilns and treats an absent one as "no kilns", so
/// the refusal it eventually gives comes from `AgentManager`, not from the
/// read. `session.connect_kiln` reads it only to learn the provider's trust
/// level and falls back to the most restrictive one. Routing all four through
/// a resolver that refuses would move two decisions that are currently
/// fail-open-then-refuse and fail-closed-and-continue.
#[tokio::test]
async fn the_handlers_that_resolve_a_session_disagree_about_an_absent_one() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let kiln = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln");
    let dispatcher = RpcDispatcher::new(test_context(tmp.path(), &kiln));

    // `review.list_hunks` resolves the session inside `ensure_loaded` and is
    // documented to be silent when it is absent — the empty queue below is
    // that silence, observable.
    let resp = dispatcher
        .dispatch(
            ClientId::new(),
            make_request("review.list_hunks", json!({"session_id": GHOST})),
        )
        .await;
    assert!(
        resp.error.is_none(),
        "review.list_hunks must answer an absent session with an empty queue, not an error: {:?}",
        resp.error
    );
    assert_eq!(
        resp.result.expect("a result")["hunks"],
        json!([]),
        "an absent session has no hunks"
    );

    // `session.get` refuses the same absent session in the same breath.
    let resp = dispatcher
        .dispatch(
            ClientId::new(),
            make_request("session.get", json!({"session_id": GHOST})),
        )
        .await;
    let err = resp
        .error
        .expect("session.get must refuse an absent session");
    assert_eq!(err.code, INVALID_PARAMS);
    assert_eq!(err.message, format!("Session not found: {GHOST}"));
}
