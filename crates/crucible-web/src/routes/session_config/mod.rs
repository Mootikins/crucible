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

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::services::daemon::AppState;

pub(super) mod basic;
pub(super) mod prompt;

#[cfg(test)]
mod tests;

// The `__path_*` types come with the handlers: `utoipa_axum::routes!` reads
// each handler's `#[utoipa::path]` attribute through the type the macro
// generates beside it, and resolves both names in this module's scope.
pub(super) use basic::{
    __path_get_precognition, __path_list_agent_options, __path_set_agent_option,
    __path_set_precognition, get_precognition, list_agent_options, set_agent_option,
    set_precognition,
};

pub(super) use prompt::{
    __path_get_context_strategy, __path_set_context_strategy, get_context_strategy,
    set_context_strategy,
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
pub(super) fn config_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(set_precognition, get_precognition))
        // Not one of Crucible's knobs: the settings the external agent
        // advertised for itself. One path serves both directions because the
        // value belongs to the agent — GET lists what it has, POST sets one,
        // and the agent's own list is the only report of what the value
        // became. It sits with the config routes because that is where the
        // settings panel's calls belong, and because a route outside this
        // group would stop inheriting the auth and limits above.
        .routes(routes!(list_agent_options, set_agent_option))
        // The nine knobs the daemon advertised that the web could not reach.
        // Gate A2e keeps the axis from drifting again; these close it.
        .routes(routes!(set_context_strategy, get_context_strategy))
}
