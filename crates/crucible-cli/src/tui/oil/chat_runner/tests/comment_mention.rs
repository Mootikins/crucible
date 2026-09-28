//! `@comment:<id>` in the chat input reaches the daemon as typed.
//!
//! The TUI has no line-comment UI. A TUI user lists the comments of a
//! diffset with `cru diff comments <diffset> -f json`, and names one in a
//! message with `@comment:<id>`. The daemon finds the comment and gives the
//! agent its context, so the TUI must send the text unchanged: the `@`
//! completion must not take the Enter key, and nothing here may expand or
//! strip the mention.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_oil::terminal::Terminal;

use crate::test_daemon::FakeDaemon;
use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::OilChatRunner;
use crate::tui::oil::event::Event;

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
    let daemon = FakeDaemon::start("chat-1", |_, _| {
        Ok(serde_json::json!({"message_id": "m-1"}))
    })
    .await;
    runner
        .process_action_for_test(action, &mut app, Some(&daemon.session))
        .await
        .expect("process_action should not fail");

    let sent: Vec<String> = daemon
        .calls()
        .into_iter()
        .filter(|(method, _)| method == "session.send_message")
        .map(|(_, params)| params["content"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        sent,
        vec![LINE.to_string()],
        "the daemon resolves the mention, so the TUI sends the text as typed"
    );
}

#[tokio::test]
async fn a_refused_mention_tells_the_user() {
    let mut app = OilChatApp::default();
    let action = type_and_submit(&mut app, "@comment:c-1 why this?");

    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    // The daemon refuses the message, as it does for an unknown comment id.
    let daemon = FakeDaemon::start("chat-1", |method, _| match method {
        "session.send_message" => Err("no stored comment has the id c-1".to_string()),
        _ => Ok(serde_json::Value::Null),
    })
    .await;
    let queued = runner
        .process_action_collecting_msgs(action, &mut app, Some(&daemon.session))
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
