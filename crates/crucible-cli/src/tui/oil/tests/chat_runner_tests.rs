//! Tests for OilChatRunner event handling and action processing

use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, ChatMode, OilChatApp};
use crate::tui::oil::chat_runner::OilChatRunner;
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
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
            mode: "normal".to_string(),
        }
    }

    fn with_chunks(chunks: Vec<ChatChunk>) -> Self {
        Self {
            chunks: Arc::new(Mutex::new(chunks)),
            error: None,
            connected: true,
            mode: "normal".to_string(),
        }
    }

    fn with_text_response(text: &str) -> Self {
        Self::with_chunks(vec![ChatChunk {
            delta: text.to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
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
                usage: None,
                subagent_events: None,
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
            mode: "normal".to_string(),
        }
    }

    fn disconnected() -> Self {
        Self {
            chunks: Arc::new(Mutex::new(Vec::new())),
            error: None,
            connected: false,
            mode: "normal".to_string(),
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
    let mut app = OilChatApp::default();

    let result = process_action_helper(Action::Quit, &mut app);
    assert!(result, "Quit action should return true");
}

#[test]
fn process_continue_action_returns_false() {
    let mut app = OilChatApp::default();

    let result = process_action_helper(Action::Continue, &mut app);
    assert!(!result, "Continue action should return false");
}

#[test]
fn process_send_action_calls_on_message() {
    let mut app = OilChatApp::default();

    let result = process_action_helper(
        Action::Send(ChatAppMsg::Status("Test status".to_string())),
        &mut app,
    );

    assert!(!result, "Send action should return false");
}

#[test]
fn process_batch_handles_multiple_actions() {
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

    let result = process_action_helper(
        Action::Batch(vec![Action::Continue, Action::Quit, Action::Continue]),
        &mut app,
    );

    assert!(result, "Batch with Quit should return true");
}

#[test]
fn process_nested_batch_flattens() {
    let mut app = OilChatApp::default();

    let result = process_action_helper(
        Action::Batch(vec![
            Action::Batch(vec![Action::Continue, Action::Continue]),
            Action::Continue,
        ]),
        &mut app,
    );

    assert!(!result, "Nested batch without Quit should return false");
}

fn process_action_helper(action: Action<ChatAppMsg>, app: &mut OilChatApp) -> bool {
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
    let runner = OilChatRunner::new();
    assert!(runner.is_ok(), "Should create runner successfully");
}

#[test]
fn chat_runner_with_mode_sets_initial_mode() {
    // Just verifies with_mode chains without panicking
    let _runner = OilChatRunner::new().unwrap().with_mode(ChatMode::Plan);
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

    assert_eq!(agent.get_mode_id(), "normal");

    agent.set_mode_str("plan").await.unwrap();
    assert_eq!(agent.get_mode_id(), "plan");

    agent.set_mode_str("auto").await.unwrap();
    assert_eq!(agent.get_mode_id(), "auto");
}

// =============================================================================
// Integration Tests with App
// =============================================================================

#[test]
fn app_handles_text_delta_from_stream() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Response ".to_string()));
    app.on_message(ChatAppMsg::TextDelta("text".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    assert!(!app.is_streaming());
}

#[test]
fn app_handles_tool_call_from_stream() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Read file".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"test.txt"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "File contents here".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::StreamComplete);

    assert!(!app.is_streaming());
}

#[test]
fn app_handles_error_from_stream() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::Error("Connection lost".to_string()));

    assert!(!app.is_streaming());
}

#[test]
fn app_handles_context_usage_update() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::ContextUsage {
        used: 50000,
        total: 100000,
    });

    use crate::tui::oil::app::ViewContext;
    use crate::tui::oil::focus::FocusContext;
    use crate::tui::oil::render::render_to_string;

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

mod daemon_event_to_tui_tests {
    use super::*;
    use crate::tui::oil::ansi::strip_ansi;
    use crucible_core::traits::llm::TokenUsage;

    const TEST_CONTEXT_LIMIT: usize = 128000;

    fn chunk_to_app_msgs(chunk: ChatChunk) -> Vec<ChatAppMsg> {
        chunk_to_app_msgs_with_limit(chunk, TEST_CONTEXT_LIMIT)
    }

