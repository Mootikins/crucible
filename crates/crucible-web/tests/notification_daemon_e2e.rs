//! A daemon notification crosses the socket into the web event stream.
//!
//! The browser reducer test proves that `notification_added` shows a toast.
//! This test proves that the event reaches the web server's per-session
//! stream from a real daemon, in the shape the browser reads.

use crucible_core::protocol::RpcMethod;
use crucible_daemon::{BindWithPluginConfigParams, DaemonClient, Server};
use crucible_web::services::daemon::{EventBroker, ReconnectingDaemon};
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_notification_reaches_a_web_session_stream() {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("d.sock");
    let server = Server::bind_with_plugin_config(BindWithPluginConfigParams {
        path: socket.clone(),
        data_home: Some(tmp.path().join("data")),
        config_home: Some(tmp.path().join("config")),
        ..Default::default()
    })
    .await
    .expect("the daemon binds");
    tokio::spawn(server.run());

    let (client, event_rx) = DaemonClient::connect_to_with_events(&socket)
        .await
        .expect("connect");
    let broker = Arc::new(EventBroker::new());
    let _daemon = ReconnectingDaemon::new(client, event_rx, broker.clone());
    let mut stream = broker.subscribe("my-session").await;

    let message = "a notice from the daemon";
    // The daemon accepts only after the plugin boot, so the VM has its sink.
    let caller = DaemonClient::connect_to(&socket).await.expect("connect");
    let code = format!("cru.log.notify({message:?}, cru.log.levels.WARN)");
    caller
        .call::<_, serde_json::Value>(RpcMethod::LuaEval, serde_json::json!({ "code": code }))
        .await
        .expect("lua.eval");

    let data = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = stream.recv().await.expect("the stream ended");
            if event.event == "notification_added"
                && event.data["notification"]["message"] == message
            {
                return event.data;
            }
        }
    })
    .await
    .expect("the notification never reached the web stream");
    assert_eq!(data["notification"]["kind"], "warning", "{data}");
}

/// A browser that attaches to a session reads the notifications of that
/// session through the web route, from a real daemon, and not those of
/// another session.
#[tokio::test(flavor = "multi_thread")]
async fn the_web_route_lists_the_notifications_of_one_session() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("d.sock");
    let server = Server::bind_with_plugin_config(BindWithPluginConfigParams {
        path: socket.clone(),
        data_home: Some(tmp.path().join("data")),
        config_home: Some(tmp.path().join("config")),
        ..Default::default()
    })
    .await
    .expect("the daemon binds");
    tokio::spawn(server.run());
    let client = DaemonClient::connect_to(&socket).await.expect("connect");

    let mut ids = Vec::new();
    for message in ["mine", "other"] {
        let created = client
            .call::<_, serde_json::Value>(
                RpcMethod::SessionCreate,
                serde_json::json!({ "type": "chat" }),
            )
            .await
            .expect("session.create");
        let id = created["session_id"].as_str().unwrap().to_string();
        let notification = crucible_core::types::Notification::warning(message);
        client
            .call::<_, serde_json::Value>(
                RpcMethod::SessionAddNotification,
                serde_json::json!({ "session_id": id, "notification": notification }),
            )
            .await
            .expect("session.add_notification");
        ids.push(id);
    }

    let app =
        crucible_web::test_support::build_test_app(crucible_web::test_support::build_state(client));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc/session.list_notifications")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "session_id": ids[0] }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let list = json["notifications"].as_array().expect("a list");
    assert_eq!(list.len(), 1, "{json}");
    assert_eq!(list[0]["message"], "mine", "{json}");
    assert_eq!(list[0]["kind"], "warning", "{json}");
}

