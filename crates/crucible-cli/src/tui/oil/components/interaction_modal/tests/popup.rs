use super::key_event;
use crate::tui::oil::components::interaction_modal::{
    InteractionMode, InteractionModal, InteractionModalMsg, InteractionModalOutput,
};
use crossterm::event::KeyCode;
use crucible_core::interaction::{InteractionRequest, InteractionResponse, PopupRequest};
use crucible_core::types::PopupEntry;

fn make_popup_modal(entries: Vec<&str>, allow_other: bool) -> InteractionModal {
    let mut popup = PopupRequest::new("Pick one");
    for e in entries {
        popup = popup.entry(PopupEntry::new(e));
    }
    if allow_other {
        popup = popup.allow_other();
    }
    InteractionModal::new("popup-1".into(), InteractionRequest::Popup(popup), false)
}

#[test]
fn popup_navigation_and_select() {
    let mut modal = make_popup_modal(vec!["Alpha", "Beta", "Gamma"], false);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.selected, 1);

    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    match output {
        InteractionModalOutput::AskResponse { response, .. } => match response {
            InteractionResponse::Popup(pr) => {
                assert_eq!(pr.selected_index, Some(1));
            }
            _ => panic!("Expected Popup response"),
        },
        _ => panic!("Expected AskResponse"),
    }
}

#[test]
fn popup_cancel() {
    let mut modal = make_popup_modal(vec!["A", "B"], false);
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
fn popup_other_text_input() {
    let mut modal = make_popup_modal(vec!["A"], true);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.selected, 1);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    assert_eq!(modal.mode, InteractionMode::TextInput);

    for c in "custom".chars() {
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(c))));
    }
    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    match output {
        InteractionModalOutput::AskResponse { response, .. } => match response {
            InteractionResponse::Popup(pr) => {
                assert_eq!(pr.other, Some("custom".into()));
            }
            _ => panic!("Expected Popup response"),
        },
        _ => panic!("Expected AskResponse"),
    }
}
