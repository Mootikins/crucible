use crate::tui::ink::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn input_buffer_insert_chars() {
    let mut buf = InputBuffer::new();

    buf.handle(InputAction::Insert('h'));
    buf.handle(InputAction::Insert('i'));

    assert_eq!(buf.content(), "hi");
    assert_eq!(buf.cursor(), 2);
}

#[test]
fn input_buffer_backspace() {
    let mut buf = InputBuffer::new();
    buf.set_content("hello");

    buf.handle(InputAction::Backspace);
    assert_eq!(buf.content(), "hell");

    buf.handle(InputAction::Backspace);
    assert_eq!(buf.content(), "hel");
}

#[test]
fn input_buffer_delete() {
    let mut buf = InputBuffer::new();
    buf.set_content("hello");

    buf.handle(InputAction::Home);
    buf.handle(InputAction::Delete);
    assert_eq!(buf.content(), "ello");
}

#[test]
fn input_buffer_cursor_movement() {
    let mut buf = InputBuffer::new();
    buf.set_content("hello");

    assert_eq!(buf.cursor(), 5);

    buf.handle(InputAction::Left);
    assert_eq!(buf.cursor(), 4);

    buf.handle(InputAction::Home);
    assert_eq!(buf.cursor(), 0);

    buf.handle(InputAction::Right);
    assert_eq!(buf.cursor(), 1);

    buf.handle(InputAction::End);
    assert_eq!(buf.cursor(), 5);
}

#[test]
fn input_buffer_submit() {
    let mut buf = InputBuffer::new();
    buf.set_content("hello");

    let result = buf.handle(InputAction::Submit);
    assert_eq!(result, Some("hello".to_string()));
    assert_eq!(buf.content(), "");
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn input_buffer_submit_empty_does_nothing() {
    let mut buf = InputBuffer::new();

    let result = buf.handle(InputAction::Submit);
    assert_eq!(result, None);
}

#[test]
fn input_buffer_clear() {
    let mut buf = InputBuffer::new();
    buf.set_content("hello");

    buf.handle(InputAction::Clear);
    assert_eq!(buf.content(), "");
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn input_buffer_history() {
    let mut buf = InputBuffer::new();

    buf.set_content("first");
    buf.handle(InputAction::Submit);

    buf.set_content("second");
    buf.handle(InputAction::Submit);

    buf.set_content("third");
    buf.handle(InputAction::Submit);

    buf.handle(InputAction::HistoryPrev);
    assert_eq!(buf.content(), "third");

    buf.handle(InputAction::HistoryPrev);
    assert_eq!(buf.content(), "second");

    buf.handle(InputAction::HistoryNext);
    assert_eq!(buf.content(), "third");

    buf.handle(InputAction::HistoryNext);
    assert_eq!(buf.content(), "");
}

#[test]
fn input_buffer_history_preserves_draft() {
    let mut buf = InputBuffer::new();

    buf.set_content("submitted");
    buf.handle(InputAction::Submit);

    buf.set_content("draft");

    buf.handle(InputAction::HistoryPrev);
    assert_eq!(buf.content(), "submitted");

    buf.handle(InputAction::HistoryNext);
    assert_eq!(buf.content(), "draft");
}

#[test]
fn key_event_to_input_action() {
    assert_eq!(
        InputAction::from(key(KeyCode::Char('a'))),
        InputAction::Insert('a')
    );
    assert_eq!(
        InputAction::from(key(KeyCode::Backspace)),
        InputAction::Backspace
    );
    assert_eq!(InputAction::from(key(KeyCode::Delete)), InputAction::Delete);
    assert_eq!(InputAction::from(key(KeyCode::Left)), InputAction::Left);
    assert_eq!(InputAction::from(key(KeyCode::Right)), InputAction::Right);
    assert_eq!(InputAction::from(key(KeyCode::Enter)), InputAction::Submit);
    assert_eq!(
        InputAction::from(key(KeyCode::Up)),
        InputAction::HistoryPrev
    );
    assert_eq!(
        InputAction::from(key(KeyCode::Down)),
        InputAction::HistoryNext
    );
}

#[test]
fn ctrl_key_bindings() {
    assert_eq!(InputAction::from(ctrl('w')), InputAction::DeleteWord);
    assert_eq!(InputAction::from(ctrl('u')), InputAction::Clear);
    assert_eq!(InputAction::from(ctrl('a')), InputAction::Home);
    assert_eq!(InputAction::from(ctrl('e')), InputAction::End);
    assert_eq!(InputAction::from(ctrl('b')), InputAction::Left);
    assert_eq!(InputAction::from(ctrl('f')), InputAction::Right);
    assert_eq!(InputAction::from(ctrl('p')), InputAction::HistoryPrev);
    assert_eq!(InputAction::from(ctrl('n')), InputAction::HistoryNext);
    assert_eq!(InputAction::from(ctrl('j')), InputAction::Insert('\n'));
}

#[test]
fn unicode_handling() {
    let mut buf = InputBuffer::new();

    buf.handle(InputAction::Insert('你'));
    buf.handle(InputAction::Insert('好'));

    assert_eq!(buf.content(), "你好");

    buf.handle(InputAction::Backspace);
    assert_eq!(buf.content(), "你");

    buf.handle(InputAction::Left);
    assert_eq!(buf.cursor(), 0);
}

#[test]
fn input_in_middle() {
    let mut buf = InputBuffer::new();
    buf.set_content("hllo");

    buf.handle(InputAction::Home);
    buf.handle(InputAction::Right);

    buf.handle(InputAction::Insert('e'));
    assert_eq!(buf.content(), "hello");
}
