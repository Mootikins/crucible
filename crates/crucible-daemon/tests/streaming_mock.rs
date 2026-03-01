//! Reusable test infrastructure for streaming agent scenarios
//!
//! Provides `StreamingMockAgent` (implements `AgentHandle`) with configurable
//! `Vec<ChatChunk>` scenarios, plus `TestHarness` bundling full daemon test setup.

use async_trait::async_trait;
use crucible_core::session::SessionType;
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatSubagentEvent, ChatToolCall, ChatToolResult, SubagentEventType,
};
use crucible_core::traits::ChatResult;
use crucible_daemon::background_manager::BackgroundJobManager;
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::{
    AgentManager, AgentManagerParams, FileSessionStorage, KilnManager, SessionManager,
};
use futures::stream::{self, BoxStream};
use futures::StreamExt;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};

/// Mock agent that yields a pre-configured sequence of `ChatChunk`s
#[derive(Clone)]
pub struct StreamingMockAgent {
    chunks: Vec<ChatChunk>,
}

impl StreamingMockAgent {
    /// Create a mock agent that yields only text chunks
    ///
    /// Each string becomes a chunk with `delta=chunk`, and the last chunk has `done=true`.
    pub fn text_only(chunks: &[&str]) -> Self {
        let chunks = chunks
            .iter()
            .enumerate()
            .map(|(i, text)| ChatChunk {
                delta: text.to_string(),
                done: i == chunks.len() - 1,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
            })
            .collect();

        Self { chunks }
    }

    /// Create a mock agent with thinking/reasoning followed by response text
    ///
    /// First chunk has `reasoning=Some(thinking)`, second has `delta=response, done=true`.
    pub fn with_thinking(thinking: &str, response: &str) -> Self {
        let chunks = vec![
            ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: Some(thinking.to_string()),
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
            },
            ChatChunk {
                delta: response.to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
            },
        ];

        Self { chunks }
    }

    /// Create a mock agent that calls a tool, receives a result, then responds
    ///
    /// Chunk 1: `tool_calls=Some(vec![ChatToolCall{...}])`
    /// Chunk 2: `tool_results=Some(vec![ChatToolResult{...}])`
    /// Chunk 3: `delta=text_after, done=true`
    pub fn with_tool_call(
        tool_name: &str,
        args: serde_json::Value,
        result: &str,
        text_after: &str,
    ) -> Self {
        let chunks = vec![
            ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: Some(vec![ChatToolCall {
                    name: tool_name.to_string(),
                    arguments: Some(args),
                    id: Some("call_1".to_string()),
                }]),
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
            },
            ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: Some(vec![ChatToolResult {
                    name: tool_name.to_string(),
                    result: result.to_string(),
                    error: None,
                    call_id: Some("call_1".to_string()),
                }]),
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
            },
            ChatChunk {
                delta: text_after.to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
            },
        ];

        Self { chunks }
    }

    /// Create a mock agent with subagent lifecycle events
    ///
    /// Yields chunks with `subagent_events` containing spawned/completed/failed events.
    pub fn with_subagent_events(events: Vec<ChatSubagentEvent>, final_text: &str) -> Self {
        let chunks = vec![
            ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: Some(events),
                precognition_notes_count: None,
            },
            ChatChunk {
                delta: final_text.to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
            },
        ];

        Self { chunks }
    }

    /// Create a mock agent that yields nothing (single empty done chunk)
    pub fn empty() -> Self {
        let chunks = vec![ChatChunk {
            delta: String::new(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
        }];

        Self { chunks }
    }
}

#[async_trait]
impl AgentHandle for StreamingMockAgent {
    fn send_message_stream(
        &mut self,
        _message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        let chunks = self.chunks.clone();
        stream::iter(chunks.into_iter().map(Ok)).boxed()
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

/// Test harness bundling full daemon test setup
pub struct TestHarness {
    pub temp_dir: TempDir,
    pub session_manager: Arc<SessionManager>,
    pub agent_manager: AgentManager,
    pub event_rx: broadcast::Receiver<SessionEventMessage>,
    pub session_id: String,
}

impl TestHarness {
    /// Create a new test harness with a fresh session
    pub async fn new() -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let (event_tx, event_rx) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));

        let agent_manager = AgentManager::new(AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: session_manager.clone(),
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
        });

        let session = session_manager
            .create_session(
                SessionType::Chat,
                temp_dir.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .expect("failed to create session");

        let session_id = session.id.clone();

        Self {
            temp_dir,
            session_manager,
            agent_manager,
            event_rx,
            session_id,
        }
    }
}

