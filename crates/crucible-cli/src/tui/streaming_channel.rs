//! Channel-based streaming for TUI
//!
//! This module provides safe streaming infrastructure that replaces the previous
//! unsafe lifetime transmute approach. The key insight is that by transferring
//! stream ownership to the spawned task, we avoid lifetime issues entirely.
//!
//! # Architecture
//!
//! ```text
//! Agent::send_message_stream() -> BoxStream<'a>
//!            |
//!            v (ownership transfer)
//!     StreamingTask::spawn()
//!            |
//!            v (sends events)
//!     StreamingReceiver (polled in main loop)
//! ```
//!
//! The spawned task consumes the stream and sends typed events to the main loop,
//! which polls the channel non-blockingly to update the UI.

use crucible_core::traits::chat::{ChatChunk, ChatResult};
use futures::{Stream, StreamExt};
use std::pin::Pin;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

pub type ChatStream = Pin<Box<dyn Stream<Item = ChatResult<ChatChunk>> + Send>>;

/// Events sent from streaming task to main loop
#[derive(Debug, Clone)]
pub enum StreamingEvent {
    /// Text delta received from LLM
    Delta { text: String, seq: u64 },
    /// Reasoning/thinking delta from LLM (e.g., Qwen3-thinking, DeepSeek-R1)
    Reasoning { text: String, seq: u64 },
    /// Tool call received from LLM
    ToolCall {
        id: Option<String>,
        name: String,
        args: serde_json::Value,
    },
    /// Tool execution completed
    ToolCompleted {
        name: String,
        result: String,
        error: Option<String>,
    },
    /// Streaming complete
    Done { full_response: String },
    /// Error during streaming
    Error { message: String },
}

pub type StreamingSender = UnboundedSender<StreamingEvent>;
pub type StreamingReceiver = UnboundedReceiver<StreamingEvent>;

/// Create a channel pair for streaming events
pub fn create_streaming_channel() -> (StreamingSender, StreamingReceiver) {
    unbounded_channel()
}

/// Wraps streaming task spawning (zero-sized type, just namespace)
pub struct StreamingTask;

/// Threshold for synthetic streaming (characters). Responses larger than this
/// that arrive as a single chunk will be broken into smaller pieces for display.
const SYNTHETIC_STREAM_THRESHOLD: usize = 100;

/// Target chunk size for synthetic streaming (characters per chunk).
const SYNTHETIC_CHUNK_SIZE: usize = 20;

impl StreamingTask {
    /// Spawn a task that consumes the stream and sends events to channel
    pub fn spawn(tx: StreamingSender, mut stream: ChatStream) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut full_response = String::new();
            let mut seq = 0u64;
            let mut chunk_count = 0u64;

            debug!("StreamingTask started, awaiting chunks");

