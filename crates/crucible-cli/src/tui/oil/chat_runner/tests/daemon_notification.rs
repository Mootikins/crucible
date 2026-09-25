//! A daemon notification crosses the socket into the TUI notification area.

use super::super::*;
use tokio::sync::mpsc;

/// A plugin `cru.log.notify` on a real daemon reaches the TUI as the same
/// notification, with its kind.
#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_notification_reaches_the_tui_over_the_socket() {
    let tmp = tempfile::TempDir::new().unwrap();
    let socket = tmp.path().join("d.sock");
    let server = crucible_daemon::Server::bind_with_plugin_config(
        crucible_daemon::BindWithPluginConfigParams {
            path: socket.clone(),
            data_home: Some(tmp.path().join("data")),
            config_home: Some(tmp.path().join("config")),
            ..Default::default()
        },
    )
    .await
    .expect("the daemon binds");
    tokio::spawn(server.run());

    let (client, event_rx) = loop {
        match crucible_daemon::DaemonClient::connect_to_with_events(&socket).await {
            Ok(connected) => break connected,
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
        }
    };
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    tokio::spawn(session_event_consumer(
        "my-session".to_string(),
        event_rx,
        msg_tx,
        None,
    ));

    let message = "a notice from the daemon";
    // The daemon accepts only after the plugin boot, so the VM has its sink.
    let code = format!("cru.log.notify({message:?}, cru.log.levels.WARN)");
    client
        .call("lua.eval", serde_json::json!({ "code": code }))
        .await
        .expect("lua.eval");

    let notification = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match msg_rx.recv().await.expect("the consumer ended") {
                ChatAppMsg::Notification(n) if n.message == message => return n,
                _ => continue,
            }
        }
    })
    .await
    .expect("the notification never reached the TUI");
    assert_eq!(
        notification.kind,
        crucible_core::types::NotificationKind::Warning
    );
}

/// A TUI that attaches to a session reads the notifications that the
/// session already has, over the socket, and not those of another session.
#[tokio::test(flavor = "multi_thread")]
async fn an_attaching_tui_reads_the_notifications_of_its_session() {
    let tmp = tempfile::TempDir::new().unwrap();
    let socket = tmp.path().join("d.sock");
    let server = crucible_daemon::Server::bind_with_plugin_config(
        crucible_daemon::BindWithPluginConfigParams {
            path: socket.clone(),
            data_home: Some(tmp.path().join("data")),
            config_home: Some(tmp.path().join("config")),
            ..Default::default()
        },
    )
    .await
    .expect("the daemon binds");
    tokio::spawn(server.run());
    let client = crucible_daemon::DaemonClient::connect_to(&socket)
        .await
        .expect("connect");

    let mut ids = Vec::new();
    for message in ["mine", "other"] {
        let created = client
            .call("session.create", serde_json::json!({ "type": "chat" }))
            .await
            .expect("session.create");
        let id = created["session_id"].as_str().unwrap().to_string();
        let notification = crucible_core::types::Notification::warning(message);
        client
            .call(
                "session.add_notification",
                serde_json::json!({ "session_id": id, "notification": notification }),
            )
            .await
            .expect("session.add_notification");
        ids.push(id);
    }

    let msgs = OilChatRunner::notification_msgs(&client, &ids[0]).await;
    assert!(
        matches!(&msgs[..], [ChatAppMsg::Notification(n)] if n.message == "mine"),
        "{msgs:?}"
    );
}
