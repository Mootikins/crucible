//! Tests for InkChatRunner event handling and action processing

use crate::tui::ink::app::{Action, App, ViewContext};
use crate::tui::ink::chat_app::{ChatAppMsg, ChatMode, InkChatApp};
use crate::tui::ink::chat_runner::InkChatRunner;
use crate::tui::ink::event::Event;
use crate::tui::ink::focus::FocusContext;
use crate::tui::ink::render::render_to_string;
use async_trait::async_trait;
use crossterm::event::{
    Event as CtEvent, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
};
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolResult};
use crucible_core::types::acp::schema::{AvailableCommand, SessionModeState};
use futures::stream::{self, BoxStream};
use std::sync::{Arc, Mutex};

struct MockAgent {
    chunks: Arc<Mutex<Vec<ChatChunk>>>,
    error: Option<ChatError>,
    connected: bool,
    mode: String,
}

impl MockAgent {
    fn new() -> Self {
        Self {
            chunks: Arc::new(Mutex::new(Vec::new())),
            error: None,
            connected: true,
            mode: "plan".to_string(),
        }
    }

    fn with_chunks(chunks: Vec<ChatChunk>) -> Self {
        Self {
            chunks: Arc::new(Mutex::new(chunks)),
            error: None,
            connected: true,
            mode: "plan".to_string(),
        }
    }

    fn with_text_response(text: &str) -> Self {
        Self::with_chunks(vec![ChatChunk {
            delta: text.to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }])
    }

    fn with_streaming_response(chunks: Vec<&str>) -> Self {
        let mut chat_chunks: Vec<ChatChunk> = chunks
            .iter()
            .map(|s| ChatChunk {
                delta: s.to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
            })
            .collect();

        if let Some(last) = chat_chunks.last_mut() {
            last.done = true;
        }

        Self::with_chunks(chat_chunks)
    }

    fn with_error(error: ChatError) -> Self {
        Self {
            chunks: Arc::new(Mutex::new(Vec::new())),
            error: Some(error),
            connected: true,
            mode: "plan".to_string(),
        }
    }

    fn disconnected() -> Self {
        Self {
            chunks: Arc::new(Mutex::new(Vec::new())),
            error: None,
            connected: false,
            mode: "plan".to_string(),
        }
    }
}

#[async_trait]
impl AgentHandle for MockAgent {
    fn send_message_stream(
        &mut self,
        _message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        if let Some(err) = self.error.take() {
            return Box::pin(stream::once(async move { Err(err) }));
        }

        let chunks = self.chunks.lock().unwrap().drain(..).collect::<Vec<_>>();
        Box::pin(stream::iter(chunks.into_iter().map(Ok)))
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        self.mode = mode_id.to_string();
        Ok(())
    }

    fn get_mode_id(&self) -> &str {
        &self.mode
    }
}

// =============================================================================
// Action Processing Tests
// =============================================================================

#[test]
fn process_quit_action_returns_true() {
    let mut app = InkChatApp::default();

    let result = process_action_helper(Action::Quit, &mut app);
    assert!(result, "Quit action should return true");
}

#[test]
fn process_continue_action_returns_false() {
    let mut app = InkChatApp::default();

    let result = process_action_helper(Action::Continue, &mut app);
    assert!(!result, "Continue action should return false");
}

#[test]
fn process_send_action_calls_on_message() {
    let mut app = InkChatApp::default();

    let result = process_action_helper(
        Action::Send(ChatAppMsg::Status("Test status".to_string())),
        &mut app,
    );

    assert!(!result, "Send action should return false");
}

#[test]
fn process_batch_handles_multiple_actions() {
    let mut app = InkChatApp::default();

    let result = process_action_helper(
        Action::Batch(vec![
            Action::Send(ChatAppMsg::Status("First".to_string())),
            Action::Send(ChatAppMsg::Status("Second".to_string())),
            Action::Continue,
        ]),
        &mut app,
    );

    assert!(!result, "Batch without Quit should return false");
}

#[test]
fn process_batch_stops_on_quit() {
    let mut app = InkChatApp::default();

    let result = process_action_helper(
        Action::Batch(vec![Action::Continue, Action::Quit, Action::Continue]),
        &mut app,
    );

    assert!(result, "Batch with Quit should return true");
}

