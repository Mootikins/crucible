//! Mock daemon infrastructure shared across route contract tests.
//!
//! The canonical mock daemon lives in `crucible_web::test_support`
//! (exposed via the `test-utils` self dev-dependency) — this module only
//! re-exports it and the production-composed test router. A hand-maintained copy used to
//! live here and drifted from the library copy; don't recreate it.

pub(super) use crucible_web::test_support::{
    build_mock_state, start_mock_daemon, start_mock_daemon_with_errors,
    start_mock_daemon_with_kilns, MockErrors,
};

pub(super) use crucible_web::test_support::build_test_app;
