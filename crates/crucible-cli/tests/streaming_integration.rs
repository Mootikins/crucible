//! Integration test for TUI streaming behavior

#![allow(clippy::field_reassign_with_default)]

use crucible_cli::tui::streaming_channel::{
    create_streaming_channel, StreamingEvent, StreamingTask,
};
use crucible_core::traits::chat::{ChatChunk, ChatResult};
use futures::stream;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_streaming_integration_happy_path() {
    let (tx, mut rx) = create_streaming_channel();

    let chunks: Vec<ChatResult<ChatChunk>> = vec![
        Ok(ChatChunk {
            delta: "The ".to_string(),
            done: false,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }),
        Ok(ChatChunk {
            delta: "answer ".to_string(),
            done: false,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }),
        Ok(ChatChunk {
            delta: "is 42".to_string(),
            done: false,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }),
        Ok(ChatChunk {
            delta: "".to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }),
    ];

    let handle = StreamingTask::spawn(tx, Box::pin(stream::iter(chunks)));

    let mut events = Vec::new();
    while let Ok(Some(event)) = timeout(Duration::from_secs(1), rx.recv()).await {
        let is_done = matches!(event, StreamingEvent::Done { .. });
        events.push(event);
        if is_done {
            break;
        }
    }

    handle.await.unwrap();

    assert_eq!(events.len(), 4);
    if let StreamingEvent::Done { full_response } = &events[3] {
        assert_eq!(full_response, "The answer is 42");
    } else {
        panic!("Expected Done event");
    }
}

#[tokio::test]
async fn test_streaming_handles_empty_deltas() {
    let (tx, mut rx) = create_streaming_channel();

    let chunks: Vec<ChatResult<ChatChunk>> = vec![
        Ok(ChatChunk {
            delta: "Hello".to_string(),
            done: false,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }),
        Ok(ChatChunk {
            delta: "".to_string(),
            done: false,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }),
        Ok(ChatChunk {
            delta: " world".to_string(),
            done: false,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }),
        Ok(ChatChunk {
            delta: "".to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }),
    ];

    let handle = StreamingTask::spawn(tx, Box::pin(stream::iter(chunks)));

    let mut events = Vec::new();
    while let Ok(Some(event)) = timeout(Duration::from_secs(1), rx.recv()).await {
        let is_done = matches!(event, StreamingEvent::Done { .. });
        events.push(event);
        if is_done {
            break;
        }
    }

    handle.await.unwrap();

    let delta_count = events
        .iter()
        .filter(|e| matches!(e, StreamingEvent::Delta { .. }))
        .count();
    assert_eq!(delta_count, 2);
}
