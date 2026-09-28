//! Per-knob RPC arm verification for interactive `:set`.
//!
//! The `:set` dispatch matrix (chat_app/command_handling.rs) stops at
//! `Action::Send(msg)`, and the startup-override regression test
//! (initial_sets.rs) covers only context_strategy + model. Nothing verified
//! that each knob message's arm in `process_action` sends the *matching*
//! daemon RPC — the "budget vs context_budget" miswiring class that the
//! cross-layer checklist in AGENTS.md guards. This matrix drives every
//! daemon-scoped knob end-to-end: real keystrokes (`:set …` + Enter) through
//! `OilChatApp::update`, then the resulting action through the real
//! `process_action`, and asserts on the requests that reach a fake daemon.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::session::PluginApproval;
use crucible_oil::terminal::Terminal;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use test_case::test_case;

use crate::test_daemon::FakeDaemon;
use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::OilChatRunner;
use crate::tui::oil::event::Event;

/// A daemon that holds plugin approvals, and answers the mode list with
/// `modes` and the current mode `ask`. `refuse_modes` makes every
/// `session.set_mode` fail.
async fn knob_daemon(
    approvals: BTreeMap<String, PluginApproval>,
    modes: &[&str],
    refuse_modes: bool,
) -> FakeDaemon {
    let approvals = Arc::new(Mutex::new(approvals));
    let modes = serde_json::to_value(crate::tui::oil::chat_app::state::mode_descriptors(modes))
        .expect("modes serialize");
    FakeDaemon::start("chat-1", move |method, params| match method {
        "session.set_plugin_approval" => {
            let plugin = params["plugin"].as_str().unwrap_or_default().to_string();
            let approval: PluginApproval =
                serde_json::from_value(params["approval"].clone()).map_err(|e| e.to_string())?;
            approvals.lock().unwrap().insert(plugin, approval);
            Ok(Value::Null)
        }
        "session.list_plugin_approvals" => Ok(json!({ "approvals": *approvals.lock().unwrap() })),
        "session.list_modes" => Ok(json!({ "current_mode_id": "ask", "modes": modes })),
        "session.set_mode" if refuse_modes => Err(format!("unknown mode '{}'", params["mode_id"])),
        _ => Ok(Value::Null),
    })
    .await
}

/// The requests that change session state, in order. Reads are left out.
fn setters(daemon: &FakeDaemon) -> Vec<String> {
    daemon
        .methods()
        .into_iter()
        .filter(|m| !m.starts_with("session.list_"))
        .collect()
}

/// Type a line one `Char` at a time (driving the real input/autocomplete
/// path) and press Enter, returning the submit action.
fn type_and_submit(app: &mut OilChatApp, line: &str) -> Action<ChatAppMsg> {
    for c in line.chars() {
        app.update(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )));
    }
    app.update(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
}

/// Run an action through the real `process_action` against `daemon`.
async fn run(app: &mut OilChatApp, daemon: &FakeDaemon, action: Action<ChatAppMsg>) {
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    runner
        .process_action_for_test(action, app, Some(&daemon.session))
        .await
        .expect("process_action should not fail");
}

#[test_case("model=gpt-4o", "session.switch_model" ; "model")]
#[test_case("contextstrategy=summarize", "session.set_context_strategy" ; "context strategy")]
#[test_case("precognition=off", "session.set_precognition" ; "precognition")]
#[test_case("plugin_turn_limit=7", "session.set_plugin_turn_limit" ; "plugin turn limit")]
#[test_case("plugin_approval.goal=ask", "session.set_plugin_approval" ; "plugin approval")]
#[tokio::test]
async fn interactive_set_knob_reaches_matching_rpc(body: &str, expected_rpc: &str) {
    let mut app = OilChatApp::default();
    let action = type_and_submit(&mut app, &format!(":set {body}"));
    assert!(
        matches!(action, Action::Send(_)),
        ":set {body} typed interactively must submit a daemon-sync action, got Continue/Quit"
    );
    let daemon = knob_daemon(BTreeMap::new(), &["ask"], false).await;
    run(&mut app, &daemon, action).await;
    assert_eq!(
        setters(&daemon),
        vec![expected_rpc.to_string()],
        ":set {body} must invoke exactly the {expected_rpc} RPC once"
    );
}

