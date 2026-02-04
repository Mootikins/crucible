use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::node::Node;
use crate::tui::oil::terminal::Terminal;
use crossterm::event::EventStream;
use futures::StreamExt;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

fn view_with_default_ctx(app: &OilChatApp) -> Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

#[tokio::test]
async fn event_loop_does_not_freeze_without_input() {
    let mut terminal = Terminal::with_size(80, 24);
    let mut app = OilChatApp::default();
    let (_msg_tx, mut msg_rx) = mpsc::unbounded_channel::<ChatAppMsg>();

    let start = Instant::now();
    let iterations = 100;

    for i in 0..iterations {
        let tree = view_with_default_ctx(&app);
        terminal.render(&tree).unwrap();

        while let Ok(msg) = msg_rx.try_recv() {
            let _ = app.on_message(msg);
        }

        let _ = app.update(Event::Tick);

        if start.elapsed() > Duration::from_secs(5) {
            panic!("Loop froze after {} iterations in {:?}", i, start.elapsed());
        }
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "100 iterations took {:?}, expected < 2s",
        elapsed
    );
}

#[tokio::test]
async fn render_loop_completes_many_iterations() {
    let mut terminal = Terminal::with_size(80, 24);
    let mut app = OilChatApp::default();

    let start = Instant::now();

    for _ in 0..500 {
        let tree = view_with_default_ctx(&app);
        terminal.render(&tree).unwrap();
        let _ = app.update(Event::Tick);
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "500 render iterations took {:?}",
        elapsed
    );
}

/// Verify render time doesn't grow unbounded with message history.
/// 100 iterations × 20 messages = 2000 markdown parses.
/// 10s threshold allows ~5ms per parse which is reasonable without caching.
#[tokio::test]
async fn render_with_messages_does_not_accumulate() {
    let mut terminal = Terminal::with_size(80, 24);
    let mut app = OilChatApp::default();

    for i in 0..20 {
        app.on_message(ChatAppMsg::UserMessage(format!("Message {}", i)));
        app.on_message(ChatAppMsg::TextDelta(format!("Response chunk {} ", i)));
    }
    app.on_message(ChatAppMsg::StreamComplete);

    let start = Instant::now();

    for _ in 0..100 {
        let tree = view_with_default_ctx(&app);
        terminal.render(&tree).unwrap();
        let _ = app.update(Event::Tick);
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "100 iterations with 20 messages took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn event_stream_with_timeout_does_not_block() {
    if crossterm::terminal::enable_raw_mode().is_err() {
        eprintln!("Skipping test: no TTY available");
        return;
    }

    let mut event_stream = EventStream::new();
    let mut tick_interval = tokio::time::interval(Duration::from_millis(50));

    let start = Instant::now();
    let mut tick_count = 0;

    for _ in 0..20 {
        tokio::select! {
            biased;

            Some(_event) = event_stream.next() => {}

            _ = tick_interval.tick() => {
                tick_count += 1;
            }
        }

        if start.elapsed() > Duration::from_secs(5) {
            crossterm::terminal::disable_raw_mode().ok();
            panic!(
                "Event loop blocked after {} ticks in {:?}",
                tick_count,
                start.elapsed()
            );
        }
    }

    crossterm::terminal::disable_raw_mode().ok();

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(3),
        "20 select iterations took {:?}, tick_count={}",
        elapsed,
        tick_count
    );
    assert!(
        tick_count >= 15,
        "Expected at least 15 ticks, got {}",
        tick_count
    );
}

#[tokio::test]
async fn escape_key_closes_popup() {
    use crate::tui::oil::app::{Action, App};
    use crate::tui::oil::chat_app::OilChatApp;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = OilChatApp::default();

    // ESC should continue (used for closing popup), not quit
    let esc_event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let action = app.update(esc_event);

    assert!(
        matches!(action, Action::Continue),
        "ESC should produce Action::Continue (closes popup), got {:?}",
        action
    );
}

#[tokio::test]
async fn double_ctrl_c_triggers_quit_action() {
    use crate::tui::oil::app::{Action, App};
    use crate::tui::oil::chat_app::OilChatApp;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = OilChatApp::default();

    // First Ctrl+C shows notification
    let ctrl_c_event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    let action = app.update(ctrl_c_event.clone());
    assert!(
        matches!(action, Action::Continue),
        "First Ctrl+C should show notification, got {:?}",
        action
    );

    let action = app.update(ctrl_c_event);
    assert!(
        matches!(action, Action::Quit),
        "Second Ctrl+C should produce Action::Quit, got {:?}",
        action
    );
}

#[tokio::test]
async fn ctrl_c_clears_input_first() {
    use crate::tui::oil::app::{Action, App};
    use crate::tui::oil::chat_app::OilChatApp;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = OilChatApp::default();

    // Type some input
    let key_event = Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    app.update(key_event);

    // Ctrl+C clears input
    let ctrl_c_event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    let action = app.update(ctrl_c_event);

    assert!(
        matches!(action, Action::Continue),
        "Ctrl+C with text should clear input, got {:?}",
        action
    );
}
