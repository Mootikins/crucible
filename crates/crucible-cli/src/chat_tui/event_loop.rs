//! Event loop for the chat TUI
//!
//! This module provides the main event loop that:
//! - Polls for crossterm events
//! - Handles keyboard input
//! - Manages terminal rendering
//! - Coordinates with async operations
//! - Integrates with AgentHandle for chat backend communication

use std::io::Stdout;
use std::time::Duration;

use anyhow::{Context, Result};
use crucible_core::traits::chat::{AgentHandle, ChatResponse};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;

use super::app::ChatApp;
use super::messages::ChatMessageDisplay;
use super::render::render_chat_viewport;
use super::{cleanup_terminal, setup_inline_terminal, VIEWPORT_HEIGHT};

/// Message to send to the chat backend
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub content: String,
}

/// Result of handling a single event
pub enum EventResult {
    /// Continue the event loop
    Continue,
    /// User requested to quit
    Quit,
    /// Send a message to the agent
    SendMessage(String),
    /// A local command was handled (e.g., /clear)
    CommandHandled,
}

/// Run the main event loop for the chat TUI
///
/// This function:
/// - Sets up the terminal with inline viewport
/// - Polls for events with 10ms timeout
/// - Handles key events
/// - Re-renders when dirty
/// - Cleans up on exit
///
/// Returns messages to send via the provided channel
pub async fn run_event_loop(message_tx: mpsc::UnboundedSender<ChatMessage>) -> Result<()> {
    let mut terminal =
        setup_inline_terminal(VIEWPORT_HEIGHT).context("failed to setup inline terminal")?;

    let mut app = ChatApp::new();

    let result = event_loop_inner(&mut terminal, &mut app, &message_tx).await;

    // Always cleanup, even on error
    cleanup_terminal().context("failed to cleanup terminal")?;

    result
}

/// Response from the agent communication task
#[derive(Debug)]
pub enum AgentResponse {
    /// Successful response from agent
    Success(ChatResponse),
    /// Error from agent
    Error(String),
}

/// Run the event loop with full agent integration
///
/// This is the primary entry point for the chat TUI with agent backend.
/// It:
/// - Sets up the terminal with inline viewport
/// - Spawns a background task for agent communication
/// - Uses `tokio::select!` to handle keyboard events and agent responses
/// - Displays user and agent messages via `insert_before()`
/// - Shows streaming indicator while waiting for responses
/// - Cleans up on exit
///
/// # Arguments
/// * `agent` - The agent handle to communicate with
///
/// # Example
/// ```no_run
/// use crucible_cli::chat_tui::run_with_agent;
///
/// async fn example<A: crucible_core::traits::chat::AgentHandle>(agent: A) {
///     run_with_agent(agent).await.unwrap();
/// }
/// ```
pub async fn run_with_agent<A: AgentHandle + 'static>(mut agent: A) -> Result<()> {
    let mut terminal =
        setup_inline_terminal(VIEWPORT_HEIGHT).context("failed to setup inline terminal")?;

    let mut app = ChatApp::new();

    // Channels for agent communication
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<String>();
    let (response_tx, mut response_rx) = mpsc::unbounded_channel::<AgentResponse>();

    // Spawn agent communication task
    tokio::spawn(async move {
        while let Some(content) = message_rx.recv().await {
            let response = match agent.send_message(&content).await {
                Ok(resp) => AgentResponse::Success(resp),
                Err(e) => AgentResponse::Error(e.to_string()),
            };
            if response_tx.send(response).is_err() {
                break; // Main loop closed
            }
        }
    });

    let result =
        event_loop_with_agent(&mut terminal, &mut app, &message_tx, &mut response_rx).await;

    // Always cleanup, even on error
    cleanup_terminal().context("failed to cleanup terminal")?;

    result
}

