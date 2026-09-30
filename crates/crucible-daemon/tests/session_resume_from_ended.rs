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
    let cache_warning = warnings
        .iter()
        .find(|w| w["kind"] == "prompt_cache_cold")
        .unwrap_or_else(|| panic!("a stored resume always builds a new agent handle: {reply}"));
    let idle = cache_warning["idle_seconds"]
        .as_i64()
        .expect("prompt_cache_cold names how long the session sat idle");
    assert!(idle >= 0, "idle_seconds must not be negative: {reply}");
}

/// `idle_seconds` grows with how long the session actually sat ended, so a
/// caller who reads it gets real information, not a constant.
#[tokio::test]
async fn prompt_cache_cold_names_real_idle_time() {
    let server = InProcessDaemonBuilder::new()
        .expect("a test daemon builder")
        .start()
        .await
        .expect("failed to start server");
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("failed to connect");

    let id = ended_session(&client).await;
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

    let reply: Value = client
        .session_resume(&id)
        .await
        .expect("session.resume should revive an ended session");

    let idle = reply["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .find(|w| w["kind"] == "prompt_cache_cold")
        .expect("prompt_cache_cold")["idle_seconds"]
        .as_i64()
        .expect("idle_seconds");
    assert!(
        idle >= 1,
        "the session sat idle over a second: idle_seconds={idle}"
    );
}

/// An ACP session that never finished a turn has no `acp_session_id` to
/// resume, so the next handshake calls `session/new` with no history —
/// `ContextNotRestored` names that.
#[tokio::test]
async fn session_resume_warns_when_an_acp_session_never_had_an_agent_side_id() {
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

    let agent = crucible_core::session::SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("never-connected".to_string()),
        provider_key: None,
        provider: crucible_core::config::BackendType::Mock,
        model: "mock-model".to_string(),
        system_prompt: String::new(),
        max_context_tokens: None,
        endpoint: None,
        env_overrides: Default::default(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        context_budget: None,
        context_strategy: Default::default(),
        mode: None,
        tool_policy: None,
    };
    client
        .session_configure_agent(&id, &agent)
        .await
        .expect("configure the session as an ACP agent, without ever connecting one");

    // No message was ever sent, so no `acp_session_id` was ever persisted.
    client.session_end(&id).await.expect("session_end failed");

    let reply: Value = client
        .session_resume(&id)
        .await
        .expect("session.resume should revive an ended session");

    let kinds: Vec<&str> = reply["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .map(|w| w["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"context_not_restored"),
        "an ACP session with no agent-side id must warn that its next turn starts empty: {reply}"
    );
}

/// A non-ACP (internal) session never gets `context_not_restored`: only an
/// ACP session names an agent-side history that could fail to come back.
#[tokio::test]
async fn session_resume_never_warns_context_not_restored_for_an_internal_session() {
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

    let kinds: Vec<&str> = reply["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .map(|w| w["kind"].as_str().unwrap())
        .collect();
    assert!(
        !kinds.contains(&"context_not_restored"),
        "an internal session has no agent-side history to lose: {reply}"
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
