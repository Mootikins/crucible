//! Per-knob RPC arm verification for interactive `:set`.
//!
//! The `:set` dispatch matrix (chat_app/command_handling.rs) stops at
//! `Action::Send(msg)`, and the startup-override regression test
//! (initial_sets.rs) covers only context_strategy + model. Nothing verified
//! that each knob message's arm in `process_action` invokes the *matching*
//! `AgentHandle` RPC — the "budget vs context_budget" miswiring class from
//! the AGENTS.md cross-layer checklist. This matrix drives every
//! daemon-scoped knob end-to-end: real keystrokes (`:set …` + Enter) through
//! `OilChatApp::update`, then the resulting action through the real
//! `process_action`, asserting exactly the matching RPC fired.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::events::EventRing;
use crucible_core::session::PluginApproval;
use crucible_core::traits::chat::{AgentHandle, ChatError, ChatResult, SessionKnobs};
use crucible_oil::terminal::Terminal;
use std::sync::Arc;
use test_case::test_case;

use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::OilChatRunner;
use crate::tui::oil::event::Event;

/// Records the name of every knob RPC invoked, in call order. Equality
/// assertions on `calls` catch both a miswired arm (wrong name recorded)
/// and duplicate dispatch (extra entries).
#[derive(Default)]
pub(super) struct KnobRecordingAgent {
    pub(super) calls: Vec<&'static str>,
    /// What the daemon holds for each plugin, as a resumed handle reads it.
    pub(super) approvals: std::collections::BTreeMap<String, PluginApproval>,
}

crucible_core::impl_noop_agent!(KnobRecordingAgent);

#[async_trait::async_trait]
impl AgentHandle for KnobRecordingAgent {
    async fn send_message_fire_and_forget(&mut self, _message: String) -> ChatResult<()> {
        Ok(())
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        self.calls.push("set_mode_str");
        Ok(())
    }
}

/// Every setter the matrix drives records its name. The rest is the empty
/// answer, written out so the compiler sees the choice.
#[async_trait::async_trait]
impl SessionKnobs for KnobRecordingAgent {
    async fn set_plugin_approval(
        &mut self,
        plugin: &str,
        approval: PluginApproval,
    ) -> ChatResult<()> {
        self.calls.push("set_plugin_approval");
        self.approvals.insert(plugin.into(), approval);
        Ok(())
    }
    fn get_plugin_approval(&self, plugin: &str) -> PluginApproval {
        self.approvals.get(plugin).copied().unwrap_or_default()
    }
    async fn set_plugin_turn_limit(&mut self, _limit: u32) -> ChatResult<()> {
        self.calls.push("set_plugin_turn_limit");
        Ok(())
    }
    fn get_plugin_turn_limit(&self) -> u32 {
        25
    }
    fn get_system_prompt(&self) -> Option<String> {
        None
    }

    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        self.calls.push("switch_model");
        Ok(())
    }
    async fn set_context_strategy(
        &mut self,
        _strategy: crucible_core::session::ContextStrategy,
    ) -> ChatResult<()> {
        self.calls.push("set_context_strategy");
        Ok(())
    }
    async fn set_precognition(&mut self, _enabled: bool) -> ChatResult<()> {
        self.calls.push("set_precognition");
        Ok(())
    }
    fn current_model(&self) -> Option<&str> {
        None
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        Vec::new()
    }

    async fn fetch_available_modes(&mut self) -> Vec<crucible_core::types::mode::ModeDescriptor> {
        Vec::new()
    }

    fn get_context_strategy(&self) -> crucible_core::session::ContextStrategy {
        crucible_core::session::ContextStrategy::default()
    }

    fn get_precognition(&self) -> bool {
        true
    }
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

/// Run an action through the real `process_action` and return the recorded
/// RPC call sequence.
async fn record_rpc_calls(app: &mut OilChatApp, action: Action<ChatAppMsg>) -> Vec<&'static str> {
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    let mut agent = KnobRecordingAgent::default();
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    runner
        .process_action_for_test(action, app, &mut agent, &bridge)
        .await
        .expect("process_action should not fail");
    agent.calls
}