/// Inner event loop with agent integration
async fn event_loop_with_agent(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut ChatApp,
    message_tx: &mpsc::UnboundedSender<String>,
    response_rx: &mut mpsc::UnboundedReceiver<AgentResponse>,
) -> Result<()> {
    loop {
        // Render if dirty
        if app.needs_render() {
            terminal
                .draw(|frame| {
                    render_chat_viewport(app, frame);
                })
                .context("failed to draw terminal")?;
            app.render_state.clear();
        }

        // Use tokio::select! to handle both keyboard events and agent responses
        tokio::select! {
            // Handle keyboard events (with timeout for responsiveness)
            _ = tokio::time::sleep(Duration::from_millis(10)) => {
                if event::poll(Duration::from_millis(0)).context("failed to poll events")? {
                    match event::read().context("failed to read event")? {
                        Event::Key(key) => {
                            if key.kind == KeyEventKind::Press {
                                match handle_key_with_agent(app, key, message_tx, terminal)? {
                                    EventResult::Quit => break,
                                    EventResult::SendMessage(_) => {
                                        // Message already sent by handle_key_with_agent
                                    }
                                    EventResult::CommandHandled => {
                                        // Local command was processed
                                    }
                                    EventResult::Continue => {}
                                }
                            }
                        }
                        Event::Resize(_, _) => {
                            app.render_state.mark_dirty();
                        }
                        _ => {}
                    }
                }
            }

            // Handle agent responses
            Some(response) = response_rx.recv() => {
                match response {
                    AgentResponse::Success(resp) => {
                        let msg = ChatMessageDisplay::assistant(&resp.content);
                        ChatApp::insert_message(terminal, &msg)
                            .context("failed to insert assistant message")?;
                    }
                    AgentResponse::Error(err) => {
                        let msg = ChatMessageDisplay::system(format!("Error: {}", err));
                        ChatApp::insert_message(terminal, &msg)
                            .context("failed to insert error message")?;
                    }
                }
                app.set_streaming(false);
                app.render_state.mark_dirty();
            }
        }

        // Check if app wants to exit
        if app.should_exit {
            break;
        }

        // Yield to tokio runtime
        tokio::task::yield_now().await;
    }

    Ok(())
}

/// Handle a key event with agent integration
///
/// Similar to `handle_key_event` but also handles message display and sending.
/// Generic over the terminal backend to support both real terminals and test backends.
fn handle_key_with_agent<B: ratatui::backend::Backend>(
    app: &mut ChatApp,
    key: KeyEvent,
    message_tx: &mpsc::UnboundedSender<String>,
    terminal: &mut Terminal<B>,
) -> Result<EventResult> {
    // Global quit shortcuts that bypass normal handling
    if key.code == KeyCode::Char('d') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Ok(EventResult::Quit);
    }

    // Delegate to ChatApp.handle_key() which returns Option<String> for messages
    if let Some(content) = app.handle_key(key) {
        // Check for local commands first
        let trimmed = content.trim();

        // Handle /clear command locally
        if trimmed == "/clear" {
            handle_clear_command(terminal, app)?;
            return Ok(EventResult::CommandHandled);
        }

        // Handle /exit and /quit commands locally
        if trimmed == "/exit" || trimmed == "/quit" {
            let system_msg = ChatMessageDisplay::system("Exiting chat session...");
            ChatApp::insert_message(terminal, &system_msg)
                .context("failed to insert system message")?;
            app.request_exit();
            return Ok(EventResult::Quit);
        }

        // Display the user message in scrollback
        let user_msg = ChatMessageDisplay::user(&content);
        ChatApp::insert_message(terminal, &user_msg)
            .context("failed to insert user message")?;

        // Send to agent
        message_tx
            .send(content.clone())
            .context("failed to send message to agent")?;

        // Set streaming state
        app.set_streaming(true);
        app.render_state.mark_dirty();

        return Ok(EventResult::SendMessage(content));
    }

    Ok(EventResult::Continue)
}

/// Handle the /clear command
///
/// This displays a visual separator in the scrollback to mark a new session.
/// The agent's context is preserved (full context reset would require protocol support).
fn handle_clear_command<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut ChatApp,
) -> Result<()> {
    // Display a separator message
    let separator = ChatMessageDisplay::system("─── Session cleared ───");
    ChatApp::insert_message(terminal, &separator)
        .context("failed to insert clear separator")?;

    // Clear input and mark dirty
    app.input.clear();
    app.render_state.mark_dirty();

    Ok(())
}

