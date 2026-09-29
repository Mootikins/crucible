//! Web routes for daemon config that is NOT a [`crucible_core::types::SessionKnob`]:
//! per-plugin approval and the settings an external ACP agent advertises for
//! itself.
//!
//! Every [`crucible_core::types::SessionKnob`] now rides `set_knob`/`get_knob`
//! in `routes/session/mod.rs`, one route pair for all five. What stays here
//! takes a second key (`plugin`) or has no fixed value space (`agent_option`),
//! so `KnobValue` cannot express it.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::services::daemon::AppState;

pub(super) mod approval;
pub(super) mod basic;

#[cfg(test)]
mod tests;

// The `__path_*` types come with the handlers: `utoipa_axum::routes!` reads
// each handler's `#[utoipa::path]` attribute through the type the macro
// generates beside it, and resolves both names in this module's scope.
pub(super) use approval::{
    __path_get_plugin_approval, __path_list_plugin_approvals, __path_set_plugin_approval,
    get_plugin_approval, list_plugin_approvals, set_plugin_approval,
};
pub(super) use basic::{
    __path_list_agent_options, __path_set_agent_option, list_agent_options, set_agent_option,
};

/// Every `/api/session/{id}/config/...` route, as a standalone router the session
/// group merges in.
///
/// Registered here rather than spelled out in `routes/session/mod.rs`: the
/// group belongs with the knobs it serves, and spelling it out there would
/// make that file import 28 handler names it otherwise has no interest in. The
/// knobs now live entirely in this directory —
/// handler, request/response shape, route, and round-trip test.
///
/// Merged into the session router rather than nested as its own group, so it
/// inherits bearer auth, the host guard, the CORS allowlist, the body limit and
/// the security headers. A separate group is how a surface quietly stops
/// inheriting them.
pub(super) fn config_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(set_plugin_approval, get_plugin_approval))
        .routes(routes!(list_plugin_approvals))
        // Not one of Crucible's knobs: the settings the external agent
        // advertised for itself. One path serves both directions because the
        // value belongs to the agent — GET lists what it has, POST sets one,
        // and the agent's own list is the only report of what the value
        // became. It sits with the config routes because that is where the
        // settings panel's calls belong, and because a route outside this
        // group would stop inheriting the auth and limits above.
        .routes(routes!(list_agent_options, set_agent_option))
    // Every session knob — model, mode, context strategy, precognition,
    // plugin turn limit — rides `set_knob`/`get_knob` in
    // `routes/session/mod.rs` now, not a route per knob here.
}