/// A browser closes a shared notice through the web route. The real daemon
/// hides it for that session only: the list of the session leaves it out,
/// and the list of the other session keeps it.
#[tokio::test(flavor = "multi_thread")]
async fn the_web_route_closes_a_shared_notice_for_one_session() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("d.sock");
    let server = Server::bind_with_plugin_config(BindWithPluginConfigParams {
        path: socket.clone(),
        data_home: Some(tmp.path().join("data")),
        config_home: Some(tmp.path().join("config")),
        ..Default::default()
    })
    .await
    .expect("the daemon binds");
    tokio::spawn(server.run());
    let client = DaemonClient::connect_to(&socket).await.expect("connect");

    let mut sessions = Vec::new();
    for _ in 0..2 {
        let created = client
            .call::<_, serde_json::Value>(
                RpcMethod::SessionCreate,
                serde_json::json!({ "type": "chat" }),
            )
            .await
            .expect("session.create");
        sessions.push(created["session_id"].as_str().unwrap().to_string());
    }
    client
        .call::<_, serde_json::Value>(
            RpcMethod::LuaEval,
            serde_json::json!({ "code": "cru.log.notify('shared')" }),
        )
        .await
        .expect("lua.eval");
    // The hub stores what `cru.log.notify` queued on its own task.
    let shared_id = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let listed = client
                .call::<_, serde_json::Value>(
                    RpcMethod::NotificationList,
                    serde_json::json!({ "all": true }),
                )
                .await
                .expect("notification.list");
            if let Some(id) = listed["notifications"][0]["id"].as_str() {
                return id.to_string();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the hub never stored the notice");

    let app =
        crucible_web::test_support::build_test_app(crucible_web::test_support::build_state(client));
    // `session.list_notifications` and `session.dismiss_notification` are
    // RPC methods now ([[Simplification Plan#Step 19]] item 9); the browser
    // calls `rpc(method, params)` through `POST /api/rpc/{method}`.
    let rpc_request = |method: &str, body: serde_json::Value| {
        Request::builder()
            .method("POST")
            .uri(format!("/api/rpc/{method}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let json_of = |response: axum::response::Response| async move {
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
    };

    // Two independent web clients have live streams before one closes the
    // notice. Each holds the older snapshot, as a browser does, and opens its
    // stream through the lease that a browser stream holds.
    let snapshot = json_of(
        app.clone()
            .oneshot(rpc_request(
                "session.list_notifications",
                serde_json::json!({ "session_id": sessions[0] }),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(snapshot["notifications"].as_array().unwrap().len(), 1);
    let mut streams = Vec::new();
    let mut connections = Vec::new();
    for _ in 0..2 {
        let (client, events) = DaemonClient::connect_to_with_events(&socket).await.unwrap();
        let daemon = Arc::new(ReconnectingDaemon::new(
            client,
            events,
            Arc::new(EventBroker::new()),
        ));
        streams.push(daemon.subscribe_events(&sessions[0]).await.unwrap());
        connections.push(daemon);
    }

    let closed = json_of(
        app.clone()
            .oneshot(rpc_request(
                "session.dismiss_notification",
                serde_json::json!({ "session_id": sessions[0], "notification_id": shared_id }),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(closed["success"], true, "{closed}");

    for stream in &mut streams {
        let dismissed = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let event = stream.next().await.expect("the stream stays open");
                if event.event == "notification_dismissed" {
                    break event.data["notification_id"].as_str().unwrap().to_string();
                }
            }
        })
        .await
        .expect("both web clients receive the dismissal");
        assert_eq!(dismissed, shared_id);

        // The browser applies the event to its older snapshot. The result
        // must be the list that a new read answers.
        let mut reconciled = snapshot["notifications"].as_array().unwrap().clone();
        reconciled.retain(|n| n["id"] != dismissed.as_str());
        let current = json_of(
            app.clone()
                .oneshot(rpc_request(
                    "session.list_notifications",
                    serde_json::json!({ "session_id": sessions[0] }),
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            &reconciled,
            current["notifications"].as_array().unwrap(),
            "the snapshot with the event applied differs from a new read"
        );
    }

    for (session, expected) in [(&sessions[0], 0), (&sessions[1], 1)] {
        let listed = json_of(
            app.clone()
                .oneshot(rpc_request(
                    "session.list_notifications",
                    serde_json::json!({ "session_id": session }),
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            listed["notifications"].as_array().unwrap().len(),
            expected,
            "{session}: {listed}"
        );
    }
}
