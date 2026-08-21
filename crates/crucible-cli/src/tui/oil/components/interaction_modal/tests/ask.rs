use super::{ctrl_c, key_event};
use crate::tui::oil::components::interaction_modal::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput,
};
use crossterm::event::KeyCode;
use crucible_core::interaction::{
    AskBatch, AskQuestion, AskRequest, InteractionRequest, InteractionResponse,
};

#[test]
fn test_ask_modal_selection() {
    let ask = AskRequest::new("Choose one").choices(["A", "B", "C"]);
    let mut modal = InteractionModal::new("req-2".to_string(), InteractionRequest::Ask(ask), true);

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
    let mut modal = InteractionModal::new("req-3".to_string(), InteractionRequest::Ask(ask), true);

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
    let mut modal = InteractionModal::new("req-4".to_string(), InteractionRequest::Ask(ask), true);

    let output = modal.update(InteractionModalMsg::Key(ctrl_c()));
    match output {
        InteractionModalOutput::AskResponse { response, .. } => {
            assert!(matches!(response, InteractionResponse::Cancelled));
        }
        _ => panic!("Expected AskResponse with Cancelled"),
    }
}

/// Every question a user answers in a batch reaches the requester.
///
/// The TUI used to build its reply with `AskBatchResponse::new(id)`, which sets
/// `answers: Vec::new(), cancelled: false`, and to clear the selection on the
/// way to the next question. So a plugin calling `cru.ui.ask_batch` got back
/// zero answers and `cancelled: false` — indistinguishable from a user who
/// deliberately answered nothing.
#[test]
fn a_batch_returns_one_answer_per_question() {
    let batch = AskBatch::new()
        .question(AskQuestion::new("Q1", "First").choices(["a0", "a1", "a2"]))
        .question(AskQuestion::new("Q2", "Second").choices(["b0", "b1"]));
    let mut modal = InteractionModal::new(
        "req-batch".to_string(),
        InteractionRequest::AskBatch(batch),
        true,
    );

    // Question 1: move to index 2, advance.
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));

    // Question 2: move to index 1, submit.
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));

    match output {
        InteractionModalOutput::AskResponse { response, .. } => match response {
            InteractionResponse::AskBatch(batch_response) => {
                assert!(
                    !batch_response.cancelled,
                    "the user answered; not cancelled"
                );
                assert_eq!(
                    batch_response.answers.len(),
                    2,
                    "one answer per question, got {:?}",
                    batch_response.answers
                );
                assert_eq!(batch_response.answers[0].selected, vec![2]);
                assert_eq!(batch_response.answers[1].selected, vec![1]);
            }
            other => panic!("expected an AskBatch response, got {other:?}"),
        },
        other => panic!("expected an AskResponse, got {other:?}"),
    }
}
