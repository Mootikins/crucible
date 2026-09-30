//! `session.resume`'s stored fallback, through the live RPC method — the
//! path the TUI and a Lua script use, not a web route.
//!
//! The web's `resume_session` used to catch ANY failure of `session.resume`
//! and retry with `session.resume_from_storage`, so a session held in memory
//! as `Ended` came back to life only through the web. A raw RPC caller who
//! called `session.resume` directly got the bare `InvalidState` refusal and
//! no second try. `session.resume` already fell back to storage for a
//! session not held in memory at all (`NotFound`); it now does the same for
//! one held as `Ended`, and says so in its reply (`resumed_from_storage`),
//! so a caller that needs the full transcript after a stored resume knows to
//! ask for it.

mod common;

use common::InProcessDaemonBuilder;
use crucible_core::protocol::requests::SessionCreateRequest;
use crucible_daemon::DaemonClient;
use serde_json::Value;

async fn ended_session(client: &DaemonClient) -> String {
    let session = client
        .session_create(SessionCreateRequest {
            session_type: "chat".to_string(),
            ..Default::default()
        })
        .await
        .expect("session_create failed");
    let id = session.id.to_string();
    client.session_end(&id).await.expect("session_end failed");
    id
}

/// An `Ended` session, still resident in the daemon's memory, resumes
/// through `session.resume` alone — no second RPC method needed — and the
/// reply says the resume went through storage.
#[tokio::test]
async fn session_resume_revives_a_session_ended_in_memory() {
    let server = InProcessDaemonBuilder::new()
        .expect("a test daemon builder")
        .start()
        .await
        .expect("failed to start server");
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("failed to connect");

    let id = ended_session(&client).await;

    let reply: Value = client
        .session_resume(&id)
        .await
        .expect("session.resume should revive an ended session, not refuse it");

    assert_eq!(reply["state"], "active", "{reply}");
    assert_eq!(
        reply["resumed_from_storage"], true,
        "an ended-in-memory session must be flagged as a stored resume: {reply}"
    );
}

/// A session this daemon never held (an earlier process recorded it) still
/// resumes from storage, unaffected by the widened check: the reply flags it
/// the same way.
#[tokio::test]
async fn session_resume_still_revives_a_session_absent_from_memory() {
    // Held by this test, not by either `InProcessDaemon`: a fresh-temp-dir
    // builder would drop and erase the data home the moment the first daemon
    // shuts down, before the second one ever opened it.
    let data_home = tempfile::tempdir().expect("a shared data home");

    let server = InProcessDaemonBuilder::at_data_home(data_home.path().to_path_buf())
        .start()
        .await
        .expect("failed to start server");
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("failed to connect");
    let id = ended_session(&client).await;
    drop(client);
    server.shutdown().await;

    // A second daemon, same data root: it never held the session in memory.
    let server2 = InProcessDaemonBuilder::at_data_home(data_home.path().to_path_buf())
        .start()
        .await
        .expect("failed to start the second server");
    let client2 = DaemonClient::connect_to(server2.socket_path())
        .await
        .expect("failed to connect to the second server");

    let reply: Value = client2
        .session_resume(&id)
        .await
        .expect("session.resume should load the session from storage");

    assert_eq!(reply["state"], "active", "{reply}");
    assert_eq!(reply["resumed_from_storage"], true, "{reply}");
}