            while let Some(result) = stream.next().await {
                chunk_count += 1;
                match result {
                    Ok(chunk) => {
                        debug!(
                            chunk_num = chunk_count,
                            delta_len = chunk.delta.len(),
                            done = chunk.done,
                            has_tool_calls = chunk.tool_calls.is_some(),
                            "Received chunk"
                        );

                        // Only send Delta events for non-empty chunks
                        // Sequence numbers track actual content deltas, not empty stream items
                        if !chunk.delta.is_empty() {
                            full_response.push_str(&chunk.delta);

                            // Detect single-chunk responses (like from ACP) and synthesize streaming
                            // This provides visual feedback with token count for non-streaming agents
                            if chunk.done && chunk.delta.len() > SYNTHETIC_STREAM_THRESHOLD {
                                // Break into smaller chunks for synthetic streaming effect
                                seq = Self::emit_synthetic_chunks(&tx, &chunk.delta, seq).await;
                            } else {
                                let _ = tx.send(StreamingEvent::Delta {
                                    text: chunk.delta,
                                    seq,
                                });
                                seq += 1;
                            }
                        }

                        // Forward tool calls to the TUI
                        if let Some(tool_calls) = chunk.tool_calls {
                            for tc in tool_calls {
                                let _ = tx.send(StreamingEvent::ToolCall {
                                    id: tc.id,
                                    name: tc.name,
                                    args: tc.arguments.unwrap_or(serde_json::Value::Null),
                                });
                            }
                        }

                        // Forward tool results (completions) to the TUI
                        if let Some(tool_results) = chunk.tool_results {
                            for tr in tool_results {
                                let _ = tx.send(StreamingEvent::ToolCompleted {
                                    name: tr.name,
                                    result: tr.result,
                                    error: tr.error,
                                });
                            }
                        }

                        // Forward reasoning/thinking content to the TUI
                        if let Some(reasoning) = chunk.reasoning {
                            if !reasoning.is_empty() {
                                let _ = tx.send(StreamingEvent::Reasoning {
                                    text: reasoning,
                                    seq,
                                });
                                seq += 1;
                            }
                        }

                        if chunk.done {
                            debug!(
                                chunk_count,
                                response_len = full_response.len(),
                                "Stream done"
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(
                            chunk_count,
                            error = %e,
                            "Stream error"
                        );
                        let _ = tx.send(StreamingEvent::Error {
                            message: e.to_string(),
                        });
                        return;
                    }
                }
            }

            if full_response.is_empty() && chunk_count > 0 {
                warn!(chunk_count, "Stream completed with empty response");
            }

            let _ = tx.send(StreamingEvent::Done { full_response });
        })
    }

    /// Break a large response into smaller chunks and emit with minimal delay.
    /// Returns the next sequence number after all chunks are emitted.
    async fn emit_synthetic_chunks(tx: &StreamingSender, text: &str, mut seq: u64) -> u64 {
        let mut remaining = text;

        while !remaining.is_empty() {
            // Find a good break point (prefer word boundaries)
            let chunk_end = if remaining.len() <= SYNTHETIC_CHUNK_SIZE {
                remaining.len()
            } else {
                // Look for space near the target size
                remaining[..SYNTHETIC_CHUNK_SIZE.min(remaining.len())]
                    .rfind(' ')
                    .map(|pos| pos + 1) // Include the space
                    .unwrap_or(SYNTHETIC_CHUNK_SIZE.min(remaining.len()))
            };

            let (chunk, rest) = remaining.split_at(chunk_end);
            remaining = rest;

            let _ = tx.send(StreamingEvent::Delta {
                text: chunk.to_string(),
                seq,
            });
            seq += 1;

            // Minimal delay to allow UI to update (1ms is enough for visual effect)
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }

        seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_event_delta() {
        let event = StreamingEvent::Delta {
            text: "Hello".to_string(),
            seq: 0,
        };
        assert!(matches!(event, StreamingEvent::Delta { .. }));
    }

    #[test]
    fn test_streaming_event_done() {
        let event = StreamingEvent::Done {
            full_response: "Complete response".to_string(),
        };
        assert!(matches!(event, StreamingEvent::Done { .. }));
    }

    #[test]
    fn test_streaming_event_error() {
        let event = StreamingEvent::Error {
            message: "Connection failed".to_string(),
        };
        assert!(matches!(event, StreamingEvent::Error { .. }));
    }

    #[tokio::test]
    async fn test_create_streaming_channel() {
        let (tx, mut rx) = create_streaming_channel();

        tx.send(StreamingEvent::Delta {
            text: "test".to_string(),
            seq: 0,
        })
        .unwrap();

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, StreamingEvent::Delta { text, .. } if text == "test"));
    }

    #[tokio::test]
    async fn test_streaming_task_sends_deltas() {
        use futures::stream;

        let (tx, mut rx) = create_streaming_channel();

        let chunks: Vec<ChatResult<ChatChunk>> = vec![
            Ok(ChatChunk {
                delta: "Hello ".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
            }),
            Ok(ChatChunk {
                delta: "world".to_string(),
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
        let stream = stream::iter(chunks);

        let handle = StreamingTask::spawn(tx, Box::pin(stream));
        handle.await.unwrap();

        let e1 = rx.recv().await.unwrap();
        assert!(
            matches!(&e1, StreamingEvent::Delta { text, seq } if text == "Hello " && *seq == 0)
        );

        let e2 = rx.recv().await.unwrap();
        assert!(matches!(&e2, StreamingEvent::Delta { text, seq } if text == "world" && *seq == 1));

        let e3 = rx.recv().await.unwrap();
        assert!(
            matches!(&e3, StreamingEvent::Done { full_response } if full_response == "Hello world")
        );
    }

    #[tokio::test]
    async fn test_streaming_task_handles_error() {
        use crucible_core::traits::chat::ChatError;
        use futures::stream;

        let (tx, mut rx) = create_streaming_channel();

        let chunks: Vec<ChatResult<ChatChunk>> = vec![
            Ok(ChatChunk {
                delta: "Start".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
            }),
            Err(ChatError::Connection("Connection lost".to_string())),
        ];
        let stream = stream::iter(chunks);

        let handle = StreamingTask::spawn(tx, Box::pin(stream));
        handle.await.unwrap();

        let e1 = rx.recv().await.unwrap();
        assert!(matches!(e1, StreamingEvent::Delta { .. }));

        let e2 = rx.recv().await.unwrap();
        assert!(matches!(&e2, StreamingEvent::Error { message } if message.contains("Connection")));
    }

    #[tokio::test]
    async fn test_streaming_task_forwards_tool_calls() {
        use crucible_core::traits::chat::ChatToolCall;
        use futures::stream;
        use serde_json::json;

        let (tx, mut rx) = create_streaming_channel();

        let tool_call = ChatToolCall {
            id: Some("call_123".to_string()),
            name: "read_file".to_string(),
            arguments: Some(json!({"path": "/test.txt"})),
        };

        let chunks: Vec<ChatResult<ChatChunk>> = vec![
            Ok(ChatChunk {
                delta: "Let me read that file.".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
            }),
            Ok(ChatChunk {
                delta: "".to_string(),
                done: false,
                tool_calls: Some(vec![tool_call]),
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
        let stream = stream::iter(chunks);

        let handle = StreamingTask::spawn(tx, Box::pin(stream));
        handle.await.unwrap();

        // First event: text delta
        let e1 = rx.recv().await.unwrap();
        assert!(matches!(e1, StreamingEvent::Delta { .. }));

        // Second event: tool call
        let e2 = rx.recv().await.unwrap();
        match e2 {
            StreamingEvent::ToolCall { name, args, .. } => {
                assert_eq!(name, "read_file");
                assert_eq!(args["path"], "/test.txt");
            }
            other => panic!("Expected ToolCall event, got {:?}", other),
        }

        // Third event: done
        let e3 = rx.recv().await.unwrap();
        assert!(matches!(e3, StreamingEvent::Done { .. }));
    }
}