#[test]
fn process_nested_batch_flattens() {
    let mut app = InkChatApp::default();

    let result = process_action_helper(
        Action::Batch(vec![
            Action::Batch(vec![Action::Continue, Action::Continue]),
            Action::Continue,
        ]),
        &mut app,
    );

    assert!(!result, "Nested batch without Quit should return false");
}

fn process_action_helper(action: Action<ChatAppMsg>, app: &mut InkChatApp) -> bool {
    match action {
        Action::Quit => true,
        Action::Continue => false,
        Action::Send(msg) => {
            let next_action = app.on_message(msg);
            process_action_helper(next_action, app)
        }
        Action::Batch(actions) => {
            for action in actions {
                if process_action_helper(action, app) {
                    return true;
                }
            }
            false
        }
    }
}

// =============================================================================
// Builder & Configuration Tests
// =============================================================================

#[test]
fn chat_runner_new_creates_with_defaults() {
    let runner = InkChatRunner::new();
    assert!(runner.is_ok(), "Should create runner successfully");
}

#[test]
fn chat_runner_with_mode_sets_initial_mode() {
    // Just verifies with_mode chains without panicking
    let _runner = InkChatRunner::new().unwrap().with_mode(ChatMode::Act);
}

// =============================================================================
// Mock Agent Tests
// =============================================================================

#[test]
fn mock_agent_returns_configured_chunks() {
    let mut agent = MockAgent::with_text_response("Hello, world!");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        use futures::StreamExt;

        let mut stream = agent.send_message_stream("test".to_string());
        let chunk = stream.next().await.unwrap().unwrap();

        assert_eq!(chunk.delta, "Hello, world!");
        assert!(chunk.done);
    });
}

#[test]
fn mock_agent_streaming_returns_multiple_chunks() {
    let mut agent = MockAgent::with_streaming_response(vec!["Hello, ", "world", "!"]);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        use futures::StreamExt;

        let stream = agent.send_message_stream("test".to_string());
        let chunks: Vec<_> = stream.collect().await;

        assert_eq!(chunks.len(), 3);
        assert!(!chunks[0].as_ref().unwrap().done);
        assert!(!chunks[1].as_ref().unwrap().done);
        assert!(chunks[2].as_ref().unwrap().done);
    });
}

#[test]
fn mock_agent_error_returns_error() {
    let mut agent = MockAgent::with_error(ChatError::Connection("Test error".to_string()));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        use futures::StreamExt;

        let mut stream = agent.send_message_stream("test".to_string());
        let result = stream.next().await.unwrap();

        assert!(result.is_err());
    });
}

#[test]
fn mock_agent_connected_state() {
    let connected = MockAgent::new();
    let disconnected = MockAgent::disconnected();

    assert!(connected.is_connected());
    assert!(!disconnected.is_connected());
}

#[tokio::test]
async fn mock_agent_mode_changes() {
    let mut agent = MockAgent::new();

    assert_eq!(agent.get_mode_id(), "plan");

    agent.set_mode_str("act").await.unwrap();
    assert_eq!(agent.get_mode_id(), "act");

    agent.set_mode_str("auto").await.unwrap();
    assert_eq!(agent.get_mode_id(), "auto");
}

// =============================================================================
// Integration Tests with App
// =============================================================================

#[test]
fn app_handles_text_delta_from_stream() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Response ".to_string()));
    app.on_message(ChatAppMsg::TextDelta("text".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    assert!(!app.is_streaming());
}

#[test]
fn app_handles_tool_call_from_stream() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Read file".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"test.txt"}"#.to_string(),
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "File contents here".to_string(),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
    });
    app.on_message(ChatAppMsg::StreamComplete);

    assert!(!app.is_streaming());
}

#[test]
fn app_handles_error_from_stream() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::Error("Connection lost".to_string()));

    assert!(!app.is_streaming());
}

#[test]
fn app_handles_context_usage_update() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::ContextUsage {
        used: 50000,
        total: 100000,
    });

    use crate::tui::ink::app::ViewContext;
    use crate::tui::ink::focus::FocusContext;
    use crate::tui::ink::render::render_to_string;

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("50%") || output.contains("ctx"),
        "Should show context percentage: {}",
        output
    );
}
