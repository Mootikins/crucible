//! RPC contract tests for notification methods.
//!
//! These tests define the expected request/response shapes for notification
//! RPC methods.
//!
//! Contract methods:
//! - `session.add_notification` - Add notification to session queue
//! - `session.list_notifications` - Get all notifications for session
//! - `session.dismiss_notification` - Remove notification by ID

mod common;

use common::TestDaemon;
use crucible_core::types::Notification;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

async fn setup_daemon() -> (TestDaemon, UnixStream) {
    let daemon = TestDaemon::start()
        .await
        .expect("Failed to start daemon");

    let stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect to daemon");

    (daemon, stream)
}

async fn rpc_call(
    stream: &mut UnixStream,
    method: &str,
    params: serde_json::Value,
    id: i64,
) -> serde_json::Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });

    let request_str = format!("{}\n", serde_json::to_string(&request).unwrap());

    stream
        .write_all(request_str.as_bytes())
        .await
        .expect("Failed to write request");

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await.expect("Failed to read response");

    serde_json::from_slice(&buf[..n]).expect("Failed to parse response")
}

async fn create_test_session(stream: &mut UnixStream, daemon: &TestDaemon) -> String {
    // Create a kiln directory in the daemon's temp directory
    let kiln_dir = daemon.socket_path.parent().unwrap().join("kiln");
    std::fs::create_dir_all(&kiln_dir).expect("Failed to create kiln dir");

    let response = rpc_call(
        stream,
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
    let (mut daemon, mut stream) = setup_daemon().await;
    let session_id = create_test_session(&mut stream, &daemon).await;

    let notification = Notification::toast("Test notification");

    let params = json!({
        "session_id": session_id,
        "notification": {
            "id": notification.id,
            "kind": "toast",
            "message": notification.message,
        }
    });

    let response = rpc_call(&mut stream, "session.add_notification", params, 2).await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"].is_object(), "Expected result object");
    assert_eq!(
        response["result"]["session_id"].as_str().unwrap(),
        session_id
    );
    assert_eq!(response["result"]["success"].as_bool().unwrap(), true);

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_list_notifications_contract() {
    let (mut daemon, mut stream) = setup_daemon().await;
    let session_id = create_test_session(&mut stream, &daemon).await;

    let params = json!({
        "session_id": session_id,
    });

    let response = rpc_call(&mut stream, "session.list_notifications", params, 2).await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"].is_object(), "Expected result object");
    assert_eq!(
        response["result"]["session_id"].as_str().unwrap(),
        session_id
    );
    assert!(
        response["result"]["notifications"].is_array(),
        "Expected notifications array"
    );

    let notifications = response["result"]["notifications"]
        .as_array()
        .expect("notifications should be array");
    assert_eq!(notifications.len(), 0, "Should start with no notifications");

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_dismiss_notification_contract() {
    let (mut daemon, mut stream) = setup_daemon().await;
    let session_id = create_test_session(&mut stream, &daemon).await;

    let params = json!({
        "session_id": session_id,
        "notification_id": "notif-12345678",
    });

    let response = rpc_call(&mut stream, "session.dismiss_notification", params, 2).await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response["result"].is_object(), "Expected result object");
    assert_eq!(
        response["result"]["session_id"].as_str().unwrap(),
        session_id
    );
    assert_eq!(
        response["result"]["notification_id"].as_str().unwrap(),
        "notif-12345678"
    );
    assert!(
        response["result"]["success"].is_boolean(),
        "Expected success boolean"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_add_notification_with_progress_kind() {
    let (mut daemon, mut stream) = setup_daemon().await;
    let session_id = create_test_session(&mut stream, &daemon).await;

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

    let response = rpc_call(&mut stream, "session.add_notification", params, 2).await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["success"].as_bool().unwrap(), true);

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_add_notification_with_warning_kind() {
    let (mut daemon, mut stream) = setup_daemon().await;
    let session_id = create_test_session(&mut stream, &daemon).await;

    let notification = Notification::warning("Low disk space");

    let params = json!({
        "session_id": session_id,
        "notification": {
            "id": notification.id,
            "kind": "warning",
            "message": notification.message,
        }
    });

    let response = rpc_call(&mut stream, "session.add_notification", params, 2).await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["success"].as_bool().unwrap(), true);

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_list_notifications_after_adding() {
    let (mut daemon, mut stream) = setup_daemon().await;
    let session_id = create_test_session(&mut stream, &daemon).await;

    let notification = Notification::toast("Test message");
    let add_params = json!({
        "session_id": session_id,
        "notification": {
            "id": notification.id.clone(),
            "kind": "toast",
            "message": notification.message.clone(),
        }
    });

    rpc_call(&mut stream, "session.add_notification", add_params, 2).await;

    let list_params = json!({
        "session_id": session_id,
    });

    let response = rpc_call(&mut stream, "session.list_notifications", list_params, 3).await;

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

async fn test_dismiss_notification_removes_from_list() {
    let (mut daemon, mut stream) = setup_daemon().await;
    let session_id = create_test_session(&mut stream, &daemon).await;

    let notification = Notification::toast("Test message");
    let add_params = json!({
        "session_id": session_id,
        "notification": {
            "id": notification.id.clone(),
            "kind": "toast",
            "message": notification.message.clone(),
        }
    });

    rpc_call(&mut stream, "session.add_notification", add_params, 2).await;

    let dismiss_params = json!({
        "session_id": session_id,
        "notification_id": notification.id,
    });

    let dismiss_response =
        rpc_call(&mut stream, "session.dismiss_notification", dismiss_params, 3).await;

    assert_eq!(
        dismiss_response["result"]["success"].as_bool().unwrap(),
        true,
        "Dismiss should succeed"
    );

    let list_params = json!({
        "session_id": session_id,
    });

    let list_response = rpc_call(&mut stream, "session.list_notifications", list_params, 4).await;

    let notifications = list_response["result"]["notifications"]
        .as_array()
        .expect("notifications should be array");
    assert_eq!(
        notifications.len(),
        0,
        "Should have no notifications after dismiss"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_dismiss_nonexistent_notification_returns_false() {
    let (mut daemon, mut stream) = setup_daemon().await;
    let session_id = create_test_session(&mut stream, &daemon).await;

    let params = json!({
        "session_id": session_id,
        "notification_id": "notif-nonexist",
    });

    let response = rpc_call(&mut stream, "session.dismiss_notification", params, 2).await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(
        response["result"]["success"].as_bool().unwrap(),
        false,
        "Should return false when notification not found"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

#[tokio::test]

async fn test_session_not_found_error() {
    let (mut daemon, mut stream) = setup_daemon().await;

    let params = json!({
        "session_id": "sess-nonexistent",
    });

    let response = rpc_call(&mut stream, "session.list_notifications", params, 1).await;

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
