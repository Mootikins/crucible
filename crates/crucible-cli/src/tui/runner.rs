//! TUI runner - main event loop for terminal UI
//!
//! Coordinates terminal input, event polling, and rendering.
//! Uses ratatui with alternate screen for full viewport control.
//!
//! ## Debug Logging
//!
//! To see full output including prompts and responses:
//! ```bash
//! RUST_LOG=crucible_cli::tui::runner=debug cru chat
//! tail -f ~/.crucible/chat.log  # in another terminal
//! ```

use tracing::debug;

use crate::chat::bridge::AgentEventBridge;
use crate::chat::slash_registry::SlashCommandRegistry;
use crate::tui::agent_picker::AgentSelection;
use crate::tui::notification::NotificationLevel;
use crate::tui::streaming_channel::{
    create_streaming_channel, StreamingEvent, StreamingReceiver, StreamingTask,
};

use crate::tui::conversation::StatusKind;
use crate::tui::conversation_view::{ConversationView, RatatuiView};
use crate::tui::{
    map_key_event, ContentBlock, DynamicPopupProvider, InputAction, ParseEvent, PopupProvider,
    StreamingParser, TuiState,
};
use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, MouseEvent, MouseEventKind},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use crucible_core::events::{FileChangeKind, SessionEvent};
use crucible_core::traits::chat::AgentHandle;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

/// TUI runner with full ratatui control and new conversation styling.
///
/// Uses alternate screen mode with:
/// - Inverted user messages
/// - Clean tool call display
/// - Status bar below input
/// - Mouse scroll support
pub struct RatatuiRunner {
    view: RatatuiView,
    popup_provider: std::sync::Arc<DynamicPopupProvider>,
    /// Token count from current/last response
    token_count: usize,
    /// Previous token count for direction indicator
    prev_token_count: usize,
    /// Spinner animation frame (cycles 0-3)
    spinner_frame: usize,
    /// Animation frame counter for timing (60fps loop)
    animation_frame: usize,
    /// Track if we're currently streaming
    is_streaming: bool,
    /// Track Ctrl+C for double-press exit
    ctrl_c_count: u8,
    last_ctrl_c: Option<std::time::Instant>,
    /// Popup state
    popup: Option<crate::tui::state::PopupState>,
    /// Background streaming task
    streaming_task: Option<tokio::task::JoinHandle<()>>,
    /// Channel receiver for streaming events
    streaming_rx: Option<StreamingReceiver>,
    /// Streaming parser for incremental markdown parsing
    streaming_parser: Option<StreamingParser>,
    /// Receiver for agent availability probing results
    agent_probe_rx: Option<tokio::sync::oneshot::Receiver<Vec<crucible_acp::KnownAgent>>>,
    /// Command history (most recent last)
    history: Vec<String>,
    /// Current position in history (None = not browsing history)
    history_index: Option<usize>,
    /// Saved input when entering history mode
    history_saved_input: String,
    /// Current agent name (for display in /agent command)
    current_agent: Option<String>,
    /// Command registry for slash command lookup
    command_registry: std::sync::Arc<SlashCommandRegistry>,
    /// If true, session should restart with agent picker instead of exiting
    restart_requested: bool,
    /// If true, this runner supports restart via /new command
    supports_restart: bool,
    /// Pre-selected agent for first iteration (skips picker, still allows /new)
    default_selection: Option<AgentSelection>,
}

impl RatatuiRunner {
    /// Create a new ratatui-based TUI runner.
    pub fn new(
        mode_id: &str,
        popup_provider: std::sync::Arc<DynamicPopupProvider>,
        command_registry: std::sync::Arc<SlashCommandRegistry>,
    ) -> Result<Self> {
        let (width, height) = size().unwrap_or((80, 24));

        Ok(Self {
            view: RatatuiView::new(mode_id, width, height),
            popup_provider,
            token_count: 0,
            prev_token_count: 0,
            spinner_frame: 0,
            animation_frame: 0,
            is_streaming: false,
            ctrl_c_count: 0,
            last_ctrl_c: None,
            popup: None,
            streaming_task: None,
            streaming_rx: None,
            streaming_parser: None,
            agent_probe_rx: None,
            history: Vec::new(),
            history_index: None,
            history_saved_input: String::new(),
            current_agent: None,
            command_registry,
            restart_requested: false,
            supports_restart: false, // Set to true when using run_with_factory
            default_selection: None,
        })
    }

    /// Skip the splash screen (e.g., when agent was pre-specified via CLI)
    pub fn skip_splash(&mut self) {
        self.view.dismiss_splash();
    }

    /// Set a default agent selection for the first iteration.
    ///
    /// When set, skips the picker phase on first run but still supports
    /// restart via `/new` command (which will show the picker).
    pub fn with_default_selection(&mut self, selection: AgentSelection) -> &mut Self {
        self.default_selection = Some(selection);
        self
    }

    /// Set the current agent name for display in /agent command
    pub fn set_current_agent(&mut self, name: &str) {
        self.current_agent = Some(name.to_string());
    }

    /// Get the current agent name
    pub fn current_agent_name(&self) -> Option<&str> {
        self.current_agent.as_deref()
    }

    /// Check if a restart was requested (e.g., via /new command)
    pub fn restart_requested(&self) -> bool {
        self.restart_requested
    }

