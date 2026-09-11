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

    async fn fetch_available_modes(&mut self) -> Vec<String> {
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
#[test_case("contextstrategy=sliding_window", "set_context_strategy" ; "context strategy")]
#[test_case("precognition=off", "set_precognition" ; "precognition")]
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
    fn get_system_prompt(&self) -> Option<String> {
        None
    }

    async fn fetch_available_modes(&mut self) -> Vec<String> {
        *self.fetches.lock().unwrap() += 1;
        self.modes.clone()
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
