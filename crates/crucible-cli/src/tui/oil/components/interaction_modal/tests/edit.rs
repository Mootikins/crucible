use super::key_event;
use crate::tui::oil::components::interaction_modal::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput, InteractionMode,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::interaction::{EditRequest, InteractionRequest, InteractionResponse};

fn make_edit_modal(content: &str) -> InteractionModal {
    let edit = EditRequest::new(content);
    InteractionModal::new("edit-1".into(), InteractionRequest::Edit(edit), false)
}

#[test]
fn edit_initializes_lines() {
    let modal = make_edit_modal("line one\nline two\nline three");
    assert_eq!(modal.edit_lines.len(), 3);
    assert_eq!(modal.edit_lines[0], "line one");
}

#[test]
fn edit_normal_mode_navigation() {
    let mut modal = make_edit_modal("abc\ndef\nghi");

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('j'))));
    assert_eq!(modal.edit_cursor_line, 1);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('l'))));
    assert_eq!(modal.edit_cursor_col, 1);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('k'))));
    assert_eq!(modal.edit_cursor_line, 0);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('h'))));
    assert_eq!(modal.edit_cursor_col, 0);
}

#[test]
fn edit_insert_mode_typing() {
    let mut modal = make_edit_modal("hello");

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('i'))));
    assert_eq!(modal.mode, InteractionMode::TextInput);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('X'))));
    assert_eq!(modal.edit_lines[0], "Xhello");
    assert_eq!(modal.edit_cursor_col, 1);
}

#[test]
fn edit_ctrl_s_saves() {
    let mut modal = make_edit_modal("original");

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('i'))));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('!'))));

    let save_key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
    let output = modal.update(InteractionModalMsg::Key(save_key));
    match output {
        InteractionModalOutput::AskResponse { response, .. } => match response {
            InteractionResponse::Edit(er) => {
                assert_eq!(er.modified, "!original");
            }
            _ => panic!("Expected Edit response"),
        },
        _ => panic!("Expected AskResponse"),
    }
}

#[test]
fn edit_cancel_in_normal_mode() {
    let mut modal = make_edit_modal("text");
    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
    assert!(matches!(
        output,
        InteractionModalOutput::AskResponse {
            response: InteractionResponse::Cancelled,
            ..
        }
    ));
}

#[test]
fn edit_enter_splits_line() {
    let mut modal = make_edit_modal("abcdef");

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('i'))));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Right)));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Right)));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Right)));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));

    assert_eq!(modal.edit_lines.len(), 2);
    assert_eq!(modal.edit_lines[0], "abc");
    assert_eq!(modal.edit_lines[1], "def");
    assert_eq!(modal.edit_cursor_line, 1);
    assert_eq!(modal.edit_cursor_col, 0);
}

#[test]
fn edit_backspace_joins_lines() {
    let mut modal = make_edit_modal("abc\ndef");

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('j'))));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('i'))));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Backspace)));

    assert_eq!(modal.edit_lines.len(), 1);
    assert_eq!(modal.edit_lines[0], "abcdef");
    assert_eq!(modal.edit_cursor_line, 0);
    assert_eq!(modal.edit_cursor_col, 3);
}
