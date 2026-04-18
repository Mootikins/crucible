use super::key_event;
use crate::tui::oil::components::interaction_modal::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput,
};
use crossterm::event::KeyCode;
use crucible_core::interaction::{InteractionRequest, InteractionResponse, ShowRequest};

fn make_show_modal(content: &str) -> InteractionModal {
    let show = ShowRequest::new(content);
    InteractionModal::new("show-1".into(), InteractionRequest::Show(show), false)
}

#[test]
fn show_scroll_down_and_up() {
    let content = (0..30)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut modal = make_show_modal(&content);

    assert_eq!(modal.scroll_offset, 0);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('j'))));
    assert_eq!(modal.scroll_offset, 1);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('k'))));
    assert_eq!(modal.scroll_offset, 0);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('k'))));
    assert_eq!(modal.scroll_offset, 0);
}

#[test]
fn show_dismiss_with_q() {
    let mut modal = make_show_modal("hello");
    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('q'))));
    assert!(matches!(
        output,
        InteractionModalOutput::AskResponse {
            response: InteractionResponse::Cancelled,
            ..
        }
    ));
}

#[test]
fn show_dismiss_with_esc() {
    let mut modal = make_show_modal("hello");
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
fn show_page_down() {
    let content = (0..50)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut modal = make_show_modal(&content);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::PageDown)));
    assert_eq!(modal.scroll_offset, 20);
}
