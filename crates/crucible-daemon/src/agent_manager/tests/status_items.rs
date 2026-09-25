//! The engine's plugin-turn status item (decision 10 of "Plugin Turns and
//! Status Items").
//!
//! The item shows while a plugin turn runs or while a plugin's approval is
//! not `inherit`. Its source is the session's real approval knob and the turn
//! that runs now. No plugin publishes it, and no client makes it up.

use super::*;
use crucible_core::session::PluginApproval;
use crucible_core::types::{StatusDisplayItem, StatusItemKind};

/// The list that one `status_items_changed` event carries.
fn items_of(event: &SessionEventMessage) -> Vec<StatusDisplayItem> {
    assert_eq!(event.event, "status_items_changed", "{event:?}");
    serde_json::from_value(event.data["status"].clone()).expect("the list decodes")
}

/// The next `status_items_changed` event of the session.
async fn next_items(h: &mut ReactorTestHarness) -> Vec<StatusDisplayItem> {
    items_of(&h.wait_for_first_of(&["status_items_changed"]).await)
}

/// The one engine item of `plugin`, with the fields that the clients draw.
fn plugin_item(plugin: &str, text: &str, color_group: &str) -> StatusDisplayItem {
    StatusDisplayItem {
        id: format!("plugin_turns:{plugin}"),
        text: text.into(),
        priority: 0,
        color_group: crucible_core::status_color::StatusColorGroup::from_name(color_group),
        action: Some("plugin_approval".into()),
        pinned: true,
        plugin: plugin.into(),
        kind: StatusItemKind::PluginTurns,
        progress: None,
    }
}

/// A change of the knob replaces the list at once: `ask` and `stop` pin an
/// item with the plugin's name, and `inherit` removes it.
#[tokio::test]
async fn the_approval_knob_drives_a_pinned_plugin_item() {
    let mut h = ReactorTestHarness::new().await;
    let (am, sid) = (h.agent_manager.clone(), h.session_id.clone());

    am.set_plugin_approval(&sid, "alpha", PluginApproval::Ask, Some(&h.event_tx))
        .await
        .unwrap();
    assert_eq!(
        next_items(&mut h).await,
        [plugin_item("alpha", "alpha · ask", "warn")]
    );

    am.set_plugin_approval(&sid, "alpha", PluginApproval::Stop, Some(&h.event_tx))
        .await
        .unwrap();
    assert_eq!(
        next_items(&mut h).await,
        [plugin_item("alpha", "alpha · stop", "danger")]
    );

    am.set_plugin_approval(&sid, "alpha", PluginApproval::Inherit, Some(&h.event_tx))
        .await
        .unwrap();
    assert_eq!(next_items(&mut h).await, []);
}

/// A plugin turn pins an item while it runs, and the item goes when the
/// turn ends. The item names the approval too, when it is not `inherit`.
#[tokio::test]
async fn a_running_plugin_turn_is_pinned_until_the_turn_ends() {
    let mut h = ReactorTestHarness::new().await;
    let (am, sid) = (h.agent_manager.clone(), h.session_id.clone());
    am.set_plugin_approval(&sid, "beta", PluginApproval::Ask, Some(&h.event_tx))
        .await
        .unwrap();
    assert_eq!(
        next_items(&mut h).await,
        [plugin_item("beta", "beta · ask", "warn")]
    );
    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());

    am.send_plugin_message(&sid, "from alpha".into(), "alpha".into(), &h.event_tx)
        .await
        .unwrap();
    let mut lists = Vec::new();
    timeout(Duration::from_secs(10), async {
        loop {
            let event = h.event_rx.recv().await.expect("the channel stays open");
            if event.event == "status_items_changed" {
                lists.push(items_of(&event));
            }
            if event.event == "turn_finished" {
                return;
            }
        }
    })
    .await
    .expect("the turn finishes");

    assert_eq!(
        lists,
        [
            vec![
                plugin_item("alpha", "↻ alpha", "info"),
                plugin_item("beta", "beta · ask", "warn"),
            ],
            vec![plugin_item("beta", "beta · ask", "warn")],
        ],
        "one list when the turn starts, one when it ends"
    );
}

