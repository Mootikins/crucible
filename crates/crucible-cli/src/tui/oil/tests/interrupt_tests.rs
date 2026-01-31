use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn ctrl_enter() -> KeyEvent {
    KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL)
}

fn view_with_default_ctx(app: &OilChatApp) -> crate::tui::oil::node::Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

#[test]
fn ctrl_c_during_streaming_sends_stream_cancelled() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Thinking...".to_string()));

    assert!(app.is_streaming(), "Should be streaming");

    let action = app.update(Event::Key(ctrl('c')));

    assert!(
        matches!(action, Action::Send(ChatAppMsg::StreamCancelled)),
        "Ctrl+C during streaming should send StreamCancelled, got {:?}",
        action
    );
}

#[test]
fn escape_during_streaming_sends_stream_cancelled() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Working...".to_string()));

    assert!(app.is_streaming(), "Should be streaming");

    let action = app.update(Event::Key(key(KeyCode::Esc)));

    assert!(
        matches!(action, Action::Send(ChatAppMsg::StreamCancelled)),
        "Escape during streaming should send StreamCancelled, got {:?}",
        action
    );
}

#[test]
fn enter_during_streaming_queues_message() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("First question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Responding...".to_string()));

    assert!(app.is_streaming(), "Should be streaming");

    for c in "follow up".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    let action = app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        matches!(action, Action::Send(ChatAppMsg::QueueMessage(ref msg)) if msg == "follow up"),
        "Enter during streaming should queue message, got {:?}",
        action
    );
    assert!(app.input_content().is_empty(), "Input should be cleared");
}

#[test]
fn empty_enter_during_streaming_does_nothing() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Answer...".to_string()));

    assert!(app.is_streaming(), "Should be streaming");
    assert!(app.input_content().is_empty(), "Input should be empty");

    let action = app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        matches!(action, Action::Continue),
        "Empty Enter during streaming should continue, got {:?}",
        action
    );
}

#[test]
fn stream_cancelled_message_clears_streaming() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Partial response...".to_string()));

    assert!(app.is_streaming(), "Should be streaming");

    app.on_message(ChatAppMsg::StreamCancelled);

    assert!(!app.is_streaming(), "Should not be streaming after cancel");
}

#[test]
fn queued_message_processed_after_stream_complete() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("first".to_string()));
    app.on_message(ChatAppMsg::QueueMessage("queued question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("response".to_string()));

    let action = app.on_message(ChatAppMsg::StreamComplete);

    assert!(
        matches!(action, Action::Send(ChatAppMsg::UserMessage(ref msg)) if msg == "queued question"),
        "StreamComplete should trigger queued message, got {:?}",
        action
    );
}

#[test]
fn queued_message_processed_after_stream_cancelled() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("first".to_string()));
    app.on_message(ChatAppMsg::QueueMessage("queued question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("partial".to_string()));

    let action = app.on_message(ChatAppMsg::StreamCancelled);

    assert!(
        matches!(action, Action::Send(ChatAppMsg::UserMessage(ref msg)) if msg == "queued question"),
        "StreamCancelled should trigger queued message, got {:?}",
        action
    );
}

#[test]
fn multiple_queued_messages_processed_in_order() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("initial".to_string()));
    app.on_message(ChatAppMsg::QueueMessage("first queued".to_string()));
    app.on_message(ChatAppMsg::QueueMessage("second queued".to_string()));

    let action1 = app.on_message(ChatAppMsg::StreamComplete);
    assert!(
        matches!(action1, Action::Send(ChatAppMsg::UserMessage(ref msg)) if msg == "first queued"),
        "First queue should process first, got {:?}",
        action1
    );

    let action2 = app.on_message(ChatAppMsg::StreamComplete);
    assert!(
        matches!(action2, Action::Send(ChatAppMsg::UserMessage(ref msg)) if msg == "second queued"),
        "Second queue should process second, got {:?}",
        action2
    );

    let action3 = app.on_message(ChatAppMsg::StreamComplete);
    assert!(
        matches!(action3, Action::Continue),
        "Empty queue should return Continue, got {:?}",
        action3
    );
}

#[test]
fn status_shows_queued_count() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("...".to_string()));

    for c in "follow up".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    let action = app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        matches!(action, Action::Send(ChatAppMsg::QueueMessage(ref msg)) if msg == "follow up"),
        "Enter during streaming should queue message, got {:?}",
        action
    );

    if let Action::Send(msg) = action {
        app.on_message(msg);
    }

    let tree = view_with_default_ctx(&app);
    let output = crate::tui::oil::render::render_to_string(&tree, 80);

    assert!(
        output.contains("1 message queued") || output.contains("queued"),
        "Should show queued notification: {}",
        output
    );
}

#[test]
fn typing_during_streaming_works() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("response...".to_string()));

    assert!(app.is_streaming(), "Should be streaming");

    for c in "next question".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert_eq!(
        app.input_content(),
        "next question",
        "Should be able to type during streaming"
    );
}

#[test]
fn ctrl_c_not_streaming_still_clears_input() {
    let mut app = OilChatApp::default();

    for c in "some text".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(!app.is_streaming(), "Should not be streaming");
    assert_eq!(app.input_content(), "some text");

    let action = app.update(Event::Key(ctrl('c')));

    assert!(
        matches!(action, Action::Continue),
        "Ctrl+C with input should continue"
    );
    assert!(
        app.input_content().is_empty(),
        "Input should be cleared by Ctrl+C when not streaming"
    );
}

#[test]
fn escape_not_streaming_no_effect_on_input() {
    let mut app = OilChatApp::default();

    for c in "some text".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(!app.is_streaming(), "Should not be streaming");

    let action = app.update(Event::Key(key(KeyCode::Esc)));

    assert!(
        matches!(action, Action::Continue),
        "Escape when not streaming should continue"
    );
}

#[test]
fn cancelled_status_shows_after_cancel() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("partial...".to_string()));
    app.on_message(ChatAppMsg::StreamCancelled);

    let tree = view_with_default_ctx(&app);
    let output = crate::tui::oil::render::render_to_string(&tree, 80);

    assert!(
        output.contains("Cancelled") || output.contains("cancelled"),
        "Should show cancelled notification: {}",
        output
    );
}

#[test]
fn queue_message_when_idle_promotes_to_user_message() {
    let mut app = OilChatApp::default();

    assert!(!app.is_streaming());

    let action = app.on_message(ChatAppMsg::QueueMessage("hello".to_string()));

    assert!(
        matches!(action, Action::Send(ChatAppMsg::UserMessage(ref msg)) if msg == "hello"),
        "QueueMessage when idle should promote to UserMessage, got {:?}",
        action
    );
}

#[test]
fn queue_message_when_idle_shows_spinner() {
    let mut app = OilChatApp::default();

    assert!(!app.is_streaming());

    let action = app.on_message(ChatAppMsg::QueueMessage("hello".to_string()));

    if let Action::Send(msg) = action {
        app.on_message(msg);
    }

    assert!(
        app.is_streaming(),
        "Should be streaming after promoted QueueMessage"
    );
}