/// Helper to collect events from broadcast receiver with timeout
///
/// Loops over broadcast events, filtering by event name, with 2-second timeout.
pub async fn next_event(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    event_name: &str,
) -> SessionEventMessage {
    timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(event) = rx.recv().await {
                if event.event == event_name {
                    return event;
                }
            }
        }
    })
    .await
    .expect("timed out waiting for event")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_text_only_yields_correct_chunks() {
        let mut agent = StreamingMockAgent::text_only(&["hello", "world"]);
        let mut stream = agent.send_message_stream("test".to_string());

        let chunk1 = stream
            .next()
            .await
            .expect("chunk 1 missing")
            .expect("chunk 1 error");
        assert_eq!(chunk1.delta, "hello");
        assert!(!chunk1.done);

        let chunk2 = stream
            .next()
            .await
            .expect("chunk 2 missing")
            .expect("chunk 2 error");
        assert_eq!(chunk2.delta, "world");
        assert!(chunk2.done);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_empty_yields_single_done_chunk() {
        let mut agent = StreamingMockAgent::empty();
        let mut stream = agent.send_message_stream("test".to_string());

        let chunk = stream
            .next()
            .await
            .expect("chunk missing")
            .expect("chunk error");
        assert_eq!(chunk.delta, "");
        assert!(chunk.done);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_with_thinking_yields_reasoning_then_response() {
        let mut agent = StreamingMockAgent::with_thinking("thinking...", "response");
        let mut stream = agent.send_message_stream("test".to_string());

        let chunk1 = stream
            .next()
            .await
            .expect("chunk 1 missing")
            .expect("chunk 1 error");
        assert_eq!(chunk1.reasoning, Some("thinking...".to_string()));
        assert_eq!(chunk1.delta, "");
        assert!(!chunk1.done);

        let chunk2 = stream
            .next()
            .await
            .expect("chunk 2 missing")
            .expect("chunk 2 error");
        assert_eq!(chunk2.delta, "response");
        assert!(chunk2.done);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_with_tool_call_yields_call_result_response() {
        let args = serde_json::json!({"query": "test"});
        let mut agent =
            StreamingMockAgent::with_tool_call("search", args.clone(), "found 5 results", "Done!");
        let mut stream = agent.send_message_stream("test".to_string());

        let chunk1 = stream
            .next()
            .await
            .expect("chunk 1 missing")
            .expect("chunk 1 error");
        assert!(chunk1.tool_calls.is_some());
        let calls = chunk1.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search");
        assert!(!chunk1.done);

        let chunk2 = stream
            .next()
            .await
            .expect("chunk 2 missing")
            .expect("chunk 2 error");
        assert!(chunk2.tool_results.is_some());
        let results = chunk2.tool_results.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "search");
        assert_eq!(results[0].result, "found 5 results");
        assert!(!chunk2.done);

        let chunk3 = stream
            .next()
            .await
            .expect("chunk 3 missing")
            .expect("chunk 3 error");
        assert_eq!(chunk3.delta, "Done!");
        assert!(chunk3.done);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_with_subagent_events() {
        let events = vec![
            ChatSubagentEvent {
                id: "subagent-1".to_string(),
                event_type: SubagentEventType::Spawned,
                prompt: Some("Do something".to_string()),
                summary: None,
                error: None,
            },
            ChatSubagentEvent {
                id: "subagent-1".to_string(),
                event_type: SubagentEventType::Completed,
                prompt: None,
                summary: Some("Completed successfully".to_string()),
                error: None,
            },
        ];

        let mut agent = StreamingMockAgent::with_subagent_events(events.clone(), "All done");
        let mut stream = agent.send_message_stream("test".to_string());

        let chunk1 = stream
            .next()
            .await
            .expect("chunk 1 missing")
            .expect("chunk 1 error");
        assert!(chunk1.subagent_events.is_some());
        let subagent_events = chunk1.subagent_events.unwrap();
        assert_eq!(subagent_events.len(), 2);
        assert_eq!(subagent_events[0].event_type, SubagentEventType::Spawned);
        assert_eq!(subagent_events[1].event_type, SubagentEventType::Completed);
        assert!(!chunk1.done);

        let chunk2 = stream
            .next()
            .await
            .expect("chunk 2 missing")
            .expect("chunk 2 error");
        assert_eq!(chunk2.delta, "All done");
        assert!(chunk2.done);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_harness_creates_session() {
        let harness = TestHarness::new().await;
        assert!(!harness.session_id.is_empty());

        let session = harness
            .session_manager
            .get_session(&harness.session_id)
            .expect("failed to get session");

        assert_eq!(session.id, harness.session_id);
        assert_eq!(session.session_type, SessionType::Chat);
    }
}