/// A user turn is not a plugin turn: it changes no status item.
#[tokio::test]
async fn a_user_turn_publishes_no_plugin_item() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());
    h.agent_manager
        .send_message(&h.session_id, "hi".into(), &h.event_tx, true, None)
        .await
        .unwrap();
    let mut changed = 0;
    timeout(Duration::from_secs(10), async {
        loop {
            let event = h.event_rx.recv().await.expect("the channel stays open");
            changed += usize::from(event.event == "status_items_changed");
            if event.event == "turn_finished" {
                return;
            }
        }
    })
    .await
    .expect("the turn finishes");
    assert_eq!(changed, 0);
}

/// The read that a client makes when it attaches (`session.status`) holds
/// the same engine item, also for a session that is not in memory.
#[tokio::test]
async fn the_attach_read_holds_the_engine_item() {
    let h = ReactorTestHarness::new().await;
    let (am, sid) = (h.agent_manager.clone(), h.session_id.clone());
    am.set_plugin_approval(&sid, "alpha", PluginApproval::Stop, None)
        .await
        .unwrap();
    assert_eq!(
        am.status_items(&sid).await,
        [plugin_item("alpha", "alpha · stop", "danger")]
    );
    am.session_manager.end_session(&sid).await.unwrap();
    am.session_manager.remove_session(&sid).unwrap();
    assert_eq!(
        am.status_items(&sid).await,
        [plugin_item("alpha", "alpha · stop", "danger")],
        "a dormant session reads its approvals from storage"
    );
}

/// A plugin's `cru.statusline.publish` goes through the notifier that the
/// daemon installs. The event carries the engine item beside the published
/// one, so a plugin's list does not erase the approval state.
#[tokio::test]
async fn a_lua_publish_keeps_the_engine_item_in_the_event() {
    let mut h = ReactorTestHarness::new().await;
    let (am, sid) = (h.agent_manager.clone(), h.session_id.clone());
    am.set_plugin_approval(&sid, "alpha", PluginApproval::Ask, None)
        .await
        .unwrap();
    let registry = crucible_lua::StatusRegistry::new();
    am.set_status_registry(registry.clone());
    assert!(
        registry.set_change_notifier(crate::agent_manager::status_items::change_notifier(
            Arc::downgrade(&am),
            h.event_tx.clone(),
        ))
    );
    let lua = mlua::Lua::new();
    crucible_lua::register_status_module(&lua, registry).unwrap();
    lua.globals().set("sid", sid.clone()).unwrap();
    lua.load(
        r#"cru.statusline.publish(sid, {
            cru.statusline.item{ id = "sync", text = "sync idle", plugin = "sync", color = "ok" },
        })"#,
    )
    .exec()
    .unwrap();

    let items = next_items(&mut h).await;
    assert_eq!(items[0], plugin_item("alpha", "alpha · ask", "warn"));
    assert_eq!(items[1].text, "sync idle");
    assert_eq!(items[1].kind, StatusItemKind::Published);
    assert_eq!(items.len(), 2);
}

/// The event and the attach read carry one shape. A slot's progress, its
/// id and its color group reach both, so a client decodes each the same way.
#[tokio::test]
async fn the_event_and_the_attach_read_carry_the_same_item() {
    let mut h = ReactorTestHarness::new().await;
    let (am, sid) = (h.agent_manager.clone(), h.session_id.clone());
    let registry = crucible_lua::StatusRegistry::new();
    am.set_status_registry(registry.clone());
    assert!(
        registry.set_change_notifier(crate::agent_manager::status_items::change_notifier(
            Arc::downgrade(&am),
            h.event_tx.clone(),
        ))
    );
    let lua = mlua::Lua::new();
    crucible_lua::register_status_module(&lua, registry).unwrap();
    lua.globals().set("sid", sid.clone()).unwrap();
    lua.load(
        r#"cru.plugin.set_status{ session = sid, key = "pull", text = "pulling", progress = 0.5 }"#,
    )
    .exec()
    .unwrap();

    let event = h.wait_for_first_of(&["status_items_changed"]).await;
    assert_eq!(event.data["status"][0]["progress"], serde_json::json!(0.5));
    let read = serde_json::to_value(am.status_items(&sid).await).unwrap();
    assert_eq!(event.data["status"], read, "one wire shape for both");
}
