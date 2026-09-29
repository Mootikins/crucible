//! RPC contract tests for notification methods.
//!
//! These tests define the expected request/response shapes for notification
//! RPC methods.
//!
//! Contract methods:
//! - `session.add_notification` - Add a notification, scoped to the session
//! - `session.list_notifications` - The notifications of one session
//! - `session.dismiss_notification` - Remove one notification of that session,
//!   or hide a shared one for that session only
//! - `notification.list` - The daemon's own ring, as a client may see it
//! - `notification.dismiss` - Drop one entry of that ring

mod common;

use common::{RpcConn, TestDaemon};
use crucible_core::types::Notification;
use serde_json::json;

async fn setup_daemon() -> (TestDaemon, RpcConn) {
    let daemon = TestDaemon::start().await.expect("Failed to start daemon");

    let conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect to daemon");

    (daemon, conn)
}

async fn create_test_session(conn: &mut RpcConn, daemon: &TestDaemon) -> String {
    // Create a kiln directory in the daemon's temp directory
    let kiln_dir = daemon.socket_path.parent().unwrap().join("kiln");
    std::fs::create_dir_all(&kiln_dir).expect("Failed to create kiln dir");

    let response = conn
        .call_method(
            "session.create",
            json!({
                "type": "chat",
                "kiln": kiln_dir.to_string_lossy(),
            }),
            1,
        )
        .await;

    response["result"]["session_id"]
        .as_str()
        .expect("No session_id in response")
        .to_string()
}

#[tokio::test]