/// Inner event loop logic, separated for testability
async fn event_loop_inner(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut ChatApp,
    message_tx: &mpsc::UnboundedSender<ChatMessage>,
) -> Result<()> {
    loop {
        // Render if dirty
        if app.needs_render() {
            terminal
                .draw(|frame| {
                    render_chat_viewport(app, frame);
                })
                .context("failed to draw terminal")?;
            app.render_state.clear();
        }

        // Poll for events with timeout
        if event::poll(Duration::from_millis(10)).context("failed to poll events")? {
            match event::read().context("failed to read event")? {
                Event::Key(key) => {
                    // Only handle key press events, ignore release
                    if key.kind == KeyEventKind::Press {
                        match handle_key_event(app, key, message_tx)? {
                            EventResult::Quit => break,
                            EventResult::SendMessage(content) => {
                                message_tx
                                    .send(ChatMessage { content })
                                    .context("failed to send message")?;
                            }
                            EventResult::CommandHandled => {
                                // Local command was processed (not used in this simpler loop)
                            }
                            EventResult::Continue => {}
                        }
                    }
                }
                Event::Resize(_, _) => {
                    // Terminal resized, force re-render
                    app.render_state.mark_dirty();
                }
                _ => {
                    // Ignore other events (mouse, etc.)
                }
            }
        }

        // Check if app wants to exit
        if app.should_exit {
            break;
        }

        // Yield to tokio runtime
        tokio::task::yield_now().await;
    }

    Ok(())
}

