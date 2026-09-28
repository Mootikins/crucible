//! The test daemons of the route contract tests.
//!
//! The mock daemon and the real in-process daemon live in
//! `crucible_web::test_support`, which the `test-utils` self dev-dependency
//! exposes. This module only re-exports them and the test router. A copy of
//! the mock lived here once and drifted from the library copy. Do not make a
//! copy again.

pub(super) use crucible_web::test_support::{
    build_state, start_mock_daemon, start_mock_daemon_with_errors, start_real_daemon_with_kilns,
    MockErrors,
};

pub(super) use crucible_web::test_support::build_test_app;
