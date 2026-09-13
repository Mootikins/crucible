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

/// A server over one fixture plugin `<rp>/plugins/<name>/init.luau` whose
/// body sets the global `<name>_ran`, recorded in the installed manifest
/// under `data_home`. The URL's scheme is one `normalize_git_url` refuses,
/// so the bootstrap clones nothing and touches no network; discovery finds
/// the directory on the injected `runtimepath`.
async fn server_with_installed_plugin(tmp: &TempDir, name: &str) -> Server {
    let data_home = tmp.path().join("data");
    let runtimepath = tmp.path().join("rp");
    let plugin_dir = runtimepath.join("plugins").join(name);
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("init.luau"),
        format!("_G.{}_ran = true\nreturn {{}}\n", name.replace('-', "_")),
    )
    .unwrap();
    std::fs::create_dir_all(&data_home).unwrap();
    std::fs::write(
        crate::plugin_ops::installed_manifest_path(&data_home),
        serde_json::json!({
            "version": crate::plugin_ops::INSTALLED_PLUGINS_VERSION,
            "plugins": {
                name: { "url": format!("file:///nowhere/{name}") }
            }
        })
        .to_string(),
    )
    .unwrap();

    Server::bind_with_plugin_config(BindWithPluginConfigParams {
        path: tmp.path().join("d.sock"),
        runtimepath: vec![runtimepath],
        config_home: Some(data_home.join("config")),
        data_home: Some(data_home),
        ..Default::default()
    })
    .await
    .expect("bind")
}

/// Whether the fixture plugin's body set its global on the plugin VM.
fn plugin_ran(loader: &crate::daemon_plugins::DaemonPluginLoader, name: &str) -> bool {
    loader
        .lua()
        .globals()
        .get::<Option<bool>>(format!("{}_ran", name.replace('-', "_")))
        .expect("a boolean or nil")
        .unwrap_or(false)
}

/// The operator asked for an installed plugin through `cru plugin add`, not
/// through `cru.plugin.setup`. The boot merges the manifest into the spec
/// at Builtin rank, so the spec-driven pass activates it like any entry.
#[tokio::test(flavor = "multi_thread")]
async fn an_installed_plugin_with_no_operator_entry_is_activated_at_boot() {
    let tmp = TempDir::new().unwrap();
    let server = server_with_installed_plugin(&tmp, "boot-installed-probe").await;

    server.boot_plugins().await;

    let loader = server.plugin_loader.lock().await;
    let loader = loader.as_ref().expect("loader present");
    assert_eq!(
        loader.plugin_state("boot-installed-probe"),
        Some(crucible_lua::manifest::PluginState::Active)
    );
    assert!(plugin_ran(loader, "boot-installed-probe"));
    let spec = crucible_lua::spec_of(loader.lua());
    assert_eq!(
        spec.rank_of("boot-installed-probe"),
        Some(crucible_core::config::SpecRank::Builtin),
        "the installed manifest is a Builtin-rank fragment source"
    );
}

/// An install sits below the operator's own `init.lua`: `{ "<name>",
/// enabled = false }` there disables an installed plugin, and its body
/// never runs.
#[tokio::test(flavor = "multi_thread")]
async fn an_installed_plugin_is_a_builtin_rank_entry_the_operator_can_disable() {
    let tmp = TempDir::new().unwrap();
    let server = server_with_installed_plugin(&tmp, "boot-quiet-probe").await;
    {
        let loader = server.plugin_loader.lock().await;
        loader
            .as_ref()
            .expect("loader present")
            .eval_user_init(r#"cru.plugin.setup({ { "boot-quiet-probe", enabled = false } })"#)
            .await
            .expect("the operator's init.lua");
    }

    server.boot_plugins().await;

    let loader = server.plugin_loader.lock().await;
    let loader = loader.as_ref().expect("loader present");
    assert_eq!(
        loader.plugin_state("boot-quiet-probe"),
        Some(crucible_lua::manifest::PluginState::Disabled)
    );
    assert!(
        !plugin_ran(loader, "boot-quiet-probe"),
        "a disabled plugin's body must not run"
    );
}

/// A `cru.plugin.setup` entry with a git source is cloned at boot and then
/// activated by the spec-driven pass. Here the clone is refused (the URL's
/// scheme is not allowed), and the directory already sits on the
/// runtimepath, so the test reads the activation alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_declared_git_plugin_is_activated_at_boot() {
    let tmp = TempDir::new().unwrap();
    let data_home = tmp.path().join("data");
    let runtimepath = tmp.path().join("rp");
    let plugin_dir = runtimepath.join("plugins").join("boot-declared-probe");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("init.luau"),
        "_G.boot_declared_probe_ran = true\nreturn {}\n",
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
    {
        let loader = server.plugin_loader.lock().await;
        loader
            .as_ref()
            .expect("loader present")
            .eval_user_init(r#"cru.plugin.setup({ "file:///nowhere/boot-declared-probe" })"#)
            .await
            .expect("the operator's init.lua");
    }

    server.boot_plugins().await;

    let loader = server.plugin_loader.lock().await;
    let loader = loader.as_ref().expect("loader present");
    assert_eq!(
        loader.plugin_state("boot-declared-probe"),
        Some(crucible_lua::manifest::PluginState::Active)
    );
    assert!(plugin_ran(loader, "boot-declared-probe"));
    let spec = crucible_lua::spec_of(loader.lua());
    assert!(crate::daemon_plugins::declared_git_entry(
        &spec,
        "boot-declared-probe"
    ));
}
