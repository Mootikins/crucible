use crate::tui::ink::app::App;
use crate::tui::ink::chat_app::{ChatAppMsg, InkChatApp};
use crate::tui::ink::event::Event;
use crate::tui::ink::terminal::Terminal;
use crossterm::event::EventStream;
use futures::StreamExt;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[tokio::test]
async fn event_loop_does_not_freeze_without_input() {
    let mut terminal = Terminal::new().unwrap();
    let mut app = InkChatApp::default();
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<ChatAppMsg>();

    let start = Instant::now();
    let iterations = 100;

    for i in 0..iterations {
        let tree = app.view();
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
    let mut terminal = Terminal::new().unwrap();
    let mut app = InkChatApp::default();

    let start = Instant::now();

    for _ in 0..500 {
        let tree = app.view();
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

#[tokio::test]
async fn render_with_messages_does_not_accumulate() {
    let mut terminal = Terminal::new().unwrap();
    let mut app = InkChatApp::default();

    for i in 0..20 {
        app.on_message(ChatAppMsg::UserMessage(format!("Message {}", i)));
        app.on_message(ChatAppMsg::TextDelta(format!("Response chunk {} ", i)));
    }
    app.on_message(ChatAppMsg::StreamComplete);

    let start = Instant::now();

    for _ in 0..100 {
        let tree = app.view();
        terminal.render(&tree).unwrap();
        let _ = app.update(Event::Tick);
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
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
async fn escape_key_triggers_quit_action() {
    use crate::tui::ink::app::{Action, App};
    use crate::tui::ink::chat_app::InkChatApp;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = InkChatApp::default();

    let esc_event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let action = app.update(esc_event);

    assert!(
        matches!(action, Action::Quit),
        "ESC should produce Action::Quit, got {:?}",
        action
    );
}

#[tokio::test]
async fn ctrl_c_triggers_quit_action() {
    use crate::tui::ink::app::{Action, App};
    use crate::tui::ink::chat_app::InkChatApp;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = InkChatApp::default();

    let ctrl_c_event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    let action = app.update(ctrl_c_event);

    assert!(
        matches!(action, Action::Quit),
        "Ctrl+C should produce Action::Quit, got {:?}",
        action
    );
}
