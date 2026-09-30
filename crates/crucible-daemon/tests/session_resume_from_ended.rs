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

/// A resume from storage warns about what the revival did not bring back:
/// the session's own Lua VM state and any work in flight when it ended,
/// neither of which storage holds.
#[tokio::test]
async fn session_resume_from_storage_warns_about_lost_state() {
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
        .expect("session.resume should revive an ended session");

    assert_eq!(reply["resumed_from_storage"], true, "{reply}");
    let warnings = reply["warnings"]
        .as_array()
        .expect("a stored resume must carry a warnings array");
    let kinds: Vec<&str> = warnings
        .iter()
        .map(|w| w["kind"].as_str().expect("each warning names its kind"))
        .collect();
    assert!(
        kinds.contains(&"plugin_state_reset"),
        "a stored resume tore down the session's Lua VM: {reply}"
    );
    assert!(
        kinds.contains(&"pending_work_cleared"),
        "a stored resume cancelled any work in flight when the session ended: {reply}"
    );
}

/// A resume that stays in memory (a `Paused` session) never touched
/// `AgentManager::cleanup_session`, so it lost nothing and the reply omits
/// `warnings` entirely rather than sending an empty array.
#[tokio::test]
async fn session_resume_live_has_no_warnings_key() {
    let server = InProcessDaemonBuilder::new()
        .expect("a test daemon builder")
        .start()
        .await
        .expect("failed to start server");
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("failed to connect");

    let session = client
        .session_create(SessionCreateRequest {
            session_type: "chat".to_string(),
            ..Default::default()
        })
        .await
        .expect("session_create failed");
    let id = session.id.to_string();

    client
        .session_pause(&id)
        .await
        .expect("session_pause failed");

    let reply: Value = client
        .session_resume(&id)
        .await
        .expect("session.resume should revive a paused session in memory");

    assert_eq!(reply["state"], "active", "{reply}");
    assert!(
        reply.get("resumed_from_storage").is_none(),
        "a live resume must omit resumed_from_storage, not send false: {reply}"
    );
    assert!(
        reply.get("warnings").is_none(),
        "a live resume must omit warnings, not send an empty array: {reply}"
    );
}

/// A kiln the session used, but that no longer resolves in the registry by
/// the time it revives, gets its own warning naming the stored path — the
/// session keeps searching its other kilns, but this one contributes
/// nothing until it is re-registered.
#[tokio::test]
async fn session_resume_from_storage_warns_about_an_unresolved_kiln() {
    let data_home = tempfile::tempdir().expect("a shared data home");
    let kiln_path = data_home.path().join("kiln-gone");

    let server = InProcessDaemonBuilder::at_data_home(data_home.path().to_path_buf())
        .with_kiln_at("kiln-gone", &kiln_path)
        .start()
        .await
        .expect("failed to start server");
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("failed to connect");

    let session = client
        .session_create(SessionCreateRequest {
            session_type: "chat".to_string(),
            kilns: SessionCreateRequest::kiln_set(vec![crucible_daemon::test_support::kiln_name(
                "kiln-gone",
            )]),
            ..Default::default()
        })
        .await
        .expect("session_create failed");
    let id = session.id.to_string();
    client.session_end(&id).await.expect("session_end failed");
    drop(client);
    server.shutdown().await;

    // A second daemon, same data root, that never registers "kiln-gone": the
    // registry it resolves against no longer has that name.
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

    let warnings = reply["warnings"]
        .as_array()
        .expect("a stored resume must carry a warnings array");
    let unresolved = warnings
        .iter()
        .find(|w| w["kind"] == "kiln_unavailable")
        .unwrap_or_else(|| panic!("expected a kiln_unavailable warning: {reply}"));
    let path = unresolved["path"]
        .as_str()
        .expect("kiln_unavailable names the stored path");
    assert!(
        path.ends_with("kiln-gone"),
        "the warning should name the kiln that no longer resolves, got {path}"
    );
}
