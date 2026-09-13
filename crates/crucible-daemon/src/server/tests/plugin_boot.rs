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

/// The `notification_added` on `event_rx` that carries `message`, or a
/// panic after two seconds. The boot also loads the developer's own plugins
/// from `user_plugins_dir()` and `CRUCIBLE_PLUGIN_PATH`, and one of them can
/// notify first, so the first event of that type is not enough.
async fn notification_added_with_message(
    event_rx: &mut broadcast::Receiver<SessionEventMessage>,
    message: &str,
) -> SessionEventMessage {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match event_rx.recv().await {
                Ok(event)
                    if event.event == "notification_added"
                        && event.data["notification"]["message"] == message =>
                {
                    return event
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed: {err}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("timed out: no notification_added {message:?} after the plugin boot")
    })
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

    server.boot_plugins().await;

    let loader = server.plugin_loader.lock().await;
    let loader = loader.as_ref().expect("loader present");
    loader
        .eval(r#"cru.log.notify("from the plugin boot")"#)
        .await
        .expect("cru.log.notify must run on the booted plugin VM");

    let event = notification_added_with_message(&mut event_rx, "from the plugin boot").await;
    assert_eq!(event.session_id, WILDCARD_SESSION);
}

/// A git plugin the bootstrap put on disk has no spec entry yet: the
/// operator asked for it through `plugins.installed.json`, not through
/// `cru.plugin.setup`. The interim loop after the spec-driven pass activates
/// it by name.
///
/// Task 10 moves declarations into the spec; keep or retire this test then.
///
/// The URL's scheme is one `normalize_git_url` refuses, so the bootstrap
/// clones nothing and touches no network. The plugin's directory sits on the
/// injected `runtimepath`, which is where discovery finds it.
#[tokio::test(flavor = "multi_thread")]
async fn the_plugin_boot_activates_a_bootstrapped_plugin_that_has_no_spec_entry() {
    let tmp = TempDir::new().unwrap();
    let data_home = tmp.path().join("data");
    let runtimepath = tmp.path().join("rp");
    let plugin_dir = runtimepath.join("plugins").join("boot-interim-probe");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("init.luau"),
        "_G.boot_interim_probe_ran = true\nreturn {}\n",
    )
    .unwrap();
    std::fs::create_dir_all(&data_home).unwrap();
    std::fs::write(
        crate::plugin_ops::installed_manifest_path(&data_home),
        serde_json::json!({
            "version": crate::plugin_ops::INSTALLED_PLUGINS_VERSION,
            "plugins": {
                "boot-interim-probe": { "url": "file:///nowhere/boot-interim-probe" }
            }
        })
        .to_string(),
    )
    .unwrap();

    let server = Server::bind_with_plugin_config(BindWithPluginConfigParams {
        path: tmp.path().join("d.sock"),
        runtimepath: vec![runtimepath],
        config_home: Some(data_home.join("config")),
        data_home: Some(data_home),
        ..Default::default()
    })
    .await
    .expect("bind");

    server.boot_plugins().await;

    let loader = server.plugin_loader.lock().await;
    let loader = loader.as_ref().expect("loader present");
    assert_eq!(
        loader.plugin_state("boot-interim-probe"),
        Some(crucible_lua::manifest::PluginState::Active)
    );
    let ran: bool = loader
        .lua()
        .globals()
        .get("boot_interim_probe_ran")
        .expect("the plugin's body ran on the plugin VM");
    assert!(ran);
}
