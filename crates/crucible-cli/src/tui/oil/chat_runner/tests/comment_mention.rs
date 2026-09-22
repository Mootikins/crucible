//! `@comment:<id>` in the chat input reaches the daemon as typed.
//!
//! The TUI has no line-comment UI. A TUI user lists the comments of a
//! diffset with `cru diff comments <diffset> -f json`, and names one in a
//! message with `@comment:<id>`. The daemon finds the comment and gives the
//! agent its context, so the TUI must send the text unchanged: the `@`
//! completion must not take the Enter key, and nothing here may expand or
//! strip the mention.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::events::EventRing;
use crucible_core::traits::chat::{AgentHandle, ChatError, ChatResult};
use crucible_oil::terminal::Terminal;
use std::sync::Arc;

use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::OilChatRunner;
use crate::tui::oil::event::Event;

/// Records every message that the TUI sends to the daemon.
#[derive(Default)]
struct SendRecordingAgent {
    sent: Vec<String>,
}

crucible_core::impl_noop_agent!(SendRecordingAgent);
crucible_core::impl_unsupported_session_knobs!(SendRecordingAgent);

#[async_trait::async_trait]
impl AgentHandle for SendRecordingAgent {
    async fn send_message_fire_and_forget(&mut self, message: String) -> ChatResult<()> {
        self.sent.push(message);
        Ok(())
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

/// Refuses every message, as the daemon does for an unknown comment id.
struct RefusingAgent;

crucible_core::impl_noop_agent!(RefusingAgent);
crucible_core::impl_unsupported_session_knobs!(RefusingAgent);

#[async_trait::async_trait]
impl AgentHandle for RefusingAgent {
    async fn send_message_fire_and_forget(&mut self, _message: String) -> ChatResult<()> {
        Err(ChatError::InvalidInput(
            "no stored comment has the id c-1".into(),
        ))
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

/// Type a line one key at a time, then press Enter, as a user does.
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

#[tokio::test]
async fn a_comment_mention_reaches_the_daemon_as_typed() {
    const LINE: &str = "@comment:8b1f5c2e-0000-4000-8000-000000000001 why this?";

    let mut app = OilChatApp::default();
    let action = type_and_submit(&mut app, LINE);

    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    let mut agent = SendRecordingAgent::default();
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    runner
        .process_action_for_test(action, &mut app, &mut agent, &bridge)
        .await
        .expect("process_action should not fail");

    assert_eq!(
        agent.sent,
        vec![LINE.to_string()],
        "the daemon resolves the mention, so the TUI sends the text as typed"
    );
}

#[tokio::test]
async fn a_refused_mention_tells_the_user() {
    let mut app = OilChatApp::default();
    let action = type_and_submit(&mut app, "@comment:c-1 why this?");

    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    let queued = runner
        .process_action_collecting_msgs(action, &mut app, &mut RefusingAgent, &bridge)
        .await;

    let errors: Vec<String> = queued
        .into_iter()
        .filter_map(|msg| match msg {
            ChatAppMsg::Error(text) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "a refusal is not a log line only");
    assert!(
        errors[0].contains("no stored comment has the id c-1"),
        "the reason of the daemon must reach the user: {}",
        errors[0]
    );
}
