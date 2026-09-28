//! The messages of an event that are not transcript items. The daemon folds
//! the transcript (`crucible_core::transcript`); its tests pin the rules of
//! the fold, such as the drop of a reasoning replay.

use crate::tui::oil::chat_app::messages::ChatAppMsg;
use crate::tui::oil::chat_runner::SessionEventStream;
use serde_json::json;

#[test]
fn message_complete_with_token_counts_emits_context_usage() {
    let mut stream = SessionEventStream::new();
    let msgs = stream.translate(
        "message_complete",
        &json!({
            "message_id": "m1",
            "full_response": "hi",
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150,
        }),
    );
    let has_context_usage = msgs.iter().any(|m| {
        matches!(
            m,
            ChatAppMsg::ContextUsage {
                used: 150,
                total: _
            }
        )
    });
    assert!(
        has_context_usage,
        "Expected ContextUsage(used=150) in msgs: {:?}",
        msgs
    );
}

#[test]
fn message_complete_without_token_counts_does_not_emit_context_usage() {
    let mut stream = SessionEventStream::new();
    let msgs = stream.translate(
        "message_complete",
        &json!({
            "message_id": "m1",
            "full_response": "hi",
        }),
    );
    let has_context_usage = msgs
        .iter()
        .any(|m| matches!(m, ChatAppMsg::ContextUsage { .. }));
    assert!(
        !has_context_usage,
        "Did not expect ContextUsage without token counts: {:?}",
        msgs
    );
}