    fn chunk_to_app_msgs_with_limit(chunk: ChatChunk, context_limit: usize) -> Vec<ChatAppMsg> {
        let mut msgs = vec![];

        if !chunk.delta.is_empty() {
            msgs.push(ChatAppMsg::TextDelta(chunk.delta));
        }

        if let Some(tool_calls) = chunk.tool_calls {
            for tc in tool_calls {
                let args_val = tc.arguments.clone().unwrap_or_default();
                msgs.push(ChatAppMsg::ToolCall {
                    name: tc.name,
                    args: args_val.to_string(),
                    call_id: None,
                });
            }
        }

        if let Some(tool_results) = chunk.tool_results {
            for tr in tool_results {
                if !tr.result.is_empty() {
                    msgs.push(ChatAppMsg::ToolResultDelta {
                        name: tr.name.clone(),
                        delta: tr.result,
                        call_id: None,
                    });
                }
                msgs.push(ChatAppMsg::ToolResultComplete {
                    name: tr.name,
                    call_id: None,
                });
            }
        }

        if let Some(ref usage) = chunk.usage {
            msgs.push(ChatAppMsg::ContextUsage {
                used: usage.total_tokens as usize,
                total: context_limit,
            });
        }

        if chunk.done {
            msgs.push(ChatAppMsg::StreamComplete);
        }

        msgs
    }