/// `:set plugin_approval.<plugin>` writes to the daemon and reads the value
/// back from it, so the answer is what the daemon holds: after a resume,
/// after a change by another client, and after this set.
#[tokio::test]
async fn plugin_approval_is_set_and_read_through_the_daemon() {
    let mut app = OilChatApp::default();
    let daemon = knob_daemon(
        BTreeMap::from([("goal".to_string(), PluginApproval::Stop)]),
        &["ask"],
        false,
    )
    .await;

    let query = type_and_submit(&mut app, ":set plugin_approval.goal?");
    run(&mut app, &daemon, query).await;
    let set = type_and_submit(&mut app, ":set plugin_approval.goal=ask");
    run(&mut app, &daemon, set).await;

    assert_eq!(setters(&daemon), ["session.set_plugin_approval"]);
    let screen = crate::tui::oil::tests::helpers::vt_render(&mut app);
    assert!(screen.contains("plugin_approval.goal=stop"), "{screen}");
    assert!(screen.contains("plugin_approval.goal=ask"), "{screen}");
}

/// `:plugin-mode` is the engine command of decision 10. It asks the runner
/// for the daemon's list, opens a menu of each plugin with its three
/// values, and the chosen row sets the knob in the daemon.
#[tokio::test]
async fn the_plugin_menu_sets_the_approval_in_the_daemon() {
    let mut app = OilChatApp::default();
    let fetch = type_and_submit(&mut app, ":plugin-mode");
    assert!(
        matches!(fetch, Action::Send(ChatAppMsg::FetchPluginApprovals)),
        "the menu reads the daemon's list, got {fetch:?}"
    );
    app.on_message(ChatAppMsg::PluginApprovalsLoaded(vec![
        ("goal".into(), PluginApproval::Ask),
        ("sync".into(), PluginApproval::Inherit),
    ]));
    let rows: Vec<_> = app
        .get_popup_items()
        .iter()
        .map(|item| (item.label.clone(), item.description.clone()))
        .collect();
    let current = |label: &str, now: bool| (label.to_string(), now.then(|| "current".to_string()));
    assert_eq!(
        rows,
        [
            current("goal · inherit", false),
            current("goal · ask", true),
            current("goal · stop", false),
            current("sync · inherit", true),
            current("sync · ask", false),
            current("sync · stop", false),
        ]
    );
    for _ in 0..2 {
        app.update(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
    }
    let choose = app.update(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    let daemon = knob_daemon(BTreeMap::new(), &["ask"], false).await;
    run(&mut app, &daemon, choose).await;
    let calls = daemon.calls();
    let sets: Vec<_> = calls
        .iter()
        .filter(|(m, _)| m == "session.set_plugin_approval")
        .collect();
    assert_eq!(sets.len(), 1, "{calls:?}");
    assert_eq!(sets[0].1["plugin"], "goal");
    assert_eq!(sets[0].1["approval"], "stop");
    assert!(!app.panel_popup_is_open(), "the menu closes after a choice");
}

/// A daemon with no plugin loaded answers an empty list. The menu says so
/// instead of opening empty.
#[test]
fn an_empty_plugin_list_says_that_no_plugin_is_loaded() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::PluginApprovalsLoaded(Vec::new()));
    assert!(!app.panel_popup_is_open());
    let screen = crate::tui::oil::tests::helpers::vt_render(&mut app);
    assert!(screen.contains("No plugins loaded"), "{screen}");
}

