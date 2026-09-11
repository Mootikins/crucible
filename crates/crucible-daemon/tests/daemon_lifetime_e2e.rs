//! Two ways a daemon must be able to stop existing.
//!
//! # The fixture must not leave one behind when readiness never arrives
//!
//! `TestDaemon` spawns a real `cru daemon serve`. A `std::process::Child` has
//! no killing `Drop`, so the only thing that reaps that process is
//! `TestDaemon`'s own `Drop` — and that runs only if a `TestDaemon` exists.
//! When the readiness wait ran BEFORE the struct was built, a timeout returned
//! through `?` and dropped the `Child` bare: the daemon kept running with
//! nobody holding it, for ever. About 90 of the 91 stray daemons found on one
//! developer's box came from this one line ordering.
//!
//! # A signalled daemon must shut down, not be killed
//!
//! The daemon had no signal handler. SIGTERM's default disposition terminates
//! the process outright, so a `kill`, a container stop or a logout took the
//! daemon down mid-write: the persist task never drained and the socket file
//! stayed behind.

mod common;

use common::{DaemonNotReady, TestDaemon};
use std::os::unix::process::ExitStatusExt;
use std::time::Duration;

/// Ask the operating system whether `pid` still names a live process.
///
/// Signal 0 performs the permission and existence checks without delivering
/// anything, which is the standard way to ask. Reading source text or trusting
/// the fixture's own bookkeeping would not prove the process is gone.
fn process_is_alive(pid: u32) -> bool {
    // SAFETY: `kill` with signal 0 sends nothing. The only effects are the
    // return value and `errno`.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[tokio::test]
async fn a_readiness_timeout_leaves_no_daemon_behind() {
    // No daemon binds a socket in zero time, so this reaches the failure path
    // every run rather than on a loaded box only.
    let outcome = TestDaemon::start_with_ready_timeout(Duration::ZERO).await;

    let err = outcome
        .err()
        .expect("a zero-length readiness window cannot succeed");
    let not_ready = err
        .downcast::<DaemonNotReady>()
        .expect("the readiness wait reports which process it gave up on");

    assert!(
        !process_is_alive(not_ready.pid),
        "daemon pid {} survived the failed start; the fixture orphaned it",
        not_ready.pid
    );
}

/// A daemon that handles SIGTERM returns from `run()` and exits with a code.
/// One that does not is terminated BY the signal and has no exit code at all,
/// which is exactly what this asserts against.
#[tokio::test]
async fn sigterm_shuts_the_daemon_down_cleanly() {
    let mut daemon = TestDaemon::start().await.expect("start the daemon");

    let status = daemon
        .signal_and_wait(libc::SIGTERM, Duration::from_secs(30))
        .expect("the daemon must exit on SIGTERM rather than ignore it");

    assert_eq!(
        status.code(),
        Some(0),
        "SIGTERM must run the shutdown path; the daemon was terminated by signal {:?} instead",
        status.signal()
    );
}
