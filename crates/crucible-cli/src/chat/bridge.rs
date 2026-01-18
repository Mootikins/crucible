//! Agent Event Bridge
#![allow(deprecated)]
//!
//! Converts streaming ChatChunks from AgentHandle into SessionEvents
//! that the TUI can poll from the ring buffer.

use std::sync::Arc;

use anyhow::Result;
use crucible_core::events::{EventRing, SessionEvent, ToolCall};
use crucible_core::traits::chat::{AgentHandle, ChatToolCall};
use futures::StreamExt;

/// Bridge between AgentHandle streaming and SessionEvent ring buffer.
///
/// Converts streaming ChatChunks from an agent into SessionEvents
/// that the TUI can poll from the ring buffer.
pub struct AgentEventBridge {
    pub(crate) ring: Arc<EventRing<SessionEvent>>,
}

impl AgentEventBridge {
    /// Create a new bridge with the given ring buffer.
    pub fn new(ring: Arc<EventRing<SessionEvent>>) -> Self {
        Self { ring }
    }

    /// Send a user message through the agent and emit events to the ring.
    ///
    /// Emits:
    /// - MessageReceived (user message)
    /// - TextDelta (for each streaming chunk)
    /// - ToolCalled (for each tool call)
    /// - AgentResponded (final response with accumulated tool calls)
    pub async fn send_message<A: AgentHandle>(
        &self,
        message: &str,
        agent: &mut A,
    ) -> Result<String> {
        // 1. Emit user message
        self.ring.push(SessionEvent::MessageReceived {
            content: message.to_string(),
            participant_id: "user".to_string(),
        });

        // 2. Stream response, emitting TextDeltas and ToolCalled events
        let mut stream = agent.send_message_stream(message.to_string());
        let mut full_response = String::new();
        let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();
        let mut seq = 0u64;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| anyhow::anyhow!("{}", e))?;

            // Emit text deltas
            if !chunk.delta.is_empty() {
                self.ring.push(SessionEvent::TextDelta {
                    delta: chunk.delta.clone(),
                    seq,
                });
                full_response.push_str(&chunk.delta);
                seq += 1;
            }

            // Handle tool calls - emit ToolCalled events and accumulate for final response
            if let Some(ref tool_calls) = chunk.tool_calls {
                for tc in tool_calls {
                    let tool_call = convert_chat_tool_call(tc);

                    // Emit ToolCalled event for each tool call
                    self.ring.push(SessionEvent::ToolCalled {
                        name: tool_call.name.clone(),
                        args: tool_call.args.clone(),
                    });

                    accumulated_tool_calls.push(tool_call);
                }
            }

            if chunk.done {
                break;
            }
        }

        // 3. Emit final response with accumulated tool calls
        self.ring.push(SessionEvent::AgentResponded {
            content: full_response.clone(),
            tool_calls: accumulated_tool_calls,
        });

        Ok(full_response)
    }
}

