//! `cru.log.notify` crosses from Lua into the daemon's `NotificationHub`.
//!
//! The hub's own tests call the Rust sink directly. These two drive the call
//! from Lua on each daemon VM, so a missing `upgrade_with_notify_sink` in
//! `DaemonPluginLoader` fails here.

use super::*;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::notifications::NotificationHub;
use crate::project_manager::ProjectManager;
use crate::subscription::WILDCARD_SESSION;

struct HubFixture {
    _data_home: TempDir,
    _workspace: TempDir,
    session_id: String,
    hub: Arc<NotificationHub>,
    event_rx: broadcast::Receiver<SessionEventMessage>,
}

/// One live session with the kiln `kiln`, and a hub bound to the agent
/// manager BEFORE the plugin boot binds one. `ReactorTestHarness::new` builds
/// the VM inside `configure_agent`, which is too early for a hub bound
/// afterwards; the daemon binds the hub at boot, before any session runs.
async fn hub_fixture() -> HubFixture {
    let (workspace, session_manager, session) = setup_session_manager().await;
    let data_home = TempDir::new().unwrap();
    let (event_tx, event_rx) = broadcast::channel::<SessionEventMessage>(64);
    let projects = Arc::new(ProjectManager::new(data_home.path().join("projects.json")));
    let hub = Arc::new(NotificationHub::new(
        data_home.path(),
        session_manager.clone(),
        projects,
        event_tx,
    ));
    hub.spawn_drain();
    HubFixture {
        _data_home: data_home,
        _workspace: workspace,
        session_id: session.id.to_string(),
        hub,
        event_rx,
    }
}

#[tokio::test]
async fn cru_log_notify_on_the_plugin_vm_is_global_unless_the_call_names_a_scope() {
    let mut f = hub_fixture().await;

    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .upgrade_with_notify_sink(f.hub.sink(None))
        .expect("the plugin VM must accept the hub sink");
    loader
        .plugin_lua()
        .load(
            r#"
            cru.log.notify("for everyone")
            cru.log.notify("for the kiln", cru.log.levels.INFO, { kiln = "kiln" })
            "#,
        )
        .exec()
        .expect("cru.log.notify must run on the plugin VM");

    let global = next_event_or_skip(&mut f.event_rx, "notification_added").await;
    assert_eq!(global.session_id, WILDCARD_SESSION);
    assert_eq!(global.data["notification"]["message"], "for everyone");
    assert!(global.data["notification"]["scope"]["kilns"]
        .as_array()
        .is_none_or(|k| k.is_empty()));

    let scoped = next_event_or_skip(&mut f.event_rx, "notification_added").await;
    assert_eq!(scoped.session_id, f.session_id);
    assert_eq!(scoped.data["notification"]["message"], "for the kiln");
    assert_eq!(scoped.data["notification"]["scope"]["kilns"][0], "kiln");
}
