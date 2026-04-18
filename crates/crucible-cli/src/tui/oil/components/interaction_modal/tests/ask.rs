use super::{ctrl_c, key_event};
use crate::tui::oil::components::interaction_modal::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput,
};
use crossterm::event::KeyCode;
use crucible_core::interaction::{AskRequest, InteractionRequest, InteractionResponse};

#[test]
fn test_ask_modal_selection() {
    let ask = AskRequest::new("Choose one").choices(["A", "B", "C"]);
    let mut modal =
        InteractionModal::new("req-2".to_string(), InteractionRequest::Ask(ask), true);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.selected, 1);

    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    match output {
        InteractionModalOutput::AskResponse {
            request_id,
            response,
        } => {
            assert_eq!(request_id, "req-2");
            match response {
                InteractionResponse::Ask(ask_resp) => {
                    assert_eq!(ask_resp.selected, vec![1]);
                }
                _ => panic!("Expected Ask response"),
            }
        }
        _ => panic!("Expected AskResponse"),
    }
}

#[test]
fn test_ask_modal_cancel_esc() {
    let ask = AskRequest::new("Choose one").choices(["A", "B"]);
    let mut modal =
        InteractionModal::new("req-3".to_string(), InteractionRequest::Ask(ask), true);

    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
    match output {
        InteractionModalOutput::AskResponse { response, .. } => {
            assert!(matches!(response, InteractionResponse::Cancelled));
        }
        _ => panic!("Expected AskResponse with Cancelled"),
    }
}

#[test]
fn test_ask_modal_cancel_ctrl_c() {
    let ask = AskRequest::new("Choose one").choices(["A", "B"]);
    let mut modal =
        InteractionModal::new("req-4".to_string(), InteractionRequest::Ask(ask), true);

    let output = modal.update(InteractionModalMsg::Key(ctrl_c()));
    match output {
        InteractionModalOutput::AskResponse { response, .. } => {
            assert!(matches!(response, InteractionResponse::Cancelled));
        }
        _ => panic!("Expected AskResponse with Cancelled"),
    }
}
