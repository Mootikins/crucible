//! Regression guard: `cru session replay <id>` must still work after the
//! TUI-only replay refactor. The `session.replay` daemon RPC and its client
//! method are kept — migrating that subcommand to the local driver is a
//! separate follow-up (see plan scope exclusions).
//!
//! This test is cheap: it is a compile-time assertion that
//! `DaemonClient::session_replay` exists with the expected signature. If the
//! method is accidentally removed or renamed as part of the replay refactor,
//! this test file fails to compile.
//!
//! (An integration test that actually talks to a running daemon lives in
//! `crates/crucible-daemon/tests/rpc_integration.rs` — see
//! `test_session_replay_rpc_invalid_path` and `crates/crucible-daemon/tests/replay_e2e.rs`.)

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use crucible_daemon::rpc_client::DaemonClient;

type SessionReplayFuture<'a> =
    Pin<Box<dyn Future<Output = anyhow::Result<serde_json::Value>> + Send + 'a>>;
type SessionReplayFn = for<'a> fn(&'a DaemonClient, &'a Path, f64) -> SessionReplayFuture<'a>;

/// Confirm the client-side method is still reachable with its documented shape.
///
/// We coerce it to a function pointer of the expected signature. If the method
/// is removed, renamed, or its signature changes, this coercion fails at
/// compile time and the whole test crate refuses to build — which is exactly
/// the regression signal we want.
#[allow(dead_code)]
fn _compile_time_session_replay_exists() {
    let _ptr: SessionReplayFn = |client, path, speed| Box::pin(client.session_replay(path, speed));
}

#[test]
fn session_replay_client_method_still_compiles() {
    // The real work is the compile-time coercion above. A body-less test here
    // would risk being optimized out; this keeps the function referenced and
    // also surfaces a runnable test in `cargo test --list`.
    _compile_time_session_replay_exists();
}