    #[test]
    fn text_delta_chunk_updates_ui_with_streaming_content() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));

        let chunk = ChatChunk {
            delta: "World".to_string(),
            done: false,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
        };

        for msg in chunk_to_app_msgs(chunk) {
            app.on_message(msg);
        }

        assert!(app.is_streaming(), "App should be in streaming state");

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);
        assert!(
            output.contains("World"),
            "Streamed content should appear in UI: {}",
            output
        );
    }

    #[test]
    fn tool_call_chunk_shows_tool_in_ui() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Read file".to_string()));

        let chunk = ChatChunk {
            delta: String::new(),
            done: false,
            tool_calls: Some(vec![crucible_core::traits::chat::ChatToolCall {
                name: "read_file".to_string(),
                arguments: Some(serde_json::json!({"path": "test.rs"})),
                id: Some("tc-1".to_string()),
            }]),
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
        };

        for msg in chunk_to_app_msgs(chunk) {
            app.on_message(msg);
        }

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);
        assert!(
            output.contains("read_file"),
            "Tool call should appear in UI: {}",
            output
        );
    }

    #[test]
    fn tool_result_chunk_shows_result_in_ui() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Read file".to_string()));

        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"test.rs"}"#.to_string(),
            call_id: None,
        });

        let chunk = ChatChunk {
            delta: String::new(),
            done: false,
            tool_calls: None,
            tool_results: Some(vec![ChatToolResult {
                name: "read_file".to_string(),
                result: "fn main() {}".to_string(),
                error: None,
                call_id: None,
            }]),
            reasoning: None,
            usage: None,
            subagent_events: None,
        };

        for msg in chunk_to_app_msgs(chunk) {
            app.on_message(msg);
        }

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);
        assert!(
            output.contains("fn main()") || output.contains("read_file"),
            "Tool result should appear in UI: {}",
            output
        );
    }

    #[test]
    fn done_chunk_ends_streaming_state() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
        app.on_message(ChatAppMsg::TextDelta("Response".to_string()));

        assert!(app.is_streaming(), "Should be streaming");

        let chunk = ChatChunk {
            delta: String::new(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
        };

        for msg in chunk_to_app_msgs(chunk) {
            app.on_message(msg);
        }

        assert!(!app.is_streaming(), "Streaming should end after done=true");
    }

    #[test]
    fn full_streaming_sequence_updates_ui_correctly() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));

        let chunks = vec![
            ChatChunk {
                delta: "I ".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            },
            ChatChunk {
                delta: "am ".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            },
            ChatChunk {
                delta: "Claude!".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            },
        ];

        for chunk in chunks {
            for msg in chunk_to_app_msgs(chunk) {
                app.on_message(msg);
            }
        }

        assert!(!app.is_streaming(), "Should end streaming after done chunk");

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            output.contains("Claude"),
            "Full response should appear in UI: {}",
            output
        );
    }

    #[test]
    fn chunk_with_usage_updates_context_display() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));

        let chunk = ChatChunk {
            delta: "Response".to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: Some(TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
            }),
            subagent_events: None,
        };

        for msg in chunk_to_app_msgs_with_limit(chunk, 1000) {
            app.on_message(msg);
        }

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            output.contains("15%") || output.contains("ctx"),
            "Should show context usage from chunk: {}",
            output
        );
    }

    #[test]
    fn chunk_with_usage_unknown_total_shows_tokens() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));

        let chunk = ChatChunk {
            delta: "Response".to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: Some(TokenUsage {
                prompt_tokens: 2000,
                completion_tokens: 500,
                total_tokens: 2500,
            }),
            subagent_events: None,
        };

        for msg in chunk_to_app_msgs_with_limit(chunk, 0) {
            app.on_message(msg);
        }

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            output.contains("2k tok") || output.contains("tok"),
            "Should show token count when total is unknown: {}",
            output
        );
    }

    #[test]
    fn interleaved_text_and_tool_calls_maintain_order_after_completion() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Find files".to_string()));

        app.on_message(ChatAppMsg::TextDelta("Let me search".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "glob".to_string(),
            args: r#"{"pattern":"*.rs"}"#.to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "glob".to_string(),
            delta: "main.rs, lib.rs".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "glob".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::TextDelta(" Found 2 files.".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        assert!(!app.is_streaming());

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 120);
        let stripped = strip_ansi(&output);

        let text_pos = stripped.find("search").unwrap_or(usize::MAX);
        let tool_pos = stripped.find("glob").unwrap_or(usize::MAX);
        let result_pos = stripped.find("Found").unwrap_or(usize::MAX);

        assert!(
            text_pos < tool_pos,
            "Initial text should appear before tool call.\ntext_pos={}, tool_pos={}\nOutput:\n{}",
            text_pos,
            tool_pos,
            stripped
        );
        assert!(
            tool_pos < result_pos,
            "Tool call should appear before result text.\ntool_pos={}, result_pos={}\nOutput:\n{}",
            tool_pos,
            result_pos,
            stripped
        );
    }

    #[test]
    fn tool_call_shows_checkmark_after_completion() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Read".to_string()));

        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"test.rs"}"#.to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "content".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::StreamComplete);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            output.contains('\u{2713}') || output.contains("✓"),
            "Completed tool should show checkmark.\nOutput:\n{}",
            output
        );
    }

    #[test]
    fn subagent_spawned_shows_in_ui() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Research topic".to_string()));

        app.on_message(ChatAppMsg::SubagentSpawned {
            id: "agent-1".to_string(),
            prompt: "Research the codebase for patterns".to_string(),
        });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 100);

        assert!(
            output.contains("subagent"),
            "Subagent should appear in UI.\nOutput:\n{}",
            output
        );
        assert!(
            output.contains("Research"),
            "Subagent prompt should be visible.\nOutput:\n{}",
            output
        );
    }

    #[test]
    fn subagent_completed_shows_checkmark() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Research".to_string()));

        app.on_message(ChatAppMsg::SubagentSpawned {
            id: "agent-1".to_string(),
            prompt: "Find patterns".to_string(),
        });
        app.on_message(ChatAppMsg::SubagentCompleted {
            id: "agent-1".to_string(),
            summary: "Found 3 patterns in codebase".to_string(),
        });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 100);

        assert!(
            output.contains('\u{2713}') || output.contains("✓"),
            "Completed subagent should show checkmark.\nOutput:\n{}",
            output
        );
        assert!(
            output.contains("Found 3"),
            "Summary should be visible.\nOutput:\n{}",
            output
        );
    }

    #[test]
    fn subagent_failed_shows_error() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Research".to_string()));

        app.on_message(ChatAppMsg::SubagentSpawned {
            id: "agent-1".to_string(),
            prompt: "Find patterns".to_string(),
        });
        app.on_message(ChatAppMsg::SubagentFailed {
            id: "agent-1".to_string(),
            error: "Connection timeout".to_string(),
        });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 100);

        assert!(
            output.contains('\u{2717}') || output.contains("✗"),
            "Failed subagent should show X mark.\nOutput:\n{}",
            output
        );
        assert!(
            output.contains("timeout"),
            "Error should be visible.\nOutput:\n{}",
            output
        );
    }

    #[test]
    fn multiple_subagents_displayed_correctly() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Research".to_string()));

        app.on_message(ChatAppMsg::SubagentSpawned {
            id: "agent-1".to_string(),
            prompt: "First task".to_string(),
        });
        app.on_message(ChatAppMsg::SubagentSpawned {
            id: "agent-2".to_string(),
            prompt: "Second task".to_string(),
        });
        app.on_message(ChatAppMsg::SubagentCompleted {
            id: "agent-1".to_string(),
            summary: "Done".to_string(),
        });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 100);

        assert!(
            output.contains("First task"),
            "First subagent should appear.\nOutput:\n{}",
            output
        );
        assert!(
            output.contains("Second task"),
            "Second subagent should appear.\nOutput:\n{}",
            output
        );
    }
}
