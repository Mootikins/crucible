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
//!
//! # A signalled daemon must shut down in a few seconds
//!
//! Exit also waited for work that did not see the signal. The tokio runtime,
//! when it drops, waits for every thread that still runs a blocking call. A
//! plugin service in a synchronous `io.popen` read, or a session write that
//! never completes, held the daemon for as long as that call took, with no
//! limit. One run under a load average of 32 took more than 30 s.

mod common;

use common::{DaemonNotReady, RpcConn, TestDaemon};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::time::Duration;

/// The time a daemon may take to exit after SIGTERM.
///
/// The daemon's own budget is 4 s at most: 2 s for its tasks, 1 s for a
/// session write that already started, and 1 s for blocking work. An idle
/// daemon exits in 0.04 to 0.16 s, also with 64 busy loops on 32 cores. The
/// bound is above the worst case of the budget, so only a daemon that does not
/// keep to its budget fails.
const EXIT_BOUND: Duration = Duration::from_secs(5);

/// Send SIGTERM and require an exit with code 0 inside [`EXIT_BOUND`].
fn assert_sigterm_exits_in_time(daemon: &mut TestDaemon) {
    let started = std::time::Instant::now();
    let status = daemon
        .signal_and_wait(libc::SIGTERM, EXIT_BOUND)
        .unwrap_or_else(|| panic!("the daemon was still running {EXIT_BOUND:?} after SIGTERM"));
    assert_eq!(
        status.code(),
        Some(0),
        "SIGTERM must run the shutdown path; the daemon was terminated by signal {:?} instead",
        status.signal()
    );
    eprintln!("the daemon exited {:?} after SIGTERM", started.elapsed());
}

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

    assert_sigterm_exits_in_time(&mut daemon);
}

/// Wait until `path` exists. The caller's own test timeout is the limit.
async fn wait_for_file(path: &Path) {
    while !path.exists() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A plugin service that blocks a runtime worker must not hold the exit.
///
/// The service reads a child process to its end in one synchronous call, so
/// its worker thread does not return to the runtime until the child ends. The
/// child writes a line each second and never ends by itself. After the daemon
/// exits, its next write fails with SIGPIPE, so the test leaves no process.
#[tokio::test]
async fn sigterm_does_not_wait_for_a_plugin_service_in_a_blocking_call() {
    let mut daemon = TestDaemon::start_with_home_setup(|home| {
        let marker = home.join("service-is-blocked");
        let config_dir = home.join(".config").join("crucible");
        let plugin_dir = config_dir.join("plugins").join("stall");
        std::fs::create_dir_all(&plugin_dir)?;
        std::fs::write(
            plugin_dir.join("init.lua"),
            format!(
                r#"
return {{
    name = "stall",
    services = {{
        stall = {{
            desc = "reads a child process that never ends",
            fn = function()
                local child = io.popen("touch {marker}; while :; do echo x; sleep 1; done")
                child:read("a")
            end,
        }},
    }},
}}
"#,
                marker = marker.display()
            ),
        )?;
        let kiln = home.join(TestDaemon::KILN);
        std::fs::create_dir_all(&kiln)?;
        std::fs::write(
            config_dir.join("init.lua"),
            format!(
                "cru.config.set({{ kilns = {{ [{:?}] = {:?} }} }})\ncru.plugin.setup({{ {{ \"stall\" }} }})\n",
                TestDaemon::KILN,
                kiln.display().to_string()
            ),
        )?;
        Ok(())
    })
    .await
    .expect("start the daemon with the stalling plugin");

    wait_for_file(&daemon.home().join("service-is-blocked")).await;

    assert_sigterm_exits_in_time(&mut daemon);
}

/// A session write that never completes must not hold the exit.
///
/// The session's event log is a FIFO with no reader, so the daemon's open of
/// it blocks for ever: the persist task is stuck inside a write it started.
/// The daemon gives that write its grace time, logs it, and exits.
#[tokio::test]
async fn sigterm_does_not_wait_for_a_session_write_that_never_completes() {
    let mut daemon = TestDaemon::start().await.expect("start the daemon");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect");

    let created = conn
        .call_method(
            "session.create",
            serde_json::json!({ "session_type": "chat", "kilns": [TestDaemon::KILN] }),
            1,
        )
        .await;
    let session_id = created["result"]["session_id"]
        .as_str()
        .unwrap_or_else(|| panic!("session.create failed: {created}"))
        .to_string();

    // The startup title catch-up reads the log of each untitled session. A
    // late catch-up on a loaded box opened the FIFO for reading, and the
    // write then completed. A titled session is not read.
    let titled = conn
        .call_method(
            "session.set_title",
            serde_json::json!({ "session_id": session_id, "title": "stuck write" }),
            2,
        )
        .await;
    assert!(titled["error"].is_null(), "set_title failed: {titled}");

    let log = daemon
        .sessions_root()
        .join(&session_id)
        .join("session.jsonl");
    let c_path = std::ffi::CString::new(log.as_os_str().as_encoded_bytes()).expect("path");
    // SAFETY: `mkfifo` reads a NUL-terminated path that outlives the call.
    assert_eq!(
        unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) },
        0,
        "mkfifo {log:?}"
    );

    // `model_switched` is a persisted event, so the persist task opens the
    // FIFO and blocks. The switch needs an agent to switch.
    let configured = conn
        .call_method(
            "session.configure_agent",
            serde_json::json!({
                "session_id": session_id,
                "agent": {
                    "agent_type": "internal",
                    "provider": "mock",
                    "model": "mock",
                    "system_prompt": "test",
                    "provider_key": "mock"
                }
            }),
            3,
        )
        .await;
    assert!(
        configured["error"].is_null(),
        "configure_agent failed: {configured}"
    );
    let switched = conn
        .call_method(
            "session.switch_model",
            serde_json::json!({ "session_id": session_id, "model_id": "other" }),
            4,
        )
        .await;
    assert!(
        switched["error"].is_null(),
        "switch_model failed: {switched}"
    );

    assert_sigterm_exits_in_time(&mut daemon);
}
