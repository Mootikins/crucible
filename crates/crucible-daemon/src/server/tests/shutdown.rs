//! Session writes across a shutdown.
//!
//! The shutdown has a deadline (`SHUTDOWN_DEADLINE`), so it must not lose the
//! session events that were in the queue when it started, and it must not cut
//! a write that already started. `tests/daemon_lifetime_e2e.rs` shows the
//! other side: a write that never finishes does not hold the exit.

use super::*;
use crate::event_emitter::emit_event;

/// Stop the server, but keep `server` alive: [`TestServer::shutdown`] drops
/// the TempDir, and with it the files that the test reads.
async fn stop_keeping_files(server: &mut TestServer) {
    let _ = server.shutdown_tx.send(());
    let _ = (&mut server.task).await;
}

/// Every event in the persist queue at the shutdown signal reaches disk.
#[tokio::test]
async fn events_queued_at_shutdown_are_all_persisted() {
    const QUEUED: usize = 200;
    let mut server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, TestServer::KILN, 1).await;

    for i in 0..QUEUED {
        let _ = emit_event(
            &server.event_tx,
            SessionEventMessage::model_switched(&session_id, format!("model-{i}"), "mock"),
        );
    }
    stop_keeping_files(&mut server).await;

    let storage = FileSessionStorage::new(server.sessions_root());
    let persisted = storage
        .load_events(
            &crucible_core::session::SessionId::parse(&session_id).expect("a valid id"),
            None,
            None,
        )
        .await
        .expect("load the session's events");
    let switches = persisted
        .iter()
        .filter(|e| e["event"] == "model_switched")
        .count();
    assert_eq!(
        switches, QUEUED,
        "every queued event must be persisted before the daemon exits"
    );
}

/// A write that started before the deadline finishes after it.
///
/// The session log is a FIFO, so the persist task's open of it blocks until a
/// reader comes. The reader comes after `SHUTDOWN_DEADLINE` and before the
/// end of `STARTED_WRITE_GRACE`, so the line is only there if the shutdown
/// waited for the write that was under way.
#[tokio::test]
async fn a_write_under_way_at_the_deadline_is_finished() {
    let mut server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, TestServer::KILN, 1).await;
    // The startup title catch-up reads the log of each untitled session, and
    // a late catch-up would open the FIFO for reading before the test does.
    let titled = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "session.set_title",
            "params": { "session_id": session_id, "title": "late write" }
        }),
    )
    .await;
    assert!(titled["error"].is_null(), "set_title failed: {titled}");

    let log = server
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

    let _ = emit_event(
        &server.event_tx,
        SessionEventMessage::model_switched(&session_id, "late-model", "mock"),
    );

    // A channel with a timeout, not a join: if no write comes, the open blocks
    // for ever, and the test must fail rather than hang.
    let (read_tx, read_rx) = std::sync::mpsc::channel();
    std::thread::spawn({
        let log = log.clone();
        move || {
            std::thread::sleep(SHUTDOWN_DEADLINE + STARTED_WRITE_GRACE / 2);
            let _ = read_tx.send(std::fs::read_to_string(&log));
        }
    });
    stop_keeping_files(&mut server).await;

    let written = read_rx
        .recv_timeout(SHUTDOWN_DEADLINE + STARTED_WRITE_GRACE * 5)
        .expect("the reader got no writer; the write was never under way")
        .expect("read the FIFO");
    assert!(
        written.contains("late-model"),
        "the write under way at the deadline must finish; the reader got {written:?}"
    );
}