#[test_case("model=gpt-4o", "switch_model" ; "model")]
#[test_case("contextstrategy=summarize", "set_context_strategy" ; "context strategy")]
#[test_case("precognition=off", "set_precognition" ; "precognition")]
#[test_case("plugin_turn_limit=7", "set_plugin_turn_limit" ; "plugin turn limit")]
#[test_case("plugin_approval.goal=ask", "set_plugin_approval" ; "plugin approval")]
#[tokio::test]
async fn interactive_set_knob_reaches_matching_rpc(body: &str, expected_rpc: &str) {
    let mut app = OilChatApp::default();
    let action = type_and_submit(&mut app, &format!(":set {body}"));
    assert!(
        matches!(action, Action::Send(_)),
        ":set {body} typed interactively must submit a daemon-sync action, got Continue/Quit"
    );
    let calls = record_rpc_calls(&mut app, action).await;
    assert_eq!(
        calls,
        vec![expected_rpc],
        ":set {body} must invoke exactly the {expected_rpc} RPC once"
    );
}

/// `:set plugin_approval.<plugin>` writes through the handle and reads the
/// value back from it, so the answer is what the daemon holds: after a
/// resume, after a change by another client, and after this set.
#[tokio::test]
async fn plugin_approval_is_set_and_read_through_the_handle() {
    let mut app = OilChatApp::default();
    let mut agent = KnobRecordingAgent::default();
    agent.approvals.insert("goal".into(), PluginApproval::Stop);
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));

    let query = type_and_submit(&mut app, ":set plugin_approval.goal?");
    runner
        .process_action_for_test(query, &mut app, &mut agent, &bridge)
        .await
        .unwrap();
    let set = type_and_submit(&mut app, ":set plugin_approval.goal=ask");
    runner
        .process_action_for_test(set, &mut app, &mut agent, &bridge)
        .await
        .unwrap();

    assert_eq!(agent.approvals["goal"], PluginApproval::Ask);
    let screen = crate::tui::oil::tests::helpers::vt_render(&mut app);
    assert!(screen.contains("plugin_approval.goal=stop"), "{screen}");
    assert!(screen.contains("plugin_approval.goal=ask"), "{screen}");
}

/// `:plugin-mode` is the engine command of decision 10. It asks the runner
/// for the daemon's list, opens a menu of each plugin with its three
/// values, and the chosen row sets the knob through the handle.
#[tokio::test]
async fn the_plugin_menu_sets_the_approval_through_the_handle() {
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
    let mut agent = KnobRecordingAgent::default();
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    runner
        .process_action_for_test(choose, &mut app, &mut agent, &bridge)
        .await
        .unwrap();
    assert_eq!(agent.calls, ["set_plugin_approval"]);
    assert_eq!(agent.approvals["goal"], PluginApproval::Stop);
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

/// A handle that refuses every mode change, reporting the one it is really in.
struct ModeRejectingAgent;

crucible_core::impl_noop_agent!(ModeRejectingAgent);

crucible_core::impl_unsupported_session_knobs!(ModeRejectingAgent);

#[async_trait::async_trait]
impl AgentHandle for ModeRejectingAgent {
    async fn send_message_fire_and_forget(&mut self, _message: String) -> ChatResult<()> {
        Ok(())
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        Err(crucible_core::traits::chat::ChatError::ModeChange(format!(
            "unknown mode '{mode_id}'"
        )))
    }
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
    let mut agent = ModeRejectingAgent;
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));

    app.on_message(ChatAppMsg::ModeChanged("plan".into()));
    assert_eq!(app.mode(), "plan", "optimistic update happens first");

    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    let queued = runner
        .process_action_collecting_msgs(
            Action::Send(ChatAppMsg::ModeChanged("plan".into())),
            &mut app,
            &mut agent,
            &bridge,
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
        "a refused mode must revert to what the handle reports"
    );
    assert!(
        app.has_notifications(),
        "and the user must be told why, not just the log"
    );
}

/// A handle whose declared mode list can change between fetches.
struct ModeListingAgent {
    modes: Vec<String>,
    fetches: std::sync::Arc<std::sync::Mutex<u32>>,
    mode: String,
}

