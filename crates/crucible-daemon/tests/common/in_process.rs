//! Re-export of the shared in-process test daemon.
//!
//! The harness itself lives in [`crucible_daemon::test_support`], behind the
//! crate's `test-utils` feature, so `crucible-cli` can build the same daemon
//! for its own integration tests without depending on this crate's `tests/`
//! tree (a crate cannot reach another crate's `tests/common`). Every daemon
//! integration test in this crate uses it through this re-export instead of
//! carrying its own copy of the setup.
pub use crucible_daemon::test_support::{InProcessDaemon, InProcessDaemonBuilder};
