//! Per-knob RPC arm verification for interactive `:set`.
//!
//! The `:set` dispatch matrix (chat_app/command_handling.rs) stops at
//! `Action::Send(msg)`, and the startup-override regression test
//! (initial_sets.rs) covers only thinking_budget + model. Nothing verified
//! that each knob message's arm in `process_action` invokes the *matching*
//! `AgentHandle` RPC — the "budget vs thinking_budget" miswiring class from
//! the AGENTS.md cross-layer checklist. This matrix drives every
//! daemon-scoped knob end-to-end: real keystrokes (`:set …` + Enter) through
//! `OilChatApp::update`, then the resulting action through the real
//! `process_action`, asserting exactly the matching RPC fired.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::events::EventRing;
use crucible_core::traits::chat::{AgentHandle, ChatResult};
use crucible_oil::terminal::Terminal;
use std::sync::Arc;
use test_case::test_case;

use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::app::{Action, App};
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

    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        self.calls.push("set_mode_str");
        Ok(())
    }

    async fn switch_model(&mut self, _model_id: &str) -> ChatResult<()> {
        self.calls.push("switch_model");
        Ok(())
    }

    async fn set_thinking_budget(&mut self, _budget: i64) -> ChatResult<()> {
        self.calls.push("set_thinking_budget");
        Ok(())
    }

    async fn set_max_iterations(&mut self, _max_iterations: Option<u32>) -> ChatResult<()> {
        self.calls.push("set_max_iterations");
        Ok(())
    }

    async fn set_execution_timeout(&mut self, _timeout_secs: Option<u64>) -> ChatResult<()> {
        self.calls.push("set_execution_timeout");
        Ok(())
    }

    async fn set_context_budget(&mut self, _budget: Option<usize>) -> ChatResult<()> {
        self.calls.push("set_context_budget");
        Ok(())
    }

    async fn set_context_strategy(
        &mut self,
        _strategy: crucible_core::session::ContextStrategy,
    ) -> ChatResult<()> {
        self.calls.push("set_context_strategy");
        Ok(())
    }

    async fn set_context_window(&mut self, _window: Option<usize>) -> ChatResult<()> {
        self.calls.push("set_context_window");
        Ok(())
    }

    async fn set_output_validation(
        &mut self,
        _validation: crucible_core::session::OutputValidation,
    ) -> ChatResult<()> {
        self.calls.push("set_output_validation");
        Ok(())
    }

    async fn set_validation_retries(&mut self, _retries: u32) -> ChatResult<()> {
        self.calls.push("set_validation_retries");
        Ok(())
    }

    async fn set_precognition_results(&mut self, _count: usize) -> ChatResult<()> {
        self.calls.push("set_precognition_results");
        Ok(())
    }

    async fn set_autocompact_threshold(&mut self, _threshold: Option<f32>) -> ChatResult<()> {
        self.calls.push("set_autocompact_threshold");
        Ok(())
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
#[test_case("thinkingbudget=high", "set_thinking_budget" ; "thinking budget")]
#[test_case("maxiterations=5", "set_max_iterations" ; "max iterations")]
#[test_case("executiontimeout=30", "set_execution_timeout" ; "execution timeout")]
#[test_case("contextbudget=128000", "set_context_budget" ; "context budget")]
#[test_case("contextstrategy=sliding_window", "set_context_strategy" ; "context strategy")]
#[test_case("contextwindow=20", "set_context_window" ; "context window")]
#[test_case("outputvalidation=json", "set_output_validation" ; "output validation")]
#[test_case("validationretries=2", "set_validation_retries" ; "validation retries")]
#[test_case("precognition.results=8", "set_precognition_results" ; "precognition results")]
#[test_case("autocompact_threshold=0.8", "set_autocompact_threshold" ; "autocompact threshold")]
#[tokio::test]
async fn interactive_set_knob_reaches_matching_rpc(body: &str, expected_rpc: &str) {
    let mut app = OilChatApp::init();
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