/// Convert a ChatToolCall to a ToolCall for SessionEvent
fn convert_chat_tool_call(tc: &ChatToolCall) -> ToolCall {
    // Use arguments directly (already serde_json::Value)
    let args = tc.arguments.clone().unwrap_or(serde_json::Value::Null);

    ToolCall {
        name: tc.name.clone(),
        args,
        call_id: tc.id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatResult};
    use futures::stream::{self, BoxStream};

    /// Mock agent that returns predefined chunks for testing.
    struct MockAgent {
        /// Chunks to return (each becomes a TextDelta)
        chunks: Vec<String>,
    }

    impl MockAgent {
        /// Create a mock that will stream the given chunks.
        fn new(chunks: Vec<String>) -> Self {
            Self { chunks }
        }

        /// Create a mock that returns a single complete response.
        #[allow(dead_code)]
        fn single(response: &str) -> Self {
            Self::new(vec![response.to_string()])
        }
    }

    #[async_trait]
    impl AgentHandle for MockAgent {
        fn send_message_stream(
            &mut self,
            _message: String,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            let chunks = self.chunks.clone();
            let len = chunks.len();
            Box::pin(stream::iter(chunks.into_iter().enumerate().map(
                move |(i, delta)| {
                    Ok(ChatChunk {
                        delta,
                        done: i == len - 1,
                        tool_calls: None,
                        tool_results: None,
                        reasoning: None,
                    })
                },
            )))
        }

        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_mock_agent_streams_chunks() {
        use futures::StreamExt;

        let mut agent = MockAgent::new(vec!["Hello ".into(), "world".into()]);
        let mut stream = agent.send_message_stream("test".to_string());

        let chunk1 = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk1.delta, "Hello ");
        assert!(!chunk1.done);

        let chunk2 = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk2.delta, "world");
        assert!(chunk2.done);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_bridge_creation() {
        let ring = Arc::new(EventRing::new(1024));
        let _bridge = AgentEventBridge::new(ring);
    }

    #[tokio::test]
    async fn test_bridge_emits_message_received() {
        use crucible_core::events::SessionEvent;

        let ring = Arc::new(EventRing::new(1024));
        let bridge = AgentEventBridge::new(ring.clone());
        let mut agent = MockAgent::single("Response");

        bridge
            .send_message("Hello agent", &mut agent)
            .await
            .unwrap();

        let events: Vec<_> = ring.iter().collect();
        let first = events.first().expect("Should have events");

        match first.as_ref() {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => {
                assert_eq!(content, "Hello agent");
                assert_eq!(participant_id, "user");
            }
            other => panic!("Expected MessageReceived, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bridge_emits_text_deltas() {
        use crucible_core::events::SessionEvent;

        let ring = Arc::new(EventRing::new(1024));
        let bridge = AgentEventBridge::new(ring.clone());
        let mut agent = MockAgent::new(vec!["Hello ".into(), "beautiful ".into(), "world!".into()]);

        bridge.send_message("Hi", &mut agent).await.unwrap();

        let events: Vec<_> = ring.iter().collect();
        let deltas: Vec<_> = events
            .iter()
            .filter_map(|e| match e.as_ref() {
                SessionEvent::TextDelta { delta, seq } => Some((delta.clone(), *seq)),
                _ => None,
            })
            .collect();

        assert_eq!(deltas.len(), 3, "Should have 3 TextDelta events");
        assert_eq!(deltas[0].0, "Hello ");
        assert_eq!(deltas[1].0, "beautiful ");
        assert_eq!(deltas[2].0, "world!");
        assert_eq!(deltas[0].1, 0);
        assert_eq!(deltas[1].1, 1);
        assert_eq!(deltas[2].1, 2);
    }

    #[tokio::test]
    async fn test_bridge_emits_agent_responded() {
        use crucible_core::events::SessionEvent;

        let ring = Arc::new(EventRing::new(1024));
        let bridge = AgentEventBridge::new(ring.clone());
        let mut agent = MockAgent::new(vec!["Hello ".into(), "world!".into()]);

        let result = bridge.send_message("Hi", &mut agent).await.unwrap();

        assert_eq!(result, "Hello world!");

        let events: Vec<_> = ring.iter().collect();
        let last = events.last().expect("Should have events");

        match last.as_ref() {
            SessionEvent::AgentResponded {
                content,
                tool_calls,
            } => {
                assert_eq!(content, "Hello world!");
                assert!(tool_calls.is_empty());
            }
            other => panic!("Expected AgentResponded, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bridge_event_order() {
        use crucible_core::events::SessionEvent;

        let ring = Arc::new(EventRing::new(1024));
        let bridge = AgentEventBridge::new(ring.clone());
        let mut agent = MockAgent::new(vec!["A".into(), "B".into()]);

        bridge.send_message("test", &mut agent).await.unwrap();

        let events: Vec<_> = ring.iter().collect();

        assert_eq!(events.len(), 4);
        assert!(matches!(
            events[0].as_ref(),
            SessionEvent::MessageReceived { .. }
        ));
        assert!(
            matches!(events[1].as_ref(), SessionEvent::TextDelta { delta, .. } if delta == "A")
        );
        assert!(
            matches!(events[2].as_ref(), SessionEvent::TextDelta { delta, .. } if delta == "B")
        );
        assert!(
            matches!(events[3].as_ref(), SessionEvent::AgentResponded { content, .. } if content == "AB")
        );
    }

    /// Mock agent that returns tool calls in the final chunk
    struct MockAgentWithTools {
        text_chunks: Vec<String>,
        tool_calls: Vec<ChatToolCall>,
    }

    impl MockAgentWithTools {
        fn new(text_chunks: Vec<String>, tool_calls: Vec<ChatToolCall>) -> Self {
            Self {
                text_chunks,
                tool_calls,
            }
        }
    }

    #[async_trait]
    impl AgentHandle for MockAgentWithTools {
        fn send_message_stream(
            &mut self,
            _message: String,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            let chunks = self.text_chunks.clone();
            let tool_calls = self.tool_calls.clone();
            let len = chunks.len();

            Box::pin(stream::iter(chunks.into_iter().enumerate().map(
                move |(i, delta)| {
                    let is_last = i == len - 1;
                    Ok(ChatChunk {
                        delta,
                        done: is_last,
                        // Include tool calls in final chunk
                        tool_calls: if is_last && !tool_calls.is_empty() {
                            Some(tool_calls.clone())
                        } else {
                            None
                        },
                        tool_results: None,
                        reasoning: None,
                    })
                },
            )))
        }

        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_bridge_emits_tool_called_events() {
        use crucible_core::events::SessionEvent;

        let ring = Arc::new(EventRing::new(1024));
        let bridge = AgentEventBridge::new(ring.clone());

        let tool_calls = vec![ChatToolCall {
            name: "read_file".to_string(),
            arguments: Some(serde_json::json!({"path": "test.txt"})),
            id: Some("call_123".to_string()),
        }];
        let mut agent = MockAgentWithTools::new(vec!["Reading file...".into()], tool_calls);

        bridge
            .send_message("Read test.txt", &mut agent)
            .await
            .unwrap();

        let events: Vec<_> = ring.iter().collect();

        // Should have: MessageReceived, TextDelta, ToolCalled, AgentResponded
        assert_eq!(events.len(), 4);

        // Check ToolCalled event
        let tool_event = events
            .iter()
            .find(|e| matches!(e.as_ref(), SessionEvent::ToolCalled { .. }))
            .expect("Should have ToolCalled event");

        match tool_event.as_ref() {
            SessionEvent::ToolCalled { name, args } => {
                assert_eq!(name, "read_file");
                assert_eq!(args["path"], "test.txt");
            }
            _ => panic!("Expected ToolCalled"),
        }

        // Check AgentResponded includes tool calls
        let responded = events.last().expect("Should have events");
        match responded.as_ref() {
            SessionEvent::AgentResponded { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "read_file");
                assert_eq!(tool_calls[0].call_id, Some("call_123".to_string()));
            }
            _ => panic!("Expected AgentResponded"),
        }
    }

    #[tokio::test]
    async fn test_bridge_handles_multiple_tool_calls() {
        use crucible_core::events::SessionEvent;

        let ring = Arc::new(EventRing::new(1024));
        let bridge = AgentEventBridge::new(ring.clone());

        let tool_calls = vec![
            ChatToolCall {
                name: "read_file".to_string(),
                arguments: Some(serde_json::json!({"path": "a.txt"})),
                id: Some("call_1".to_string()),
            },
            ChatToolCall {
                name: "write_file".to_string(),
                arguments: Some(serde_json::json!({"path": "b.txt", "content": "hello"})),
                id: Some("call_2".to_string()),
            },
        ];
        let mut agent = MockAgentWithTools::new(vec!["Done".into()], tool_calls);

        bridge.send_message("Do stuff", &mut agent).await.unwrap();

        let events: Vec<_> = ring.iter().collect();

        // Count ToolCalled events
        let tool_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.as_ref(), SessionEvent::ToolCalled { .. }))
            .collect();

        assert_eq!(tool_events.len(), 2, "Should have 2 ToolCalled events");
    }
}
