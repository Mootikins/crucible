//! Web routes for the daemon's `session.{set,get}_*` config knobs.
//!
//! One module per concern rather than one file: nine knob pairs plus their
//! request/response structs is roughly +450 lines, and `session_config.rs` was
//! already 206 — one file would have breached the 1000-line module budget (gate
//! A4). Split by what the knobs mean, not by line count.
//!
//! Every knob the daemon advertises in `METHODS` must be reachable from here;
//! gate **A2e** (`crucible-cli/tests/architecture_tests.rs`) fails when one is
//! not. Nine of fifteen were missing and nothing noticed, because a knob absent
//! from the axum Router is not a compile error anywhere.
//!
//! **The wire field name is the contract, and it is not always the knob name.**
//! `session.set_execution_timeout` carries `timeout_secs`; a request struct
//! named after the knob would compile, pass review, and silently drop the value.
//! `tests.rs` round-trips each knob through a mock daemon and asserts the
//! response JSON key, because route existence alone does not prove the value
//! survives.

// Only `put` as a free function: the `.get(...)` in each pair is a method on the
// `MethodRouter` `put(...)` returns.
use axum::{routing::put, Router};

use crate::services::daemon::AppState;

pub(super) mod basic;
pub(super) mod context;
pub(super) mod execution;
pub(super) mod prompt;

#[cfg(test)]
mod tests;

pub(super) use basic::{
    get_max_tokens, get_precognition, get_precognition_results, get_temperature,
    get_thinking_budget, set_max_tokens, set_precognition, set_precognition_results,
    set_temperature, set_thinking_budget,
};
pub(super) use context::{
    get_autocompact_threshold, get_context_budget, get_context_window, set_autocompact_threshold,
    set_context_budget, set_context_window,
};
pub(super) use execution::{
    get_execution_timeout, get_max_iterations, get_validation_retries, set_execution_timeout,
    set_max_iterations, set_validation_retries,
};
pub(super) use prompt::{
    get_context_strategy, get_output_validation, get_system_prompt, set_context_strategy,
    set_output_validation, set_system_prompt,
};

/// Every `/api/session/{id}/config/...` route, as a standalone router the session
/// group merges in.
///
/// Registered here rather than spelled out in `routes/session/mod.rs`: fifteen
/// route pairs is 60 lines of chain that pushed that file past the 1000-line
/// budget (gate A4 caught it), and it made the file import 28 handler names it
/// otherwise has no interest in. The knobs now live entirely in this directory —
/// handler, request/response shape, route, and round-trip test.
///
/// Merged into the session router rather than nested as its own group, so it
/// inherits bearer auth, the host guard, the CORS allowlist, the body limit and
/// the security headers. A separate group is how a surface quietly stops
/// inheriting them.
pub(super) fn config_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/session/{id}/config/thinking-budget",
            put(set_thinking_budget).get(get_thinking_budget),
        )
        .route(
            "/api/session/{id}/config/temperature",
            put(set_temperature).get(get_temperature),
        )
        .route(
            "/api/session/{id}/config/max-tokens",
            put(set_max_tokens).get(get_max_tokens),
        )
        .route(
            "/api/session/{id}/config/precognition",
            put(set_precognition).get(get_precognition),
        )
        .route(
            "/api/session/{id}/config/precognition/results",
            put(set_precognition_results).get(get_precognition_results),
        )
        // The nine knobs the daemon advertised that the web could not reach.
        // Gate A2e keeps the axis from drifting again; these close it.
        .route(
            "/api/session/{id}/config/context-budget",
            put(set_context_budget).get(get_context_budget),
        )
        .route(
            "/api/session/{id}/config/context-window",
            put(set_context_window).get(get_context_window),
        )
        .route(
            "/api/session/{id}/config/autocompact-threshold",
            put(set_autocompact_threshold).get(get_autocompact_threshold),
        )
        .route(
            "/api/session/{id}/config/max-iterations",
            put(set_max_iterations).get(get_max_iterations),
        )
        .route(
            "/api/session/{id}/config/execution-timeout",
            put(set_execution_timeout).get(get_execution_timeout),
        )
        .route(
            "/api/session/{id}/config/validation-retries",
            put(set_validation_retries).get(get_validation_retries),
        )
        .route(
            "/api/session/{id}/config/context-strategy",
            put(set_context_strategy).get(get_context_strategy),
        )
        .route(
            "/api/session/{id}/config/output-validation",
            put(set_output_validation).get(get_output_validation),
        )
        .route(
            "/api/session/{id}/config/system-prompt",
            put(set_system_prompt).get(get_system_prompt),
        )
}