    /// Internal main loop.
    async fn main_loop<A: AgentHandle>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        bridge: &AgentEventBridge,
        agent: &mut A,
    ) -> Result<()> {
        let mut popup_debounce =
            crate::tui::popup::PopupDebounce::new(std::time::Duration::from_millis(50));
        let mut last_seen_seq = 0u64;

        loop {
            // 1. Start agent probing if splash needs it
            if self.view.splash_needs_probing() && self.agent_probe_rx.is_none() {
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.agent_probe_rx = Some(rx);
                tokio::spawn(async move {
                    let agents = crucible_acp::probe_all_agents().await;
                    let _ = tx.send(agents);
                });
            }

            // 2. Check for agent probing results
            if let Some(rx) = &mut self.agent_probe_rx {
                if let Ok(agents) = rx.try_recv() {
                    self.view.update_splash_availability(agents);
                    self.agent_probe_rx = None;
                }
            }

            // 3. Render
            terminal.draw(|f| self.view.render_frame(f))?;

            // 4. Poll events (non-blocking, ~60fps)
            if event::poll(Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => {
                        if self.handle_key_event(&key, bridge, agent).await? {
                            break;
                        }
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse_event(&mouse);
                    }
                    Event::Resize(width, height) => {
                        self.view.handle_resize(width, height)?;
                    }
                    _ => {}
                }
            }

            // 5. Refresh popup items and sync with view
            if let Some(ref mut popup) = self.popup {
                if popup_debounce.ready() {
                    let items = self.popup_provider.provide(popup.kind, &popup.query);
                    let selected = popup.selected.min(items.len().saturating_sub(1));
                    popup.items = items;
                    popup.selected = selected;
                    popup.update_viewport(5); // MAX_POPUP_ITEMS
                }
            }
            // Sync popup state to view for rendering
            self.view.set_popup(self.popup.clone());

            // 6. Poll ring buffer for session events
            self.poll_session_events(bridge, &mut last_seen_seq);

            // 7. Poll streaming channel (non-blocking)
            let mut pending_parse_events = Vec::new();
            let mut streaming_complete = false;
            let mut streaming_error = None;

            if let Some(rx) = &mut self.streaming_rx {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        StreamingEvent::Delta { text, seq: _ } => {
                            self.prev_token_count = self.token_count;
                            self.token_count += 1;
                            self.spinner_frame = self.spinner_frame.wrapping_add(1);
                            self.view.set_status(StatusKind::Generating {
                                token_count: self.token_count,
                                prev_token_count: self.prev_token_count,
                                spinner_frame: self.spinner_frame,
                            });

                            // Feed delta through parser
                            if let Some(parser) = &mut self.streaming_parser {
                                let parse_events = parser.feed(&text);
                                pending_parse_events.extend(parse_events);
                            }

                            bridge.ring.push(SessionEvent::TextDelta {
                                delta: text,
                                seq: self.token_count as u64,
                            });
                        }
                        StreamingEvent::Done { full_response } => {
                            self.is_streaming = false;
                            self.view.clear_status();
                            debug!(response_len = full_response.len(), "Streaming complete");

                            // Finalize parser
                            if let Some(parser) = &mut self.streaming_parser {
                                let parse_events = parser.finalize();
                                pending_parse_events.extend(parse_events);
                            }

                            streaming_complete = true;

                            bridge.ring.push(SessionEvent::AgentResponded {
                                content: full_response,
                                tool_calls: vec![],
                            });
                        }
                        StreamingEvent::Error { message } => {
                            self.is_streaming = false;
                            streaming_error = Some(message);
                        }
                        StreamingEvent::ToolCall { id: _, name, args } => {
                            // Display tool call in the TUI
                            self.view.push_tool_running(&name);
                            self.view.set_status_text(&format!("Running: {}", name));

                            // Push to event ring for session tracking
                            bridge.ring.push(SessionEvent::ToolCalled {
                                name,
                                args,
                            });
                        }
                    }
                }
            }

            // Apply parse events after borrow of streaming_rx is released
            if !pending_parse_events.is_empty() {
                self.apply_parse_events(pending_parse_events);
            }

            // Handle streaming completion
            if streaming_complete {
                self.streaming_parser = None;
                self.view.complete_assistant_streaming();
            }

            // Handle streaming error
            if let Some(message) = streaming_error {
                self.view.clear_status();
                self.view.set_status_text(&format!("Error: {}", message));
                self.streaming_parser = None;
            }

            // 6. Poll streaming task for completion (cleanup)
            if let Some(task) = &mut self.streaming_task {
                if task.is_finished() {
                    let task = self.streaming_task.take().unwrap();
                    let _ = task.await; // Just cleanup, events already processed
                    self.streaming_rx = None;
                }
            }

            // 7. Animate spinner during thinking phase (before tokens arrive)
            // The loop runs at ~60fps (16ms). Animate spinner every ~6 frames (~100ms).
            self.animation_frame = self.animation_frame.wrapping_add(1);
            if self.is_streaming && self.token_count == 0 && self.animation_frame % 6 == 0 {
                self.spinner_frame = self.spinner_frame.wrapping_add(1);
                self.view
                    .set_status(StatusKind::Thinking { spinner_frame: self.spinner_frame });
            }
        }

        // Ensure streaming task completes before exiting
        if let Some(task) = self.streaming_task.take() {
            let _ = task.await;
        }
        self.streaming_rx = None;

        Ok(())
    }

    /// Handle keyboard input.
    async fn handle_key_event<A: AgentHandle>(
        &mut self,
        key: &crossterm::event::KeyEvent,
        bridge: &AgentEventBridge,
        agent: &mut A,
    ) -> Result<bool> {
        use crossterm::event::KeyCode;

        // Dialog takes priority over all other input
        if self.view.has_dialog() {
            if let Some(result) = self.view.handle_dialog_key(*key) {
                self.handle_dialog_result(result)?;
            }
            return Ok(false);
        }

        // Special handling when splash is shown
        if self.view.is_showing_splash() {
            match key.code {
                // Vim-style navigation: k=up, j=down
                KeyCode::Up | KeyCode::Char('k') => {
                    self.view.splash_select_prev();
                    return Ok(false);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.view.splash_select_next();
                    return Ok(false);
                }
                // Quick access by number (1-indexed)
                KeyCode::Char(c @ '1'..='9') => {
                    let index = (c as usize) - ('1' as usize);
                    self.view.splash_select_index(index);
                    return Ok(false);
                }
                // Confirm with Enter, Space, or 'l' (vim-style right/accept)
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') => {
                    if let Some(_agent_name) = self.view.splash_confirm() {
                        // NOTE: Agent is already created before TUI starts. To support
                        // splash agent selection, we would need to defer agent creation
                        // until after selection. For now, use --agent CLI flag instead.
                        // The splash shows available agents but doesn't switch agents.
                        self.view.dismiss_splash();
                    }
                    return Ok(false);
                }
                // Exit with Esc, 'q', or 'h' (vim-style left/back)
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h') => {
                    return Ok(true);
                }
                _ => return Ok(false),
            }
        }

        // Build a minimal TuiState for key mapping (we'll migrate away from this)
        let mut temp_state = TuiState::new(self.view.mode_id());
        temp_state.input_buffer = self.view.input().to_string();
        temp_state.cursor_position = self.view.cursor_position();
        temp_state.ctrl_c_count = self.ctrl_c_count;
        temp_state.last_ctrl_c = self.last_ctrl_c;
        let action = map_key_event(key, &temp_state);

        match action {
            InputAction::Exit => {
                return Ok(true);
            }
            InputAction::Cancel => {
                if self.is_streaming {
                    // Cancel streaming
                    self.is_streaming = false;
                    self.view.clear_status();
                    self.view.set_status_text("Cancelled");
                } else {
                    // Clear input or track Ctrl+C for double-press exit
                    self.ctrl_c_count += 1;
                    self.last_ctrl_c = Some(std::time::Instant::now());

                    if self.ctrl_c_count >= 2 {
                        if let Some(last) = self.last_ctrl_c {
                            if last.elapsed() < Duration::from_millis(500) {
                                return Ok(true); // Exit on double Ctrl+C
                            }
                        }
                    }

                    self.view.set_input("");
                    self.view.set_cursor_position(0);
                    self.popup = None;
                }
            }
            InputAction::SendMessage(msg) => {
                // Reset Ctrl+C tracking
                self.ctrl_c_count = 0;

                // Add to history (avoid duplicates for repeated commands)
                if self.history.last().map_or(true, |last| last != &msg) {
                    self.history.push(msg.clone());
                }
                self.history_index = None;
                self.history_saved_input.clear();

                // Clear input IMMEDIATELY (before any async work)
                self.view.set_input("");
                self.view.set_cursor_position(0);
                self.popup = None;

                // Add user message to view
                self.view.push_user_message(&msg)?;
                debug!(prompt = %msg, "User message sent");

                // Set thinking status
                self.is_streaming = true;
                self.token_count = 0;
                self.prev_token_count = 0;
                self.spinner_frame = 0;
                self.view.set_status(StatusKind::Thinking { spinner_frame: 0 });
                self.view.set_status_text("Thinking");

                // Initialize streaming parser and start streaming message in view
                self.streaming_parser = Some(StreamingParser::new());
                self.view.start_assistant_streaming();

                // Emit user message to ring
                bridge.ring.push(SessionEvent::MessageReceived {
                    content: msg.clone(),
                    participant_id: "user".to_string(),
                });

                // Create channel and spawn streaming task
                let (tx, rx) = create_streaming_channel();

                // Stream is now 'static after API change - no unsafe needed!
                let stream = agent.send_message_stream(msg.clone());
                let task = StreamingTask::spawn(tx, stream);

                self.streaming_task = Some(task);
                self.streaming_rx = Some(rx);
            }
            InputAction::InsertChar(c) => {
                let mut input = self.view.input().to_string();
                let pos = self.view.cursor_position();
                input.insert(pos, c);
                self.view.set_input(&input);
                self.view.set_cursor_position(pos + c.len_utf8());
                self.update_popup();
            }
            InputAction::DeleteChar => {
                let input = self.view.input().to_string();
                let pos = self.view.cursor_position();
                if pos > 0 {
                    let prev = input[..pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    let new_input = format!("{}{}", &input[..prev], &input[pos..]);
                    self.view.set_input(&new_input);
                    self.view.set_cursor_position(prev);
                    self.update_popup();
                }
            }
            InputAction::InsertNewline => {
                let mut input = self.view.input().to_string();
                let pos = self.view.cursor_position();
                input.insert(pos, '\n');
                self.view.set_input(&input);
                self.view.set_cursor_position(pos + 1);
            }
            InputAction::MoveCursorLeft => {
                let input = self.view.input();
                let pos = self.view.cursor_position();
                if pos > 0 {
                    let new_pos = input[..pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.view.set_cursor_position(new_pos);
                }
            }
            InputAction::MoveCursorRight => {
                let input = self.view.input();
                let pos = self.view.cursor_position();
                if pos < input.len() {
                    let new_pos = input[pos..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| pos + i)
                        .unwrap_or(input.len());
                    self.view.set_cursor_position(new_pos);
                }
            }
            InputAction::CycleMode => {
                let new_mode = crucible_core::traits::chat::cycle_mode_id(self.view.mode_id());
                self.view.set_mode_id(new_mode);
            }
            InputAction::ScrollUp => {
                self.view.scroll_up(3);
            }
            InputAction::ScrollDown => {
                self.view.scroll_down(3);
            }
            InputAction::PageUp => {
                self.view.scroll_up(10);
            }
            InputAction::PageDown => {
                self.view.scroll_down(10);
            }
            InputAction::HalfPageUp => {
                self.view.scroll_up(5);
            }
            InputAction::HalfPageDown => {
                self.view.scroll_down(5);
            }
            InputAction::ScrollToTop => {
                self.view.scroll_to_top();
            }
            InputAction::ScrollToBottom => {
                self.view.scroll_to_bottom();
            }
            InputAction::MovePopupSelection(delta) => {
                if let Some(ref mut popup) = self.popup {
                    popup.move_selection(delta);
                    popup.update_viewport(5); // MAX_POPUP_ITEMS
                }
            }
            InputAction::ConfirmPopup => {
                if let Some(ref popup) = self.popup {
                    if !popup.items.is_empty() {
                        let idx = popup.selected.min(popup.items.len() - 1);
                        let token = popup.items[idx].token.clone();
                        self.view.set_input(&token);
                        self.view.set_cursor_position(token.len());
                    }
                }
                self.popup = None;
            }
            InputAction::ExecuteSlashCommand(cmd) => {
                // Extract command name and args, route via registry
                use crate::tui::popup::extract_command_name;

                if let Some(cmd_name) = extract_command_name(&cmd) {
                    // Extract args (everything after /command)
                    let args = cmd
                        .strip_prefix("/")
                        .and_then(|s| s.strip_prefix(cmd_name))
                        .map(|s| s.trim())
                        .unwrap_or("");

                    // Look up command in registry
                    if let Some(descriptor) = self.command_registry.get_descriptor(cmd_name) {
                        // For now, handle TUI-specific implementations inline
                        // TODO: Phase 2 - use TuiChatContext and call handler.execute()
                        match cmd_name {
                            "help" => {
                                let help_text = "Shortcuts: Shift+Tab=mode, Ctrl+C=cancel, ↑↓=scroll, @=context, /=commands";
                                self.view.set_status_text(help_text);
                            }
                            "clear" => {
                                self.view.state_mut().conversation.clear();
                                self.view.set_status_text("Conversation cleared");
                            }
                            "mode" | "plan" | "act" | "auto" => {
                                self.view.set_status_text("Use Shift+Tab to switch modes");
                            }
                            "search" => {
                                if args.is_empty() {
                                    // Show input hint from registry if available
                                    let hint = descriptor.input_hint.as_deref().unwrap_or("<query>");
                                    self.view.set_status_text(&format!("Usage: /search {} — or just type your search in chat", hint));
                                } else {
                                    let search_prompt = format!("Search my notes for: {}", args);
                                    self.view.set_input(&search_prompt);
                                    self.view.set_cursor_position(search_prompt.len());
                                    self.view.set_status_text("Press Enter to search");
                                    return Ok(false);
                                }
                            }
                            "context" => {
                                self.view.set_status_text("Use @note:<path> or @file:<path> to inject context");
                            }
                            "exit" | "quit" => {
                                return Ok(true);
                            }
                            "agent" => {
                                let current = self.current_agent_name().unwrap_or("unknown");
                                if args.is_empty() {
                                    // Show current agent
                                    self.view.set_status_text(&format!("Current agent: {}. Use /new to start fresh session with agent picker", current));
                                } else {
                                    // Suggest /new for switching
                                    self.view.set_status_text(&format!("Use /new to start a new session. Current agent: {}", current));
                                }
                            }
                            "new" => {
                                if self.supports_restart {
                                    // Request restart with agent picker
                                    self.restart_requested = true;
                                    // Clear popup/input state before restart (so splash can render)
                                    self.popup = None;
                                    self.view.set_popup(None);
                                    self.view.set_input("");
                                    self.view.set_cursor_position(0);
                                    self.view.set_status_text("Starting new session...");
                                    return Ok(true); // Exit to trigger restart
                                } else {
                                    // Can't restart without a factory
                                    self.view.set_status_text("/new requires --lazy-agent-selection or deferred mode");
                                }
                            }
                            _ => {
                                // Command exists in registry but no TUI handler yet
                                self.view.set_status_text(&format!("{}: {}", cmd_name, descriptor.description));
                            }
                        }
                    } else {
                        // Not in registry - could be agent command or unknown
                        self.view.set_status_text(&format!("Unknown command: /{}", cmd_name));
                    }
                }

                // Clear input after executing
                self.view.set_input("");
                self.view.set_cursor_position(0);
                self.popup = None;
                // Sync popup to view immediately (needed for /new to show splash)
                self.view.set_popup(None);
            }
            InputAction::HistoryPrev => {
                if !self.history.is_empty() {
                    let new_index = match self.history_index {
                        None => {
                            // Entering history mode - save current input
                            self.history_saved_input = self.view.input().to_string();
                            self.history.len() - 1
                        }
                        Some(0) => 0, // Already at oldest
                        Some(i) => i - 1,
                    };
                    self.history_index = Some(new_index);
                    if let Some(cmd) = self.history.get(new_index) {
                        self.view.set_input(cmd);
                        self.view.set_cursor_position(cmd.len());
                    }
                }
            }
            InputAction::HistoryNext => {
                if let Some(current) = self.history_index {
                    if current + 1 >= self.history.len() {
                        // Exiting history mode - restore saved input
                        self.history_index = None;
                        self.view.set_input(&self.history_saved_input);
                        self.view.set_cursor_position(self.history_saved_input.len());
                    } else {
                        let new_index = current + 1;
                        self.history_index = Some(new_index);
                        if let Some(cmd) = self.history.get(new_index) {
                            self.view.set_input(cmd);
                            self.view.set_cursor_position(cmd.len());
                        }
                    }
                }
            }
            InputAction::None => {}
        }

        Ok(false)
    }

    /// Handle mouse events for scrolling.
    fn handle_mouse_event(&mut self, mouse: &MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.view.scroll_up(3);
            }
            MouseEventKind::ScrollDown => {
                self.view.scroll_down(3);
            }
            _ => {}
        }
    }

    /// Poll session events from the ring buffer.
    fn poll_session_events(&mut self, bridge: &AgentEventBridge, last_seen_seq: &mut u64) {
        let events: Vec<_> = bridge
            .ring
            .range(*last_seen_seq, bridge.ring.write_sequence())
            .collect();
        *last_seen_seq = bridge.ring.write_sequence();

        for event in events {
            // Handle notification events
            Self::handle_notification_event(&mut self.view.state_mut().notifications, &event);

            match &*event {
                SessionEvent::TextDelta { delta, .. } => {
                    // Update token count and status
                    self.prev_token_count = self.token_count;
                    self.token_count += delta.split_whitespace().count();
                    self.spinner_frame = self.spinner_frame.wrapping_add(1);
                    self.view.set_status(StatusKind::Generating {
                        token_count: self.token_count,
                        prev_token_count: self.prev_token_count,
                        spinner_frame: self.spinner_frame,
                    });
                    self.view.set_status_text("Generating");
                    self.view.set_token_count(Some(self.token_count));
                }
                SessionEvent::AgentResponded {
                    content: _,
                    tool_calls: _,
                } => {
                    // Streaming complete - message already built via streaming channel
                    // Don't add another message here to avoid duplicates
                    self.is_streaming = false;
                    self.view.clear_status();
                    self.view.set_status_text("Ready");
                }
                SessionEvent::ToolCalled { name, args: _ } => {
                    self.view.push_tool_running(name);
                    self.view.set_status_text(&format!("Running: {}", name));
                }
                SessionEvent::ToolCompleted {
                    name,
                    result,
                    error,
                } => {
                    if let Some(err) = error {
                        self.view.error_tool(name, err);
                    } else {
                        // Truncate result for summary
                        let summary = if result.len() > 50 {
                            Some(format!("{}...", &result[..47]))
                        } else if !result.is_empty() {
                            Some(result.clone())
                        } else {
                            None
                        };
                        self.view.complete_tool(name, summary);
                    }
                }
                _ => {}
            }
        }

        // Update notification state after processing all events
        self.view.state_mut().notifications.tick();
    }

    /// Update popup based on current input.
    fn update_popup(&mut self) {
        use crate::tui::state::{PopupKind, PopupState};

        let input = self.view.input();
        let trimmed = input.trim_start();

        if trimmed.starts_with('/') {
            let query = trimmed.strip_prefix('/').unwrap_or("").to_string();
            if self.popup.as_ref().map(|p| p.kind) != Some(PopupKind::Command) {
                self.popup = Some(PopupState::new(PopupKind::Command));
            }
            if let Some(ref mut popup) = self.popup {
                popup.query = query;
            }
        } else if trimmed.starts_with('@') {
            let query = trimmed.strip_prefix('@').unwrap_or("").to_string();
            if self.popup.as_ref().map(|p| p.kind) != Some(PopupKind::AgentOrFile) {
                self.popup = Some(PopupState::new(PopupKind::AgentOrFile));
            }
            if let Some(ref mut popup) = self.popup {
                popup.query = query;
            }
        } else {
            self.popup = None;
        }
    }

    /// Get the view state for testing.
    pub fn view(&self) -> &RatatuiView {
        &self.view
    }

    /// Handle dialog result
    fn handle_dialog_result(&mut self, result: crate::tui::dialog::DialogResult) -> Result<()> {
        use crate::tui::dialog::DialogResult;

        match result {
            DialogResult::Confirm(value) => {
                // Handle confirmation based on context
                // For now, just log that a dialog was confirmed
                // In the future, this could emit events or call callbacks
                self.view
                    .set_status_text(&format!("Dialog confirmed: {}", value));
            }
            DialogResult::Cancel => {
                // Dialog was cancelled
                self.view.set_status_text("Dialog cancelled");
            }
            DialogResult::Pending => {
                // Still active (shouldn't happen after handle_key)
            }
        }
        Ok(())
    }

    /// Apply parse events to the view (converts events to content blocks)
    fn apply_parse_events(&mut self, events: Vec<ParseEvent>) {
        for event in events {
            match event {
                ParseEvent::Text(text) => {
                    // Append to existing prose block if possible, otherwise create new
                    // This consolidates streaming text into continuous prose
                    self.view.append_or_create_prose(&text);
                }
                ParseEvent::CodeBlockStart { lang } => {
                    // Start a new partial code block
                    self.view
                        .append_streaming_blocks(vec![ContentBlock::code_partial(lang, "")]);
                }
                ParseEvent::CodeBlockContent(content) => {
                    // Append to the existing code block in the view
                    self.view.append_to_last_block(&content);
                }
                ParseEvent::CodeBlockEnd => {
                    // Mark the code block as complete
                    self.view.complete_last_block();
                }
            }
        }
    }

    // =========================================================================
    // Deferred Agent Creation Support
    // =========================================================================

    /// Run the TUI with deferred agent creation.
    ///
    /// This method:
    /// 1. Enters the TUI (alternate screen)
    /// 2. If splash is active, runs picker loop and returns selection
    /// 3. Calls the provided factory to create the agent (while showing status)
    /// 4. Runs the main chat loop
    /// 5. Cleans up and exits TUI
    ///
    /// The factory receives the agent selection and should create the agent.
    /// Status updates are shown in the TUI during creation.
    ///
    /// Supports `/new` command for restarting with a different agent - the factory
    /// is called again with the new selection and conversation is cleared.
    pub async fn run_with_factory<F, Fut, A>(
        &mut self,
        bridge: &AgentEventBridge,
        create_agent: F,
    ) -> Result<()>
    where
        F: Fn(AgentSelection) -> Fut,
        Fut: std::future::Future<Output = Result<A>>,
        A: AgentHandle,
    {
        // Mark that we support restart (factory allows creating new agents)
        self.supports_restart = true;

        // Enter TUI
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Main session loop - supports restart via /new command
        loop {
            // Reset restart flag at start of each iteration
            self.restart_requested = false;

            // Phase 1: Agent selection
            // Use default_selection on first iteration (skips picker), show picker on restart
            let selection = if let Some(default) = self.default_selection.take() {
                // First iteration with pre-specified agent: skip picker
                self.view.dismiss_splash(); // Ensure splash is hidden for input
                default
            } else {
                // No default or restart: show picker
                self.view.show_splash();
                self.run_picker_phase(&mut terminal).await?
            };

            // Handle cancellation
            if matches!(selection, AgentSelection::Cancelled) {
                break; // Exit the loop and cleanup
            }

            // Phase 2: Create agent (show status in TUI)
            self.view.set_status_text("Creating agent...");
            self.render_frame(&mut terminal)?;

            // Extract agent name from selection before consuming it
            let agent_name = match &selection {
                AgentSelection::Acp(name) => name.clone(),
                AgentSelection::Internal => "internal".to_string(),
                AgentSelection::Cancelled => "unknown".to_string(), // shouldn't reach here
            };

            let mut agent = create_agent(selection).await?;

            // Set current agent for /agent command
            self.set_current_agent(&agent_name);

            // Clear conversation for fresh start
            self.view.state_mut().conversation.clear();
            self.view.set_status_text("Ready");
            self.render_frame(&mut terminal)?;

            // Phase 3: Run main loop
            self.main_loop(&mut terminal, bridge, &mut agent).await?;

            // Check if restart was requested (via /new command)
            if !self.restart_requested {
                break; // Normal exit, don't restart
            }

            // Restart requested - loop back to show picker again
            // Clear conversation BEFORE showing picker (is_showing_splash checks this)
            self.view.state_mut().conversation.clear();
            self.view.set_status_text("Restarting session...");
            self.render_frame(&mut terminal)?;
        }

        // Cleanup
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            crossterm::event::DisableMouseCapture,
            LeaveAlternateScreen,
            cursor::Show
        )?;

        Ok(())
    }

    /// Run the picker phase - shows splash and waits for agent selection.
    async fn run_picker_phase(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<AgentSelection> {
        use crossterm::event::KeyCode;

        // Start agent probing
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let agents = crucible_acp::probe_all_agents().await;
            let _ = tx.send(agents);
        });

        loop {
            // Check for probe results
            if let Ok(agents) = rx.try_recv() {
                self.view.update_splash_availability(agents);
            }

            // Render
            self.render_frame(terminal)?;

            // Handle input
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            self.view.splash_select_prev();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            self.view.splash_select_next();
                        }
                        KeyCode::Char(c @ '1'..='9') => {
                            let index = (c as usize) - ('1' as usize);
                            self.view.splash_select_index(index);
                        }
                        KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') => {
                            if let Some(agent_name) = self.view.splash_confirm() {
                                self.view.dismiss_splash();
                                let selection = if agent_name == "internal" {
                                    AgentSelection::Internal
                                } else {
                                    AgentSelection::Acp(agent_name)
                                };
                                return Ok(selection);
                            }
                        }
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h') => {
                            return Ok(AgentSelection::Cancelled);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Render a single frame (used during status updates).
    fn render_frame(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        terminal.draw(|f| self.view.render_frame(f))?;
        Ok(())
    }

    /// Handle a session event for notifications
    fn handle_notification_event(
        notifications: &mut crate::tui::notification::NotificationState,
        event: &SessionEvent,
    ) {
        match event {
            SessionEvent::FileChanged { path, .. } => {
                notifications.push_change(path.clone());
            }
            SessionEvent::FileDeleted { path } => {
                notifications.push_change(path.clone());
            }
            SessionEvent::EmbeddingFailed { error, .. } => {
                notifications.push_error(error.clone());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::slash_registry::SlashCommandRegistryBuilder;
    use crucible_core::RegistryBuilder;

    fn test_popup_provider() -> std::sync::Arc<DynamicPopupProvider> {
        std::sync::Arc::new(DynamicPopupProvider::new())
    }

    fn test_command_registry() -> std::sync::Arc<SlashCommandRegistry> {
        std::sync::Arc::new(SlashCommandRegistryBuilder::default().build())
    }

    #[test]
    fn test_tui_state_creates_correctly() {
        let state = TuiState::new("plan");
        assert!(!state.should_exit);
        assert!(state.streaming.is_none());
    }

    #[test]
    fn test_ratatui_runner_creates() {
        let runner = RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();
        assert_eq!(runner.view().mode_id(), "plan");
    }

    #[test]
    fn test_runner_tracks_current_agent() {
        let mut runner = RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

        // Default should be None (unknown)
        assert!(runner.current_agent_name().is_none());

        // Can set current agent
        runner.set_current_agent("internal");
        assert_eq!(runner.current_agent_name(), Some("internal"));

        runner.set_current_agent("opencode");
        assert_eq!(runner.current_agent_name(), Some("opencode"));
    }

    #[tokio::test]
    async fn test_runner_components_create() {
        use crate::chat::bridge::AgentEventBridge;
        use crucible_rune::SessionBuilder;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let session = SessionBuilder::new("test-runner")
            .with_folder(temp.path())
            .build();

        let ring = session.ring().clone();
        let bridge = AgentEventBridge::new(session.handle(), ring.clone());

        assert!(ring.is_empty());
        assert!(bridge.handle.session_id().contains("test-runner"));
    }

    #[test]
    fn test_ctrl_c_cancels_during_streaming() {
        use crate::tui::StreamingBuffer;

        let mut state = TuiState::new("plan");
        state.streaming = Some(StreamingBuffer::new());

        let action = InputAction::Cancel;
        let is_streaming = state.streaming.is_some();

        assert!(matches!(action, InputAction::Cancel));
        assert!(is_streaming);
    }

    #[test]
    fn test_esc_maps_to_cancel() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Cancel);
    }

    #[test]
    fn test_esc_cancels_during_streaming() {
        use crate::tui::StreamingBuffer;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut state = TuiState::new("plan");
        state.streaming = Some(StreamingBuffer::new());

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Cancel);
        assert!(state.streaming.is_some());
    }

    #[test]
    fn test_cancel_with_empty_streaming() {
        use crate::tui::StreamingBuffer;

        let mut state = TuiState::new("plan");
        state.streaming = Some(StreamingBuffer::new());

        let buf = state.streaming.take().unwrap();
        let remaining = buf.all_content();

        assert!(remaining.is_empty());
    }

    #[test]
    fn test_cancel_not_streaming_clears_input() {
        let mut state = TuiState::new("plan");
        state.input_buffer = "some text".to_string();
        state.cursor_position = 9;

        state.execute_action(InputAction::Cancel);

        assert!(state.input_buffer.is_empty());
        assert_eq!(state.cursor_position, 0);
    }

    #[test]
    fn test_status_error_preserved_until_new_message() {
        let mut state = TuiState::new("plan");

        state.status_error = Some("Connection failed".to_string());

        state.execute_action(InputAction::InsertChar('a'));
        assert!(state.status_error.is_some());

        state.execute_action(InputAction::SendMessage("test".to_string()));
        assert!(state.status_error.is_none());
    }

    #[test]
    fn test_ratatui_view_scroll() {
        let runner = RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

        // View should be accessible
        assert_eq!(runner.view().state().scroll_offset, 0);
    }

    #[test]
    fn test_runner_notification_from_file_changed() {
        use crate::tui::notification::NotificationState;
        use crucible_core::events::{FileChangeKind, SessionEvent};
        use std::path::PathBuf;

        let mut notifications = NotificationState::new();

        let event = SessionEvent::FileChanged {
            path: PathBuf::from("/notes/test.md"),
            kind: FileChangeKind::Modified,
        };

        RatatuiRunner::handle_notification_event(&mut notifications, &event);

        assert!(!notifications.is_empty());
    }

    #[test]
    fn test_runner_notification_from_embedding_failed() {
        use crate::tui::notification::{NotificationLevel, NotificationState};
        use crucible_core::events::SessionEvent;

        let mut notifications = NotificationState::new();

        let event = SessionEvent::EmbeddingFailed {
            entity_id: "note:test".into(),
            block_id: None,
            error: "connection timeout".into(),
        };

        RatatuiRunner::handle_notification_event(&mut notifications, &event);

        let result = notifications.render_tick();
        assert!(result.is_some());
        let (msg, level) = result.unwrap();
        assert!(matches!(level, NotificationLevel::Error));
        assert!(msg.contains("connection timeout") || msg.contains("error"));
    }

    #[test]
    fn test_runner_default_selection_initially_none() {
        let runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();
        // default_selection is private, but we can verify behavior indirectly
        // by checking that supports_restart is false initially
        assert!(!runner.supports_restart);
    }

    #[test]
    fn test_runner_with_default_selection_sets_value() {
        let mut runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

        // Set a default selection
        runner.with_default_selection(AgentSelection::Acp("opencode".to_string()));

        // Verify it was set (we can check the field exists by ensuring no panic)
        // The actual behavior is tested in integration tests
        assert!(runner.default_selection.is_some());
    }

    #[test]
    fn test_runner_with_default_selection_returns_self() {
        let mut runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

        // Fluent interface should return &mut Self
        let result = runner.with_default_selection(AgentSelection::Internal);
        result.set_current_agent("internal");

        assert_eq!(runner.current_agent_name(), Some("internal"));
    }

    #[test]
    fn test_runner_default_selection_consumed_on_take() {
        let mut runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

        runner.with_default_selection(AgentSelection::Acp("test".to_string()));
        assert!(runner.default_selection.is_some());

        // Simulate what run_with_factory does: take() consumes the value
        let taken = runner.default_selection.take();
        assert!(taken.is_some());
        assert!(runner.default_selection.is_none()); // Now None for restart
    }

    #[test]
    fn test_splash_visible_after_restart_with_empty_conversation() {
        // BUG REPRODUCTION: /new command shows "Restarting session..." but picker doesn't appear
        // This simulates the state after restart is requested
        let mut runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

        // Simulate first iteration: consume default_selection
        runner.with_default_selection(AgentSelection::Acp("opencode".to_string()));
        let _ = runner.default_selection.take();

        // Simulate restart preparation (what happens at lines 958-961)
        runner.view.state_mut().conversation.clear();
        runner.view.show_splash();

        // The splash should be visible
        assert!(
            runner.view.is_showing_splash(),
            "Splash should be visible after restart preparation"
        );

        // Verify the conditions for splash rendering
        assert!(
            runner.view.state().conversation.items().is_empty(),
            "Conversation should be empty"
        );
        assert!(
            runner.view.state().splash.is_some(),
            "Splash state should exist"
        );
        assert!(
            runner.view.state().popup.is_none(),
            "No popup should be active"
        );
        assert!(
            runner.view.state().dialog_stack.is_empty(),
            "No dialogs should be active"
        );
    }

    #[test]
    fn test_new_command_clears_popup_before_restart() {
        // BUG: /new command doesn't show picker because popup isn't cleared
        // When user types "/new", a command popup exists from typing "/".
        // The /new handler must clear popup BEFORE returning (early return).
        use crate::tui::state::{PopupKind, PopupState};

        let mut runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

        // Simulate: user typed "/new" which created a command popup
        runner.view.set_input("/new");
        runner.view.set_cursor_position(4); // After "/new"
        runner.popup = Some(PopupState::new(PopupKind::Command));
        runner.view.set_popup(runner.popup.clone());

        // Verify initial state
        assert!(runner.view.state().popup.is_some(), "Popup should be set from typing /");
        assert_eq!(runner.view.cursor_position(), 4, "Cursor at end of /new");

        // Simulate what /new handler does (lines 636-643):
        // 1. Set restart_requested
        // 2. Clear popup (MUST happen before early return)
        // 3. Clear input AND cursor position
        // 4. Return Ok(true)
        runner.restart_requested = true;
        runner.popup = None;
        runner.view.set_popup(None);
        runner.view.set_input("");
        runner.view.set_cursor_position(0);  // Critical: reset cursor!

        // Verify cursor is reset (prevents panic when typing in new session)
        assert_eq!(runner.view.cursor_position(), 0, "Cursor must be reset to 0");

        // Now simulate restart preparation (lines 963-965 in run_with_factory)
        runner.view.state_mut().conversation.clear();
        runner.view.show_splash();

        // Check the rendering condition (same as render_frame)
        let would_render_splash = runner.view.state().conversation.items().is_empty()
            && runner.view.state().popup.is_none()
            && runner.view.state().dialog_stack.is_empty()
            && runner.view.state().splash.is_some();

        assert!(would_render_splash, "Splash should render after /new clears popup");
        assert!(runner.view.state().popup.is_none(), "Popup must be None for splash to render");
    }

    #[test]
    fn test_insert_char_with_proper_cursor_reset() {
        // Verifies that after clearing input AND resetting cursor, insert works
        let mut runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

        // Simulate: input was "/new", cursor at position 4
        runner.view.set_input("/new");
        runner.view.set_cursor_position(4);
        assert_eq!(runner.view.input(), "/new");
        assert_eq!(runner.view.cursor_position(), 4);

        // Proper cleanup: clear input AND reset cursor
        runner.view.set_input("");
        runner.view.set_cursor_position(0);

        // Now insert should work without panic
        let mut input = runner.view.input().to_string();
        let pos = runner.view.cursor_position();
        assert_eq!(pos, 0, "Cursor must be 0 after reset");
        assert!(pos <= input.len(), "Cursor must be valid");

        input.insert(pos, 'a');
        assert_eq!(input, "a");
    }

    #[test]
    #[should_panic(expected = "assertion failed")]
    fn test_insert_char_panics_without_cursor_reset() {
        // Demonstrates the bug: clearing input without resetting cursor causes panic
        let mut runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

        // Simulate: input was "/new", cursor at position 4
        runner.view.set_input("/new");
        runner.view.set_cursor_position(4);

        // BUG: clear input but forget cursor reset
        runner.view.set_input("");
        // runner.view.set_cursor_position(0); // Missing!

        // This panics: cursor=4, but input is empty (len=0)
        let mut input = runner.view.input().to_string();
        let pos = runner.view.cursor_position();
        input.insert(pos, 'a'); // PANIC: assertion failed: self.is_char_boundary(idx)
    }
}