/// Handle a key event
fn handle_key_event(
    app: &mut ChatApp,
    key: KeyEvent,
    _message_tx: &mpsc::UnboundedSender<ChatMessage>,
) -> Result<EventResult> {
    // Global quit shortcuts that bypass normal handling
    if key.code == KeyCode::Char('d') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Ok(EventResult::Quit);
    }

    // Delegate to ChatApp.handle_key() which returns Option<String> for messages
    if let Some(message) = app.handle_key(key) {
        return Ok(EventResult::SendMessage(message));
    }

    Ok(EventResult::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_chat_message_clone() {
        let msg = ChatMessage {
            content: "test message".to_string(),
        };

        let cloned = msg.clone();
        assert_eq!(msg.content, cloned.content);
    }

    #[test]
    fn test_event_result_variants() {
        let quit = EventResult::Quit;
        let cont = EventResult::Continue;
        let send = EventResult::SendMessage("hello".to_string());

        assert!(matches!(quit, EventResult::Quit));
        assert!(matches!(cont, EventResult::Continue));
        match send {
            EventResult::SendMessage(s) => assert_eq!(s, "hello"),
            _ => panic!("expected SendMessage"),
        }
    }

    #[test]
    fn test_handle_key_ctrl_d_quits() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let result = handle_key_event(&mut app, key, &tx).unwrap();

        assert!(matches!(result, EventResult::Quit));
    }

    #[test]
    fn test_handle_key_normal_char() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        let key = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        let result = handle_key_event(&mut app, key, &tx).unwrap();

        assert!(matches!(result, EventResult::Continue));
        assert_eq!(app.input.content(), "h");
    }

    #[test]
    fn test_handle_key_enter_sends_message() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        // Type some content
        for ch in "hello".chars() {
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            handle_key_event(&mut app, key, &tx).unwrap();
        }

        // Press Enter
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = handle_key_event(&mut app, key, &tx).unwrap();

        match result {
            EventResult::SendMessage(content) => assert_eq!(content, "hello"),
            _ => panic!("expected SendMessage"),
        }

        // Input should be cleared
        assert!(app.input.content().is_empty());
    }

    #[test]
    fn test_handle_key_ctrl_c_in_normal_mode_exits() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        handle_key_event(&mut app, key, &tx).unwrap();

        assert!(app.should_exit);
    }

    #[test]
    fn test_handle_key_ctrl_c_with_completion_cancels() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        // Show completion
        app.show_command_completion();
        assert!(app.completion.is_some());

        // Press Ctrl+C
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        handle_key_event(&mut app, key, &tx).unwrap();

        // Completion should be cancelled, not exit
        assert!(app.completion.is_none());
        assert!(!app.should_exit);
    }

    #[tokio::test]
    async fn test_event_loop_components_with_test_backend() {
        // Test that we can create and render with TestBackend
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = ChatApp::new();

        // Should be able to draw
        terminal
            .draw(|frame| {
                render_chat_viewport(&mut app, frame);
            })
            .unwrap();
    }

    // === Agent Integration Tests ===

    use async_trait::async_trait;
    use crucible_core::traits::chat::{
        AgentHandle, ChatError, ChatMode as CoreChatMode, ChatResponse, ChatResult,
        CommandDescriptor,
    };

    /// Mock agent for testing
    struct MockAgent {
        response: String,
        should_error: bool,
    }

    impl MockAgent {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: response.into(),
                should_error: false,
            }
        }

        fn with_error() -> Self {
            Self {
                response: String::new(),
                should_error: true,
            }
        }
    }

    #[async_trait]
    impl AgentHandle for MockAgent {
        async fn send_message(&mut self, _message: &str) -> ChatResult<ChatResponse> {
            if self.should_error {
                Err(ChatError::Communication("Mock error".to_string()))
            } else {
                Ok(ChatResponse {
                    content: self.response.clone(),
                    tool_calls: Vec::new(),
                })
            }
        }

        async fn set_mode(&mut self, _mode: CoreChatMode) -> ChatResult<()> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }
    }

    #[test]
    fn test_agent_response_success() {
        let response = ChatResponse {
            content: "Hello".to_string(),
            tool_calls: Vec::new(),
        };
        let agent_resp = AgentResponse::Success(response);

        match agent_resp {
            AgentResponse::Success(r) => assert_eq!(r.content, "Hello"),
            AgentResponse::Error(_) => panic!("expected Success"),
        }
    }

    #[test]
    fn test_agent_response_error() {
        let agent_resp = AgentResponse::Error("Test error".to_string());

        match agent_resp {
            AgentResponse::Error(e) => assert_eq!(e, "Test error"),
            AgentResponse::Success(_) => panic!("expected Error"),
        }
    }

    #[tokio::test]
    async fn test_mock_agent_sends_response() {
        let mut agent = MockAgent::new("Test response");

        let response = agent.send_message("Hello").await.unwrap();
        assert_eq!(response.content, "Test response");
    }

    #[tokio::test]
    async fn test_mock_agent_error() {
        let mut agent = MockAgent::with_error();

        let result = agent.send_message("Hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_communication_channel() {
        // Test the channel-based agent communication pattern
        let agent = MockAgent::new("Agent response");

        let (message_tx, mut message_rx) = mpsc::unbounded_channel::<String>();
        let (response_tx, mut response_rx) = mpsc::unbounded_channel::<AgentResponse>();

        // Spawn agent task
        tokio::spawn(async move {
            let mut agent = agent;
            while let Some(content) = message_rx.recv().await {
                let response = match agent.send_message(&content).await {
                    Ok(resp) => AgentResponse::Success(resp),
                    Err(e) => AgentResponse::Error(e.to_string()),
                };
                if response_tx.send(response).is_err() {
                    break;
                }
            }
        });

        // Send a message
        message_tx.send("Hello agent".to_string()).unwrap();

        // Receive response
        let response = response_rx.recv().await.unwrap();
        match response {
            AgentResponse::Success(r) => assert_eq!(r.content, "Agent response"),
            AgentResponse::Error(e) => panic!("expected success, got error: {}", e),
        }
    }

    #[test]
    fn test_handle_key_with_agent_ctrl_d_quits() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let result = handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        assert!(matches!(result, EventResult::Quit));
    }

    #[test]
    fn test_handle_key_with_agent_sends_message() {
        let mut app = ChatApp::new();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Type content
        for ch in "hello".chars() {
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();
        }

        // Press Enter
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        // Verify message was sent
        match result {
            EventResult::SendMessage(content) => assert_eq!(content, "hello"),
            _ => panic!("expected SendMessage"),
        }

        // Verify message was sent to channel
        let received = rx.try_recv().unwrap();
        assert_eq!(received, "hello");

        // Verify streaming state is set
        assert!(app.is_streaming);
    }

    #[test]
    fn test_handle_key_with_agent_sets_streaming() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        assert!(!app.is_streaming);

        // Type and send a message
        for ch in "test".chars() {
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();
        }
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        // Streaming should be enabled
        assert!(app.is_streaming);
    }

    #[test]
    fn test_handle_key_with_agent_marks_dirty() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Clear dirty state
        app.render_state.clear();
        assert!(!app.render_state.is_dirty());

        // Type and send a message
        for ch in "test".chars() {
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();
        }
        app.render_state.clear();

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        // Should be marked dirty after sending
        assert!(app.render_state.is_dirty());
    }

    // === /clear Command Tests ===

    /// Helper to set up input with a specific command (bypassing completion for testing)
    fn setup_input_with_command(app: &mut ChatApp, command: &str) {
        app.input.clear();
        app.input.insert_str(command);
    }

    #[test]
    fn test_clear_command_returns_command_handled() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Directly set input to /clear (bypassing completion flow for unit test)
        setup_input_with_command(&mut app, "/clear");
        assert_eq!(app.input.content(), "/clear");

        // Press Enter
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        // Should return CommandHandled, not SendMessage
        assert!(
            matches!(result, EventResult::CommandHandled),
            "Expected CommandHandled for /clear command"
        );
    }

    #[test]
    fn test_clear_command_does_not_send_to_agent() {
        let mut app = ChatApp::new();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Directly set input to /clear
        setup_input_with_command(&mut app, "/clear");

        // Press Enter
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        // Should NOT have sent anything to agent
        assert!(
            rx.try_recv().is_err(),
            "/clear should not send message to agent"
        );
    }

    #[test]
    fn test_clear_command_clears_input() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Directly set input to /clear
        setup_input_with_command(&mut app, "/clear");

        // Press Enter
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        // Input should be cleared
        assert!(app.input.content().is_empty());
    }

    #[test]
    fn test_clear_command_marks_dirty() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Directly set input to /clear
        setup_input_with_command(&mut app, "/clear");

        // Clear dirty state before testing
        app.render_state.clear();
        assert!(!app.render_state.is_dirty());

        // Press Enter
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        // Should be marked dirty
        assert!(app.render_state.is_dirty());
    }

    #[test]
    fn test_exit_command_returns_quit() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Directly set input to /exit
        setup_input_with_command(&mut app, "/exit");

        // Press Enter
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        // Should return Quit
        assert!(matches!(result, EventResult::Quit));
        assert!(app.should_exit);
    }

    #[test]
    fn test_quit_command_returns_quit() {
        let mut app = ChatApp::new();
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Directly set input to /quit
        setup_input_with_command(&mut app, "/quit");

        // Press Enter
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = handle_key_with_agent(&mut app, key, &tx, &mut terminal).unwrap();

        // Should return Quit
        assert!(matches!(result, EventResult::Quit));
    }

    #[test]
    fn test_event_result_command_handled() {
        let handled = EventResult::CommandHandled;
        assert!(matches!(handled, EventResult::CommandHandled));
    }

    #[test]
    fn test_handle_clear_command_directly() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = ChatApp::new();

        // Type something in input first
        for ch in "some text".chars() {
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            app.handle_key(key);
        }
        assert!(!app.input.content().is_empty());

        // Call handle_clear_command
        handle_clear_command(&mut terminal, &mut app).unwrap();

        // Input should be cleared
        assert!(app.input.content().is_empty());

        // Dirty flag should be set
        assert!(app.render_state.is_dirty());
    }
}
