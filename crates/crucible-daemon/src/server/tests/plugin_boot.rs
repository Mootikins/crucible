//! The plugin boot binds the daemon's one `NotificationHub` to both VMs.
//!
//! The Lua tests under `agent_manager/tests/notifications.rs` build their
//! own hub and bind it themselves, so they cannot see a missing bind in
//! `boot_plugins`. This test runs the production boot on an in-process
//! daemon and reads the result from the two seams `boot_plugins` owns: the
//! agent manager's hub slot, and the plugin VM's `cru.log.notify`.

use super::*;
use crate::subscription::WILDCARD_SESSION;
use std::time::Duration;

/// The next `notification_added` on `event_rx`, or a panic after two seconds.
async fn next_notification_added(
    event_rx: &mut broadcast::Receiver<SessionEventMessage>,
) -> SessionEventMessage {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.event == "notification_added" => return event,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed: {err}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out: no notification_added after the plugin boot"))
}

/// After `boot_plugins`, the agent manager holds the hub, and a
/// `cru.log.notify` on the plugin VM reaches every client on the wildcard.
#[tokio::test(flavor = "multi_thread")]
async fn the_plugin_boot_binds_the_notification_hub_to_the_agent_manager_and_the_plugin_vm() {
    let tmp = TempDir::new().unwrap();
    let sock = tmp.path().join("d.sock");
    let server = Server::bind_with_data_home(&sock, tmp.path().join("data"))
        .await
        .expect("bind");
    let mut event_rx = server.rpc_context.event_tx.subscribe();

    assert!(
        server.agent_manager.notification_hub().is_none(),
        "the bind alone must not bind the hub; the plugin boot does"
    );

    server.boot_plugins().await;

    assert!(
        server.agent_manager.notification_hub().is_some(),
        "boot_plugins must bind the hub, or every session VM queues cru.log.notify"
    );

    let loader = server.plugin_loader.lock().await;
    let loader = loader.as_ref().expect("loader present");
    loader
        .eval(r#"cru.log.notify("from the plugin boot")"#)
        .await
        .expect("cru.log.notify must run on the booted plugin VM");

    let event = next_notification_added(&mut event_rx).await;
    assert_eq!(event.session_id, WILDCARD_SESSION);
    assert_eq!(
        event.data["notification"]["message"],
        "from the plugin boot"
    );
}
