//! A daemon notification crosses the socket into the web event stream.
//!
//! The browser reducer test proves that `notification_added` shows a toast.
//! This test proves that the event reaches the web server's per-session
//! stream from a real daemon, in the shape the browser reads.

use crucible_daemon::{BindWithPluginConfigParams, DaemonClient, Server};
use crucible_web::services::daemon::{EventBroker, ReconnectingDaemon};
use crucible_web::ChatEvent;
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
        .call("lua.eval", serde_json::json!({ "code": code }))
        .await
        .expect("lua.eval");

    let data = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = stream.recv().await.expect("the stream ended");
            if let ChatEvent::SessionEvent { event, data } = ChatEvent::from_daemon_event(&event) {
                if event == "notification_added" && data["notification"]["message"] == message {
                    return data;
                }
            }
        }
    })
    .await
    .expect("the notification never reached the web stream");
    assert_eq!(data["notification"]["kind"], "warning", "{data}");
}
