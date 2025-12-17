//! Agent Event Bridge
#![allow(deprecated)]
//!
//! Converts streaming ChatChunks from AgentHandle into SessionEvents
//! that the TUI can poll from the ring buffer.

use std::sync::Arc;

use anyhow::Result;
use crucible_core::events::SessionEvent;
use crucible_core::traits::chat::AgentHandle;
use crucible_rune::{EventRing, SessionHandle};
use futures::StreamExt;

/// Bridge between AgentHandle streaming and SessionEvent ring buffer.
///
/// Converts streaming ChatChunks from an agent into SessionEvents
/// that the TUI can poll from the ring buffer.
pub struct AgentEventBridge {
    pub(crate) handle: SessionHandle,
    pub(crate) ring: Arc<EventRing<SessionEvent>>,
}

impl AgentEventBridge {
    /// Create a new bridge with the given session handle and ring.
    pub fn new(handle: SessionHandle, ring: Arc<EventRing<SessionEvent>>) -> Self {
        Self { handle, ring }
    }

    /// Send a user message through the agent and emit events to the ring.
    ///
    /// Emits:
    /// - MessageReceived (user message)
    /// - TextDelta (for each streaming chunk)
    /// - AgentResponded (final response)
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

        // 2. Stream response, emitting TextDeltas
        let mut stream = agent.send_message_stream(message);
        let mut full_response = String::new();
        let mut seq = 0u64;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| anyhow::anyhow!("{}", e))?;

            if !chunk.delta.is_empty() {
                self.ring.push(SessionEvent::TextDelta {
                    delta: chunk.delta.clone(),
                    seq,
                });
                full_response.push_str(&chunk.delta);
                seq += 1;
            }

            if chunk.done {
                break;
            }
        }

        // 3. Emit final response
        self.ring.push(SessionEvent::AgentResponded {
            content: full_response.clone(),
            tool_calls: vec![],
        });

        Ok(full_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::traits::chat::{
        AgentHandle, ChatChunk, ChatResult,
    };
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
        fn send_message_stream<'a>(
            &'a mut self,
            _message: &'a str,
        ) -> BoxStream<'a, ChatResult<ChatChunk>> {
            let chunks = self.chunks.clone();
            let len = chunks.len();
            Box::pin(stream::iter(chunks.into_iter().enumerate().map(
                move |(i, delta)| {
                    Ok(ChatChunk {
                        delta,
                        done: i == len - 1,
                        tool_calls: None,
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
        let mut stream = agent.send_message_stream("test");

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
        use crucible_rune::SessionBuilder;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let session = SessionBuilder::new("test-bridge")
            .with_folder(temp.path())
            .build();

        let ring = session.ring().clone();
        let bridge = AgentEventBridge::new(session.handle(), ring);
        assert!(bridge.handle.session_id().contains("test-bridge"));
    }

    #[tokio::test]
    async fn test_bridge_emits_message_received() {
        use crucible_core::events::SessionEvent;
        use crucible_rune::SessionBuilder;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let session = SessionBuilder::new("test-msg")
            .with_folder(temp.path())
            .build();

        let ring = session.ring().clone();
        let bridge = AgentEventBridge::new(session.handle(), ring.clone());
        let mut agent = MockAgent::single("Response");

        bridge.send_message("Hello agent", &mut agent).await.unwrap();

        let events: Vec<_> = ring.iter().collect();
        let first = events.first().expect("Should have events");

        match first.as_ref() {
            SessionEvent::MessageReceived { content, participant_id } => {
                assert_eq!(content, "Hello agent");
                assert_eq!(participant_id, "user");
            }
            other => panic!("Expected MessageReceived, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bridge_emits_text_deltas() {
        use crucible_core::events::SessionEvent;
        use crucible_rune::SessionBuilder;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let session = SessionBuilder::new("test-deltas")
            .with_folder(temp.path())
            .build();

        let ring = session.ring().clone();
        let bridge = AgentEventBridge::new(session.handle(), ring.clone());
        let mut agent = MockAgent::new(vec![
            "Hello ".into(),
            "beautiful ".into(),
            "world!".into(),
        ]);

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
        use crucible_rune::SessionBuilder;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let session = SessionBuilder::new("test-responded")
            .with_folder(temp.path())
            .build();

        let ring = session.ring().clone();
        let bridge = AgentEventBridge::new(session.handle(), ring.clone());
        let mut agent = MockAgent::new(vec!["Hello ".into(), "world!".into()]);

        let result = bridge.send_message("Hi", &mut agent).await.unwrap();

        assert_eq!(result, "Hello world!");

        let events: Vec<_> = ring.iter().collect();
        let last = events.last().expect("Should have events");

        match last.as_ref() {
            SessionEvent::AgentResponded { content, tool_calls } => {
                assert_eq!(content, "Hello world!");
                assert!(tool_calls.is_empty());
            }
            other => panic!("Expected AgentResponded, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bridge_event_order() {
        use crucible_core::events::SessionEvent;
        use crucible_rune::SessionBuilder;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let session = SessionBuilder::new("test-order")
            .with_folder(temp.path())
            .build();

        let ring = session.ring().clone();
        let bridge = AgentEventBridge::new(session.handle(), ring.clone());
        let mut agent = MockAgent::new(vec!["A".into(), "B".into()]);

        bridge.send_message("test", &mut agent).await.unwrap();

        let events: Vec<_> = ring.iter().collect();

        assert_eq!(events.len(), 4);
        assert!(matches!(events[0].as_ref(), SessionEvent::MessageReceived { .. }));
        assert!(matches!(events[1].as_ref(), SessionEvent::TextDelta { delta, .. } if delta == "A"));
        assert!(matches!(events[2].as_ref(), SessionEvent::TextDelta { delta, .. } if delta == "B"));
        assert!(matches!(events[3].as_ref(), SessionEvent::AgentResponded { content, .. } if content == "AB"));
    }
}
