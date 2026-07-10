//! Mock daemon infrastructure shared across route contract tests.
//!
//! The canonical mock daemon lives in `crucible_cli::web::test_support`
//! (exposed via the `test-utils` self dev-dependency) — this module only
//! re-exports it plus a wider router. A second hand-maintained copy used to
//! live here and drifted from the library copy; don't recreate it.

pub(super) use crucible_cli::web::test_support::{
    build_mock_state, start_mock_daemon, start_mock_daemon_with_errors, MockErrors,
};

use axum::Router;
use crucible_cli::web::routes::{
    chat_routes, health_routes, plugin_routes, project_routes, search_routes, session_routes,
    skills_routes,
};
use crucible_cli::web::services::daemon::AppState;

/// Build the full app router with mock state. Wider than the library's
/// `test_support::build_test_app`: contract tests also cover the skills and
/// plugin route groups.
pub(super) fn build_test_app(state: AppState) -> Router {
    Router::new()
        .merge(chat_routes())
        .merge(session_routes())
        .merge(project_routes())
        .merge(search_routes())
        .merge(skills_routes())
        .merge(plugin_routes())
        .with_state(state)
        .merge(health_routes())
}