/// The status picker opens the same menu for the plugin-turn item.
#[test]
fn the_plugin_turn_item_opens_the_menu_from_the_status_picker() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::StatusItemsLoaded(vec![
        crucible_core::types::StatusDisplayItem {
            id: "plugin_turns:goal".into(),
            text: "goal · ask".into(),
            priority: 0,
            color_group: crucible_core::status_color::StatusColorGroup::from_name("warn"),
            action: Some("plugin_approval".into()),
            pinned: true,
            plugin: "goal".into(),
            kind: crucible_core::types::StatusItemKind::PluginTurns,
            progress: None,
        },
    ]));
    type_and_submit(&mut app, ":status");
    let open = app.update(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    assert!(
        matches!(open, Action::Send(ChatAppMsg::FetchPluginApprovals)),
        "got {open:?}"
    );
}

/// A value outside the three is refused before any call.
#[test]
fn an_unknown_plugin_approval_is_refused() {
    let mut app = OilChatApp::default();
    let action = type_and_submit(&mut app, ":set plugin_approval.goal=maybe");
    assert!(matches!(action, Action::Continue), "{action:?}");
}

/// A mode the daemon refuses must not leave the badge claiming it.
///
/// The badge is set optimistically by `set_mode_with_status` before the RPC is
/// made. The failure path used to be a `tracing::warn!` and nothing else, so
/// the statusline read PLAN while the agent stayed in normal — the same
/// "the UI says one thing, the agent does another" defect this area exists to
/// prevent.
#[tokio::test]
async fn a_rejected_mode_change_reverts_the_badge_and_surfaces_the_error() {
    let mut app = OilChatApp::default();
    let daemon = knob_daemon(BTreeMap::new(), &["ask", "plan"], true).await;

    app.on_message(ChatAppMsg::ModeChanged("plan".into()));
    assert_eq!(app.mode(), "plan", "optimistic update happens first");

    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    let queued = runner
        .process_action_collecting_msgs(
            Action::Send(ChatAppMsg::ModeChanged("plan".into())),
            &mut app,
            Some(&daemon.session),
        )
        .await;
    // The event loop drains the queue; do the same so the assertions below
    // describe what the user actually ends up looking at.
    for msg in queued {
        app.on_message(msg);
    }

    assert_eq!(
        app.mode(),
        "ask",
        "a refused mode must revert to what the daemon reports"
    );
    assert!(
        app.has_notifications(),
        "and the user must be told why, not just the log"
    );
}

/// The startup chain, end to end: `FetchModes` → `session.list_modes` →
/// `ModesLoaded` → the app's list.
///
/// Nothing covered this. Every other mode test hand-feeds `ModesLoaded`, so
/// deleting the `FetchModes` send or inverting the non-empty guard left the
/// whole suite green while the TUI silently ran on its built-in fallback.
#[tokio::test]
async fn fetch_modes_reaches_the_app_from_the_daemon() {
    let mut app = OilChatApp::default();
    let daemon = knob_daemon(BTreeMap::new(), &["ask", "review"], false).await;

    run(&mut app, &daemon, Action::Send(ChatAppMsg::FetchModes)).await;

    assert_eq!(
        daemon.methods(),
        ["session.list_modes"],
        "the daemon must be asked"
    );
    assert!(
        app.knows_mode("review"),
        "a mode only the daemon knew about must have reached the app's list"
    );
    assert!(
        !app.knows_mode("plan"),
        "and the daemon's list must REPLACE the built-in fallback, not extend it"
    );
}

/// A mode declared after startup is picked up. `FetchModes` fires once, so the
/// only way the list can catch up is a drift signal — here, the daemon naming
/// a mode we do not have.
#[tokio::test]
async fn a_mode_declared_after_startup_is_picked_up() {
    let mut app = OilChatApp::default();
    let daemon = knob_daemon(BTreeMap::new(), &["ask", "review"], false).await;
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));

    // The app still has only its built-in fallback list.
    assert!(!app.knows_mode("review"));

    let queued = runner
        .process_action_collecting_msgs(
            Action::Send(ChatAppMsg::ModeSynced("review".into())),
            &mut app,
            Some(&daemon.session),
        )
        .await;

    assert!(
        queued.iter().any(|m| matches!(m, ChatAppMsg::FetchModes)),
        "a mode we have never heard of must trigger a refresh, got {queued:?}"
    );
}