crucible_core::impl_noop_agent!(ModeListingAgent);

#[async_trait::async_trait]
impl AgentHandle for ModeListingAgent {
    async fn send_message_fire_and_forget(&mut self, _message: String) -> ChatResult<()> {
        Ok(())
    }
    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        &self.mode
    }
    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        if !self.modes.iter().any(|m| m == mode_id) {
            return Err(crucible_core::traits::chat::ChatError::ModeChange(format!(
                "unknown mode '{mode_id}'"
            )));
        }
        self.mode = mode_id.to_string();
        Ok(())
    }
}

/// Only the mode list is live; every knob is the empty answer.
#[async_trait::async_trait]
impl SessionKnobs for ModeListingAgent {
    async fn set_plugin_approval(
        &mut self,
        _plugin: &str,
        _approval: PluginApproval,
    ) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_plugin_approval".into()))
    }
    fn get_plugin_approval(&self, _plugin: &str) -> PluginApproval {
        PluginApproval::Inherit
    }
    async fn set_plugin_turn_limit(&mut self, _limit: u32) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_plugin_turn_limit".into()))
    }
    fn get_plugin_turn_limit(&self) -> u32 {
        25
    }
    fn get_system_prompt(&self) -> Option<String> {
        None
    }

    async fn fetch_available_modes(&mut self) -> Vec<crucible_core::types::mode::ModeDescriptor> {
        *self.fetches.lock().unwrap() += 1;
        let ids: Vec<&str> = self.modes.iter().map(String::as_str).collect();
        crate::tui::oil::chat_app::state::mode_descriptors(&ids)
    }

    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        Err(ChatError::NotSupported("switch_model".into()))
    }

    fn current_model(&self) -> Option<&str> {
        None
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        Vec::new()
    }

    async fn set_context_strategy(
        &mut self,
        _strategy: crucible_core::session::ContextStrategy,
    ) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_context_strategy".into()))
    }

    fn get_context_strategy(&self) -> crucible_core::session::ContextStrategy {
        crucible_core::session::ContextStrategy::default()
    }

    async fn set_precognition(&mut self, _enabled: bool) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_precognition".into()))
    }

    fn get_precognition(&self) -> bool {
        true
    }
}

/// The startup chain, end to end: `FetchModes` → `fetch_available_modes` →
/// `ModesLoaded` → the app's list.
///
/// Nothing covered this. Every other mode test hand-feeds `ModesLoaded`, so
/// deleting the `FetchModes` send or inverting the non-empty guard left the
/// whole suite green while the TUI silently ran on its built-in fallback.
#[tokio::test]
async fn fetch_modes_reaches_the_app_through_the_agent() {
    let mut app = OilChatApp::default();
    let fetches = std::sync::Arc::new(std::sync::Mutex::new(0));
    let mut agent = ModeListingAgent {
        modes: vec!["ask".to_string(), "review".to_string()],
        fetches: fetches.clone(),
        mode: "ask".to_string(),
    };
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));

    runner
        .process_action_for_test(
            Action::Send(ChatAppMsg::FetchModes),
            &mut app,
            &mut agent,
            &bridge,
        )
        .await
        .expect("process_action should not fail");

    assert_eq!(*fetches.lock().unwrap(), 1, "the agent must be asked");
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
    let fetches = std::sync::Arc::new(std::sync::Mutex::new(0));
    let mut agent = ModeListingAgent {
        modes: vec!["ask".to_string(), "review".to_string()],
        fetches: fetches.clone(),
        mode: "ask".to_string(),
    };
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));

    // The app still has only its built-in fallback list.
    assert!(!app.knows_mode("review"));

    let queued = runner
        .process_action_collecting_msgs(
            Action::Send(ChatAppMsg::ModeSynced("review".into())),
            &mut app,
            &mut agent,
            &bridge,
        )
        .await;

    assert!(
        queued.iter().any(|m| matches!(m, ChatAppMsg::FetchModes)),
        "a mode we have never heard of must trigger a refresh, got {queued:?}"
    );
}