async fn test_add_notification_contract() {
    let (mut daemon, mut conn) = setup_daemon().await;
    let session_id = create_test_session(&mut conn, &daemon).await;

    let notification = Notification::toast("Test notification");

    let params = json!({
        "session_id": session_id,
        "notification": {
            "id": notification.id,
            "kind": "toast",
            "message": notification.message,
        }
    });

    let response = conn
        .call_method("session.add_notification", params, 2)
        .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"].is_object(), "Expected result object");
    assert_eq!(
        response["result"]["session_id"].as_str().unwrap(),
        session_id
    );
    assert!(response["result"]["success"].as_bool().unwrap());

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_add_notification_with_progress_kind() {
    let (mut daemon, mut conn) = setup_daemon().await;
    let session_id = create_test_session(&mut conn, &daemon).await;

    let notification = Notification::progress(5, 10, "Processing files");

    let params = json!({
        "session_id": session_id,
        "notification": {
            "id": notification.id,
            "kind": {
                "progress": {
                    "current": 5,
                    "total": 10,
                }
            },
            "message": notification.message,
        }
    });

    let response = conn
        .call_method("session.add_notification", params, 2)
        .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"]["success"].as_bool().unwrap());

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_add_notification_with_warning_kind() {
    let (mut daemon, mut conn) = setup_daemon().await;
    let session_id = create_test_session(&mut conn, &daemon).await;

    let notification = Notification::warning("Low disk space");

    let params = json!({
        "session_id": session_id,
        "notification": {
            "id": notification.id,
            "kind": "warning",
            "message": notification.message,
        }
    });

    let response = conn
        .call_method("session.add_notification", params, 2)
        .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"]["success"].as_bool().unwrap());

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_list_notifications_after_adding() {
    let (mut daemon, mut conn) = setup_daemon().await;
    let session_id = create_test_session(&mut conn, &daemon).await;

    let notification = Notification::toast("Test message");
    let add_params = json!({
        "session_id": session_id,
        "notification": {
            "id": notification.id.clone(),
            "kind": "toast",
            "message": notification.message.clone(),
        }
    });

    conn.call_method("session.add_notification", add_params, 2)
        .await;

    // The hub is the one store, so the ring lists what the session added.
    let response = conn
        .call_method("notification.list", json!({ "all": true }), 3)
        .await;

    let notifications = response["result"]["notifications"]
        .as_array()
        .expect("notifications should be array");
    assert_eq!(notifications.len(), 1, "Should have one notification");

    let notif = &notifications[0];
    assert_eq!(notif["id"].as_str().unwrap(), notification.id);
    assert_eq!(notif["kind"].as_str().unwrap(), "toast");
    assert_eq!(notif["message"].as_str().unwrap(), notification.message);

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_session_not_found_error() {
    let (mut daemon, mut conn) = setup_daemon().await;

    let params = json!({
        "session_id": "sess-nonexistent",
        "notification": { "id": "n", "kind": "toast", "message": "m" },
    });

    let response = conn
        .call_method("session.add_notification", params, 1)
        .await;

    assert!(response["error"].is_object(), "Expected error object");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"),
        "Error should mention session not found"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

async fn add_toast(conn: &mut RpcConn, session_id: &str, message: &str) -> String {
    let notification = Notification::toast(message);
    let response = conn
        .call_method(
            "session.add_notification",
            json!({
                "session_id": session_id,
                "notification": {
                    "id": notification.id,
                    "kind": "toast",
                    "message": message,
                },
            }),
            2,
        )
        .await;
    assert!(
        response["result"]["success"].as_bool().unwrap(),
        "{response}"
    );
    notification.id
}

async fn list_messages(conn: &mut RpcConn, session_id: &str) -> Vec<String> {
    let response = conn
        .call_method(
            "session.list_notifications",
            json!({ "session_id": session_id }),
            3,
        )
        .await;
    response["result"]["notifications"]
        .as_array()
        .unwrap_or_else(|| panic!("a list: {response}"))
        .iter()
        .map(|n| n["message"].as_str().unwrap().to_string())
        .collect()
}

/// One store, two sessions: each session lists and dismisses only its own
/// notifications.
#[tokio::test]
async fn a_session_lists_and_dismisses_only_its_own_notifications() {
    let (mut daemon, mut conn) = setup_daemon().await;
    let mine = create_test_session(&mut conn, &daemon).await;
    let other = create_test_session(&mut conn, &daemon).await;

    let id = add_toast(&mut conn, &mine, "mine").await;
    let other_id = add_toast(&mut conn, &other, "other").await;

    assert_eq!(list_messages(&mut conn, &mine).await, ["mine"]);

    let refused = conn
        .call_method(
            "session.dismiss_notification",
            json!({ "session_id": mine, "notification_id": other_id }),
            4,
        )
        .await;
    assert_eq!(refused["result"]["success"], false, "{refused}");
    assert_eq!(list_messages(&mut conn, &other).await, ["other"]);

    let dismissed = conn
        .call_method(
            "session.dismiss_notification",
            json!({ "session_id": mine, "notification_id": id }),
            5,
        )
        .await;
    assert_eq!(dismissed["result"]["success"], true, "{dismissed}");
    assert!(list_messages(&mut conn, &mine).await.is_empty());
    let ring = conn
        .call_method("notification.list", json!({ "all": true }), 6)
        .await;
    let ring = ring["result"]["notifications"].as_array().unwrap().clone();
    assert!(
        ring.iter().all(|n| n["id"] != id.as_str()),
        "the ring drops a notice of the session itself: {ring:?}"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]
async fn test_daemon_notification_list_and_dismiss_contract() {
    let (mut daemon, mut conn) = setup_daemon().await;

    let response = conn
        .call_method("notification.list", json!({ "all": true }), 1)
        .await;
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(
        response["result"]["notifications"],
        json!([]),
        "a fresh daemon has an empty ring: {response}"
    );

    let response = conn
        .call_method("notification.dismiss", json!({ "id": "notif-nonexist" }), 2)
        .await;
    assert_eq!(response["result"]["dismissed"], json!(false), "{response}");

    let response = conn
        .call_method("notification.list", json!({ "kilns": ["not a kiln!"] }), 3)
        .await;
    assert!(
        response["error"].is_object(),
        "a bad kiln name is refused, not ignored: {response}"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Send the first message of two kiln-less sessions in one workspace, with
/// Precognition on. Returns the no-kiln notices in the ring.
async fn no_kiln_notices(daemon: &TestDaemon) -> Vec<serde_json::Value> {
    use crucible_core::protocol::requests::SessionCreateParams;
    use crucible_daemon::rpc_client::DaemonClient;
    let client = DaemonClient::connect_to(&daemon.socket_path).await.unwrap();
    let workspace = daemon.home().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let agent = crucible_core::session::SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: crucible_core::config::BackendType::Ollama,
        model: "test-model".to_string(),
        system_prompt: String::new(),
        max_context_tokens: None,
        endpoint: None,
        env_overrides: Default::default(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
        context_budget: None,
        context_strategy: Default::default(),
        mode: None,
        tool_policy: None,
    };
    for _ in 0..2 {
        let created = client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kilns: Vec::new(),
                workspace: Some(workspace.clone()),
                recording_mode: None,
                recording_path: None,
                agent_type: None,
                isolation: None,
            })
            .await
            .unwrap();
        let session_id = created.id.as_str();
        client
            .session_configure_agent(session_id, &agent)
            .await
            .unwrap();
        // The provider does not answer. Precognition runs before it.
        let _ = client.session_send_message(session_id, "hello", true).await;
    }
    let mut conn = RpcConn::connect(&daemon.socket_path).await.unwrap();
    let response = conn
        .call_method("notification.list", json!({ "all": true }), 1)
        .await;
    response["result"]["notifications"]
        .as_array()
        .unwrap_or_else(|| panic!("a list: {response}"))
        .iter()
        .filter(|n| n["message"].as_str().unwrap().contains("no kiln"))
        .cloned()
        .collect()
}

/// Two kiln-less sessions in one workspace see one info notice, and the
/// notice names the setting that turns it off.
#[tokio::test]
async fn the_no_kiln_notice_is_info_and_once_per_workspace() {
    let mut daemon = TestDaemon::start().await.unwrap();

    let notices = no_kiln_notices(&daemon).await;

    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["kind"], "toast", "{}", notices[0]);
    let message = notices[0]["message"].as_str().unwrap();
    assert!(message.contains("precognition_notify_no_kiln"), "{message}");
    daemon.stop().await.unwrap();
}

/// `chat.precognition_notify_no_kiln = false` turns the notice off.
#[tokio::test]
async fn the_setting_turns_the_no_kiln_notice_off() {
    let mut daemon = TestDaemon::start_with_home_setup(|home| {
        let init = home.join(".config/crucible/init.lua");
        let mut text = std::fs::read_to_string(&init)?;
        text.push_str("cru.config.set({ chat = { precognition_notify_no_kiln = false } })\n");
        std::fs::write(init, text)?;
        Ok(())
    })
    .await
    .unwrap();

    assert!(no_kiln_notices(&daemon).await.is_empty());
    daemon.stop().await.unwrap();
}

async fn create_session_in(conn: &mut RpcConn, workspace: &std::path::Path) -> String {
    let created = conn
        .call_method(
            "session.create",
            json!({ "type": "chat", "workspace": workspace }),
            1,
        )
        .await;
    created["result"]["session_id"]
        .as_str()
        .unwrap_or_else(|| panic!("a session: {created}"))
        .to_string()
}

/// Queue three notices through `cru.log.notify`: a global one, one for
/// `mine` and one for `theirs`. Returns the ring once the hub stored all
/// three.
async fn notify_three(
    conn: &mut RpcConn,
    mine: &std::path::Path,
    theirs: &std::path::Path,
) -> Vec<serde_json::Value> {
    let code = format!(
        "cru.log.notify('global') \
         cru.log.notify('workspace', cru.log.levels.INFO, {{ workspace = {mine:?} }}) \
         cru.log.notify('other', cru.log.levels.INFO, {{ workspace = {theirs:?} }}) \
         return 'ok'"
    );
    let eval = conn
        .call_method("lua.eval", json!({ "code": code }), 2)
        .await;
    assert_eq!(eval["result"]["result"], "ok", "{eval}");

    // The hub stores what `cru.log.notify` queued on its own task.
    loop {
        let listed = conn
            .call_method("notification.list", json!({ "all": true }), 3)
            .await;
        let ring = listed["result"]["notifications"]
            .as_array()
            .unwrap()
            .clone();
        if ring.len() == 3 {
            return ring;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

fn id_of<'a>(ring: &'a [serde_json::Value], message: &str) -> &'a serde_json::Value {
    &ring.iter().find(|n| n["message"] == message).unwrap()["id"]
}

async fn sorted_messages(conn: &mut RpcConn, session_id: &str) -> Vec<String> {
    let mut listed = list_messages(conn, session_id).await;
    listed.sort();
    listed
}

/// A session lists every notice the daemon would deliver to it: a notice
/// for its workspace and a global notice, not a notice for another
/// workspace. It cannot close a notice that does not reach it.
#[tokio::test]
async fn a_session_lists_the_shared_notices_that_reach_it() {
    let (mut daemon, mut conn) = setup_daemon().await;
    let mine = daemon.home().join("mine");
    let theirs = daemon.home().join("theirs");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::create_dir_all(&theirs).unwrap();
    let session_id = create_session_in(&mut conn, &mine).await;
    let ring = notify_three(&mut conn, &mine, &theirs).await;

    assert_eq!(
        sorted_messages(&mut conn, &session_id).await,
        ["global", "workspace"]
    );

    let refused = conn
        .call_method(
            "session.dismiss_notification",
            json!({ "session_id": session_id, "notification_id": id_of(&ring, "other") }),
            4,
        )
        .await;
    assert_eq!(refused["result"]["success"], false, "{refused}");
    assert_eq!(list_messages(&mut conn, &session_id).await.len(), 2);

    daemon.stop().await.unwrap();
}

/// Session A closes a shared notice. A does not see it again, also on a new
/// attach. Session B in the same workspace still sees it, and the ring
/// keeps it.
#[tokio::test]
async fn a_shared_notice_closed_in_one_session_still_reaches_the_other() {
    let (mut daemon, mut conn) = setup_daemon().await;
    let mine = daemon.home().join("mine");
    let theirs = daemon.home().join("theirs");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::create_dir_all(&theirs).unwrap();
    let a = create_session_in(&mut conn, &mine).await;
    let b = create_session_in(&mut conn, &mine).await;
    let ring = notify_three(&mut conn, &mine, &theirs).await;

    for (request, message) in [(4, "workspace"), (5, "global")] {
        let closed = conn
            .call_method(
                "session.dismiss_notification",
                json!({ "session_id": a, "notification_id": id_of(&ring, message) }),
                request,
            )
            .await;
        assert_eq!(closed["result"]["success"], true, "{closed}");
    }

    assert!(list_messages(&mut conn, &a).await.is_empty());
    assert_eq!(
        sorted_messages(&mut conn, &b).await,
        ["global", "workspace"]
    );

    let mut attach = RpcConn::connect(&daemon.socket_path).await.unwrap();
    assert!(
        list_messages(&mut attach, &a).await.is_empty(),
        "a new attach of A must not bring the closed notices back"
    );
    let again = attach
        .call_method(
            "session.dismiss_notification",
            json!({ "session_id": a, "notification_id": id_of(&ring, "global") }),
            6,
        )
        .await;
    assert_eq!(again["result"]["success"], true, "a second close: {again}");

    let listed = attach
        .call_method("notification.list", json!({ "all": true }), 7)
        .await;
    assert_eq!(
        listed["result"]["notifications"].as_array().unwrap().len(),
        3,
        "the ring keeps a shared notice: {listed}"
    );

    daemon.stop().await.unwrap();
}
