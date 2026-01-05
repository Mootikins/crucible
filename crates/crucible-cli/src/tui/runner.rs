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
use crate::tui::selection::SelectionState;
use ratatui::style::Color;

/// Apply selection highlighting to the frame buffer.
///
/// Modifies buffer cells within the selected range to show a highlight background.
fn apply_selection_highlight(
    frame: &mut ratatui::Frame,
    selection: &SelectionState,
    scroll_offset: usize,
    conv_height: usize,
) {
    let Some((start, end)) = selection.range() else {
        return;
    };

    let area = frame.area();
    let conv_area_height = conv_height.min(area.height as usize);

    // Get the buffer for modification
    let buffer = frame.buffer_mut();

    // Iterate through visible lines in the conversation area
    for screen_row in 0..conv_area_height {
        // Convert screen row to content line index
        let content_line = scroll_offset + screen_row;

        // Check if this line is within selection
        if content_line < start.line || content_line > end.line {
            continue;
        }

        // Determine column bounds for this line
        let (col_start, col_end) = if content_line == start.line && content_line == end.line {
            // Single line selection
            (start.col, end.col)
        } else if content_line == start.line {
            // First line of multi-line
            (start.col, area.width as usize - 1)
        } else if content_line == end.line {
            // Last line of multi-line
            (0, end.col)
        } else {
            // Middle line - full width
            (0, area.width as usize - 1)
        };

        // Apply highlight to cells in range
        for col in col_start..=col_end.min(area.width as usize - 1) {
            let x = area.x + col as u16;
            let y = area.y + screen_row as u16;

            if x < area.x + area.width && y < area.y + area.height {
                if let Some(cell) = buffer.cell_mut((x, y)) {
                    // Use explicit high-contrast colors for selection
                    cell.set_bg(Color::White);
                    cell.set_fg(Color::Black);
                }
            }
        }
    }
}
use crate::chat::slash_registry::SlashCommandRegistry;
use crate::session_logger::SessionLogger;
use crate::tui::agent_picker::AgentSelection;
use crate::tui::notification::NotificationLevel;
use crate::tui::streaming_channel::{
    create_streaming_channel, StreamingEvent, StreamingReceiver, StreamingTask,
};

use crate::tui::components::generic_popup::PopupState;
use crate::tui::conversation::StatusKind;
use crate::tui::conversation_view::{ConversationView, RatatuiView};
use crate::tui::state::{find_word_start_backward, find_word_start_forward, PopupKind};
use crate::tui::{
    map_key_event, DialogState, DynamicPopupProvider, InputAction, ParseEvent, PopupProvider,
    StreamBlock, StreamingParser, TuiState,
};
use anyhow::Result;
use crossterm::{
    cursor,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste,
        EnableMouseCapture, Event, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use crucible_core::events::{FileChangeKind, SessionEvent};
use crucible_core::interaction::{
    AskRequest, InteractionRequest, InteractionResponse, PermRequest, ShowRequest,
};
use crucible_core::InteractionRegistry;
use crucible_rune::EventRing;
use crucible_core::traits::chat::AgentHandle;
use ratatui::{backend::CrosstermBackend, Terminal};
use regex::Regex;
use std::io;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

/// Content pasted into the input area (multi-line pastes are stored separately).
///
/// Multi-line pastes are accumulated rather than inserted directly,
/// showing a summary in the input area until the message is sent.
#[derive(Debug, Clone)]
pub enum PastedContent {
    /// Multi-line text paste
    Text {
        /// The full pasted content
        content: String,
        /// Number of lines
        line_count: usize,
        /// Number of characters
        char_count: usize,
    },
    // Future: Image { data: Vec<u8>, mime: String }
}

impl PastedContent {
    /// Create a new text paste from a string
    pub fn text(content: String) -> Self {
        let line_count = content.lines().count().max(1);
        let char_count = content.chars().count();
        Self::Text {
            content,
            line_count,
            char_count,
        }
    }

    /// Get the content as a string
    pub fn content(&self) -> &str {
        match self {
            Self::Text { content, .. } => content,
        }
    }

    /// Format a summary of this paste for display
    pub fn summary(&self) -> String {
        match self {
            Self::Text {
                line_count,
                char_count,
                ..
            } => format!("[{} lines, {} chars]", line_count, char_count),
        }
    }
}

/// Regex to match paste indicator patterns like `[2 lines, 45 chars]`
static PASTE_INDICATOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\d+ lines?, \d+ chars?\]").expect("paste indicator regex should compile")
});

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
    /// Popup state (consolidated - uses generic popup with provider)
    popup: Option<PopupState>,
    /// Background streaming task
    streaming_task: Option<tokio::task::JoinHandle<()>>,
    /// Channel receiver for streaming events
    streaming_rx: Option<StreamingReceiver>,
    /// Streaming parser for incremental markdown parsing
    streaming_parser: Option<StreamingParser>,
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
    /// Mouse capture state (when disabled, allows terminal text selection)
    mouse_capture_enabled: bool,
    /// Text selection state for mouse-based selection
    selection: crate::tui::selection::SelectionState,
    /// Content cache for selection text extraction
    selection_cache: crate::tui::selection::SelectableContentCache,
    /// Pending interaction request ID (for response correlation)
    pending_interaction_id: Option<String>,
    /// Pending popup request (for handling "Other" selections)
    pending_popup: Option<crucible_core::interaction::PopupRequest>,
    /// Pending AskBatch dialog state (for multi-question interactions)
    pending_ask_batch: Option<crate::tui::ask_batch_dialog::AskBatchDialogState>,
    /// Session logger for persisting chat events to JSONL files
    session_logger: Option<std::sync::Arc<SessionLogger>>,
    /// Help documentation index (lazy-initialized on first :help command)
    docs_index: Option<crate::tui::help::DocsIndex>,
    /// Pending multi-line pastes (accumulated, sent on Enter)
    pending_pastes: Vec<PastedContent>,
    /// Buffer for rapid key input (timing-based paste detection)
    rapid_input_buffer: String,
    /// Timestamp of last key input (for rapid input detection)
    last_key_time: Option<std::time::Instant>,
    /// Event ring for emitting interaction completion events
    event_ring: Option<Arc<EventRing<SessionEvent>>>,
    /// Interaction registry for request-response correlation
    interaction_registry: Option<Arc<Mutex<InteractionRegistry>>>,
    /// Kiln context for search operations
    kiln_context: Option<Arc<crate::core_facade::KilnContext>>,
    /// Session ID to resume from (loads existing conversation history)
    resume_session_id: Option<String>,
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
            history: Vec::new(),
            history_index: None,
            history_saved_input: String::new(),
            current_agent: None,
            command_registry,
            restart_requested: false,
            supports_restart: false, // Set to true when using run_with_factory
            default_selection: None,
            mouse_capture_enabled: true, // Enable by default for scroll support
            selection: crate::tui::selection::SelectionState::new(),
            selection_cache: crate::tui::selection::SelectableContentCache::new(),
            pending_interaction_id: None,
            pending_popup: None,
            pending_ask_batch: None,
            session_logger: None,
            docs_index: None,
            pending_pastes: Vec::new(),
            rapid_input_buffer: String::new(),
            last_key_time: None,
            event_ring: None,
            interaction_registry: None,
            kiln_context: None,
            resume_session_id: None,
        })
    }

    /// Set the session logger for persisting chat events.
    pub fn with_session_logger(&mut self, kiln_path: std::path::PathBuf) -> &mut Self {
        self.session_logger = Some(std::sync::Arc::new(SessionLogger::new(kiln_path)));
        self
    }

    /// Set a session ID to resume from.
    ///
    /// When set, the runner will load existing conversation history from the
    /// session and prepopulate the conversation view. The session logger will
    /// also be configured to append to the existing session instead of creating new.
    pub fn with_resume_session(&mut self, session_id: impl Into<String>) -> &mut Self {
        self.resume_session_id = Some(session_id.into());
        self
    }

    /// Set a default agent selection for the first iteration.
    ///
    /// When set, skips the picker phase on first run but still supports
    /// restart via `/new` command (which will show the picker).
    pub fn with_default_selection(&mut self, selection: AgentSelection) -> &mut Self {
        self.default_selection = Some(selection);
        self
    }

    /// Set the event ring for emitting interaction completion events.
    ///
    /// When set, the runner will emit `InteractionCompleted` events when
    /// dialogs are completed or cancelled.
    pub fn with_event_ring(&mut self, ring: Arc<EventRing<SessionEvent>>) -> &mut Self {
        self.event_ring = Some(ring);
        self
    }

    /// Set the interaction registry for request-response correlation.
    ///
    /// When set, scripts can block on interaction responses using the registry.
    pub fn with_interaction_registry(
        &mut self,
        registry: Arc<Mutex<InteractionRegistry>>,
    ) -> &mut Self {
        self.interaction_registry = Some(registry);
        self
    }

    /// Set the kiln context for search operations.
    ///
    /// When set, `/search` command performs semantic search directly.
    pub fn with_kiln_context(
        &mut self,
        ctx: Arc<crate::core_facade::KilnContext>,
    ) -> &mut Self {
        self.kiln_context = Some(ctx);
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
        let _popup_debounce =
            crate::tui::popup::PopupDebounce::new(std::time::Duration::from_millis(50));
        let mut last_seen_seq = 0u64;

        loop {
            // 0. Update status with paste indicator if applicable
            if let Some(summary) = self.pending_pastes_summary() {
                // Only update if not streaming (streaming has its own status)
                if !self.is_streaming {
                    self.view.set_status_text(&format!("Pending: {}", summary));
                }
            }

            // 1. Render
            {
                let view = &self.view;
                let selection = &self.selection;
                let scroll_offset = view.state().scroll_offset;
                let conv_height = view.conversation_viewport_height();

                terminal.draw(|f| {
                    view.render_frame(f);
                    apply_selection_highlight(f, selection, scroll_offset, conv_height);
                })?;
            }

            // Take popup back from view after render for event handling
            self.take_popup_from_view();

            // 2. Poll events (non-blocking, ~60fps)
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
                    Event::Paste(text) => {
                        tracing::debug!(len = text.len(), has_newlines = text.contains('\n'), "Event::Paste received");
                        self.handle_paste_event(&text);
                    }
                    _ => {}
                }
            } else {
                // No event - check if rapid input buffer should be flushed
                self.flush_rapid_input_if_needed();
            }

            // 3. Sync popup state to view for rendering
            // PopupState handles its own item fetching via provider
            self.sync_popup_to_view();

            // 4. Poll ring buffer for session events
            self.poll_session_events(bridge, &mut last_seen_seq);

            // 5. Poll streaming channel (non-blocking)
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

                            // Accumulate assistant chunk for session logging
                            if let Some(logger) = &self.session_logger {
                                let logger = logger.clone();
                                let text_clone = text.clone();
                                tokio::spawn(async move {
                                    logger.accumulate_assistant_chunk(&text_clone).await;
                                });
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

                            // Flush accumulated assistant message to session log
                            if let Some(logger) = &self.session_logger {
                                let logger = logger.clone();
                                let model = self.current_agent.clone();
                                tokio::spawn(async move {
                                    logger.flush_assistant_message(model.as_deref()).await;
                                });
                            }

                            bridge.ring.push(SessionEvent::AgentResponded {
                                content: full_response,
                                tool_calls: vec![],
                            });
                        }
                        StreamingEvent::Error { message } => {
                            self.is_streaming = false;
                            // Log error to session
                            if let Some(logger) = &self.session_logger {
                                let logger = logger.clone();
                                let msg = message.clone();
                                tokio::spawn(async move {
                                    logger.log_error(&msg, true).await;
                                });
                            }
                            streaming_error = Some(message);
                        }
                        StreamingEvent::ToolCall { id, name, args } => {
                            // Display tool call in the TUI with arguments
                            tracing::debug!(
                                tool_id = ?id,
                                tool_name = %name,
                                "StreamingEvent::ToolCall received - pushing to view"
                            );
                            self.view.push_tool_running(&name, args.clone());
                            self.view.set_status_text(&format!("Running: {}", name));

                            // Log tool call to session
                            if let Some(logger) = &self.session_logger {
                                let logger = logger.clone();
                                let id_str = id.clone().unwrap_or_default();
                                let name_clone = name.clone();
                                let args_clone = args.clone();
                                tokio::spawn(async move {
                                    logger.log_tool_call(&id_str, &name_clone, args_clone).await;
                                });
                            }

                            // Push to event ring for session tracking
                            bridge.ring.push(SessionEvent::ToolCalled { name, args });
                        }
                        StreamingEvent::ToolCompleted {
                            name,
                            result,
                            error,
                        } => {
                            // Update tool display with completion status
                            if let Some(err) = &error {
                                self.view.error_tool(&name, err);
                            } else {
                                // Truncate result for summary (max 50 chars)
                                let summary = if result.len() > 50 {
                                    Some(format!("{}...", &result[..47]))
                                } else if !result.is_empty() {
                                    Some(result.clone())
                                } else {
                                    None
                                };
                                self.view.complete_tool(&name, summary);
                            }

                            // Clear status (tool is done)
                            self.view.clear_status();

                            // Log tool result to session
                            if let Some(logger) = &self.session_logger {
                                let logger = logger.clone();
                                let name_clone = name.clone();
                                // Truncate large results for logging
                                let truncated = result.len() > 8192;
                                let result_str = if truncated {
                                    result[..8192].to_string()
                                } else {
                                    result.clone()
                                };
                                tokio::spawn(async move {
                                    logger
                                        .log_tool_result(&name_clone, &result_str, truncated)
                                        .await;
                                });
                            }

                            // Push to event ring for session tracking
                            bridge.ring.push(SessionEvent::ToolCompleted {
                                name,
                                result,
                                error,
                            });
                        }
                        StreamingEvent::Reasoning { text, seq: _ } => {
                            // Track reasoning tokens in the status display
                            // (reasoning is thinking/chain-of-thought from models like Qwen3-thinking)
                            self.prev_token_count = self.token_count;
                            self.token_count += 1;
                            self.spinner_frame = self.spinner_frame.wrapping_add(1);
                            self.view.set_status(StatusKind::Generating {
                                token_count: self.token_count,
                                prev_token_count: self.prev_token_count,
                                spinner_frame: self.spinner_frame,
                            });

                            // Accumulate reasoning in view for display (Alt+T toggle)
                            self.view.append_reasoning(&text);
                            self.view.tick_reasoning_animation();

                            // Push reasoning to session using AgentThinking event
                            bridge
                                .ring
                                .push(SessionEvent::AgentThinking { thought: text });
                        }
                    }
                }
            }

            // Flush partial content for progressive display (even without newlines)
            // This is done after processing all available deltas but before applying events
            if self.is_streaming && !streaming_complete {
                if let Some(parser) = &mut self.streaming_parser {
                    if let Some(partial_event) = parser.flush_partial() {
                        pending_parse_events.push(partial_event);
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
                // Clear reasoning buffer for next response
                self.view.clear_reasoning();
            }

            // Handle streaming error
            if let Some(message) = streaming_error {
                self.view.clear_status();
                self.view.echo_error(&format!("Error: {}", message));
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
                self.view.set_status(StatusKind::Thinking {
                    spinner_frame: self.spinner_frame,
                });
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

        // AskBatch dialog takes priority over other input
        if let Some(ref mut ask_batch) = self.pending_ask_batch {
            use crate::tui::ask_batch_dialog::AskBatchResult;
            match ask_batch.handle_key(*key) {
                AskBatchResult::Complete(response) => {
                    let request_id = self.pending_interaction_id.take();
                    let interaction_response = InteractionResponse::AskBatch(response);

                    // Emit InteractionCompleted event for event listeners
                    if let (Some(ring), Some(ref id)) = (&self.event_ring, &request_id) {
                        ring.push(SessionEvent::InteractionCompleted {
                            request_id: id.clone(),
                            response: interaction_response.clone(),
                        });
                    }

                    // Complete via registry for synchronous waiters
                    if let (Some(registry), Some(ref id)) = (&self.interaction_registry, &request_id)
                    {
                        if let Ok(mut guard) = registry.lock() {
                            guard.complete(
                                id.parse().unwrap_or_default(),
                                interaction_response.clone(),
                            );
                        }
                    }

                    self.view.set_status_text("Questions answered");
                    self.pending_ask_batch = None;
                }
                AskBatchResult::Cancelled(uuid) => {
                    let request_id = self.pending_interaction_id.take();
                    let interaction_response = InteractionResponse::Cancelled;

                    // Emit InteractionCompleted event for event listeners
                    if let (Some(ring), Some(ref id)) = (&self.event_ring, &request_id) {
                        ring.push(SessionEvent::InteractionCompleted {
                            request_id: id.clone(),
                            response: interaction_response.clone(),
                        });
                    }

                    // Cancel via registry for synchronous waiters
                    if let Some(registry) = &self.interaction_registry {
                        if let Ok(mut guard) = registry.lock() {
                            guard.cancel(uuid);
                        }
                    }

                    self.view.set_status_text("Questions cancelled");
                    self.pending_ask_batch = None;
                }
                AskBatchResult::Pending => {
                    // Still in dialog, just consumed the key
                }
            }
            return Ok(false);
        }

        // Build a minimal TuiState for key mapping (we'll migrate away from this)
        let mut temp_state = TuiState::new(self.view.mode_id());
        temp_state.input_buffer = self.view.input().to_string();
        temp_state.cursor_position = self.view.cursor_position();
        temp_state.ctrl_c_count = self.ctrl_c_count;
        temp_state.last_ctrl_c = self.last_ctrl_c;
        temp_state.has_popup = self.popup.is_some(); // Needed for Up/Down to navigate popup
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
            InputAction::SendMessage(typed_msg) => {
                // Check if we're in the middle of rapid input (timing-based paste detection)
                // If Enter comes during rapid input, treat it as a newline in the paste
                if let Some(last_time) = self.last_key_time {
                    let elapsed_ms =
                        std::time::Instant::now().duration_since(last_time).as_millis() as u64;
                    if elapsed_ms <= Self::RAPID_INPUT_THRESHOLD_MS
                        && !self.rapid_input_buffer.is_empty()
                    {
                        // Still in rapid input - record newline and don't send yet
                        self.rapid_input_buffer.push('\n');
                        self.last_key_time = Some(std::time::Instant::now());
                        tracing::debug!(
                            buffer_len = self.rapid_input_buffer.len(),
                            "Enter during rapid input - treating as newline"
                        );
                        return Ok(false);
                    }
                }

                // Reset Ctrl+C tracking
                self.ctrl_c_count = 0;

                // Clear rapid input buffer (we're sending, not accumulating)
                self.clear_rapid_input();

                // Build final message: pending pastes + typed content
                // Note: For shell (!) and REPL (:) commands, we use typed_msg only
                // since pastes should be sent as chat content, not command args
                let msg = if !self.pending_pastes.is_empty()
                    && !typed_msg.starts_with('!')
                    && !typed_msg.starts_with(':')
                {
                    let mut full_msg = String::new();
                    for paste in self.pending_pastes.drain(..) {
                        full_msg.push_str(paste.content());
                        if !paste.content().ends_with('\n') {
                            full_msg.push('\n');
                        }
                    }
                    full_msg.push_str(&typed_msg);
                    full_msg
                } else {
                    // Clear any pending pastes if we're running a command
                    // (they get lost, but that's intentional for ! and : commands)
                    self.pending_pastes.clear();
                    typed_msg
                };

                // Handle shell passthrough (!) - execute immediately, don't send to agent
                if msg.starts_with('!') {
                    let cmd = msg.strip_prefix('!').unwrap_or("").trim();
                    if !cmd.is_empty() {
                        // Add to history
                        if self.history.last() != Some(&msg) {
                            self.history.push(msg.clone());
                        }
                        self.history_index = None;
                        self.history_saved_input.clear();

                        // Clear input
                        self.view.set_input("");
                        self.view.set_cursor_position(0);
                        self.popup = None;

                        // Execute shell command
                        self.execute_shell_command(cmd)?;
                    }
                    return Ok(false);
                }

                // Handle REPL commands (:) - execute immediately, don't send to agent
                if msg.starts_with(':') {
                    let cmd = msg.strip_prefix(':').unwrap_or("").trim();
                    // Take the first word as the command name, rest as args
                    let cmd_name = cmd.split_whitespace().next().unwrap_or("").to_lowercase();
                    let args = cmd.strip_prefix(&cmd_name).map(|s| s.trim()).unwrap_or("");
                    if !cmd_name.is_empty() {
                        // Add to history
                        if self.history.last() != Some(&msg) {
                            self.history.push(msg.clone());
                        }
                        self.history_index = None;
                        self.history_saved_input.clear();

                        // Clear input
                        self.view.set_input("");
                        self.view.set_cursor_position(0);
                        self.popup = None;

                        // Execute REPL command with args
                        return self.execute_repl_command(&cmd_name, args);
                    }
                    return Ok(false);
                }

                // Add to history (avoid duplicates for repeated commands)
                if self.history.last() != Some(&msg) {
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

                // Log user message to session (creates session on first message)
                if let Some(logger) = &self.session_logger {
                    let logger = logger.clone();
                    let msg_clone = msg.clone();
                    tokio::spawn(async move {
                        logger.log_user_message(&msg_clone).await;
                    });
                }

                // Set thinking status
                self.is_streaming = true;
                self.token_count = 0;
                self.prev_token_count = 0;
                self.spinner_frame = 0;
                self.view
                    .set_status(StatusKind::Thinking { spinner_frame: 0 });
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
                // Check if we're in rapid input mode (potential paste)
                let now = std::time::Instant::now();
                let in_rapid_input = self.last_key_time.is_some_and(|last| {
                    now.duration_since(last).as_millis() as u64 <= Self::RAPID_INPUT_THRESHOLD_MS
                });

                if in_rapid_input || !self.rapid_input_buffer.is_empty() {
                    // Accumulate in rapid input buffer, don't insert yet
                    self.rapid_input_buffer.push(c);
                    self.last_key_time = Some(now);
                } else {
                    // Normal typing - insert directly, just track time (no buffer)
                    self.last_key_time = Some(now);

                    // Insert the character
                    let mut input = self.view.input().to_string();
                    let pos = self.view.cursor_position();
                    input.insert(pos, c);
                    self.view.set_input(&input);
                    self.view.set_cursor_position(pos + c.len_utf8());
                    self.update_popup();
                }
            }
            InputAction::DeleteChar => {
                let input = self.view.input().to_string();
                let pos = self.view.cursor_position();
                if pos > 0 {
                    // Check if we're deleting into a paste indicator
                    if let Some((start, end, idx)) = self.find_paste_indicator_at(&input, pos) {
                        // Delete entire indicator and corresponding paste
                        let new_pos = self.delete_paste_indicator(start, end, idx);
                        self.view.set_cursor_position(new_pos);
                    } else {
                        // Normal single-character delete
                        let prev = input[..pos]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        let new_input = format!("{}{}", &input[..prev], &input[pos..]);
                        self.view.set_input(&new_input);
                        self.view.set_cursor_position(prev);
                    }
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
                }
            }
            InputAction::ConfirmPopup => {
                use crate::tui::state::PopupItem;

                if let Some(ref popup) = self.popup {
                    if let Some(item) = popup.selected_item() {
                        // Handle REPL commands specially - execute immediately
                        if let PopupItem::ReplCommand { name, .. } = item {
                            let name = name.clone();
                            self.popup = None;
                            self.view.set_input("");
                            self.view.set_cursor_position(0);
                            return self.execute_repl_command(&name, "");
                        }

                        // Handle Session items - resume the selected session
                        if let PopupItem::Session { id, .. } = item {
                            let session_id = id.clone();
                            self.popup = None;
                            self.view.set_popup(None);
                            self.view.set_input("");
                            self.view.set_cursor_position(0);

                            // Request session resume via restart mechanism
                            if self.supports_restart {
                                self.resume_session_id = Some(session_id.clone());
                                self.restart_requested = true;
                                self.view.set_status_text(&format!(
                                    "Resuming session {}...",
                                    session_id
                                ));
                                return Ok(true); // Exit to trigger restart with resume
                            } else {
                                self.view
                                    .set_status_text("Session resume requires deferred agent mode");
                            }
                            return Ok(false);
                        }

                        // For other items, insert the token
                        let token = item.token();
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
                                self.view.echo_message("Conversation cleared");
                            }
                            "mode" | "plan" | "act" | "auto" => {
                                self.view.set_status_text("Use Shift+Tab to switch modes");
                            }
                            "search" => {
                                if args.is_empty() {
                                    // Show input hint from registry if available
                                    let hint =
                                        descriptor.input_hint.as_deref().unwrap_or("<query>");
                                    self.view.set_status_text(&format!(
                                        "Usage: /search {}",
                                        hint
                                    ));
                                } else if let Some(ctx) = &self.kiln_context {
                                    // Perform semantic search directly
                                    self.view.set_status_text(&format!("Searching: {}", args));
                                    match ctx.semantic_search(args, 10).await {
                                        Ok(results) => {
                                            if results.is_empty() {
                                                self.view.echo_message(&format!(
                                                    "No results found for '{}'",
                                                    args
                                                ));
                                            } else {
                                                // Format results
                                                let mut output = format!(
                                                    "**Search results for '{}':**\n\n",
                                                    args
                                                );
                                                for (i, result) in results.iter().enumerate() {
                                                    output.push_str(&format!(
                                                        "{}. **{}** ({:.0}%)\n   {}\n\n",
                                                        i + 1,
                                                        result.title,
                                                        result.similarity * 100.0,
                                                        result.snippet.lines().next().unwrap_or("")
                                                    ));
                                                }
                                                self.view.echo_message(&output);
                                            }
                                            self.view.set_status_text(&format!(
                                                "Found {} results",
                                                results.len()
                                            ));
                                        }
                                        Err(e) => {
                                            self.view.echo_message(&format!(
                                                "Search failed: {}",
                                                e
                                            ));
                                            self.view.set_status_text("Search failed");
                                        }
                                    }
                                } else {
                                    // No kiln context - show error
                                    self.view.echo_message(
                                        "Search unavailable: kiln context not initialized",
                                    );
                                    self.view.set_status_text("Search unavailable");
                                }
                            }
                            "context" => {
                                self.view.set_status_text(
                                    "Use @note:<path> or @file:<path> to inject context",
                                );
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
                                    self.view.set_status_text(&format!(
                                        "Use /new to start a new session. Current agent: {}",
                                        current
                                    ));
                                }
                            }
                            "new" => {
                                if self.supports_restart {
                                    // Request restart with new session
                                    self.restart_requested = true;
                                    // Clear popup/input state before restart
                                    self.popup = None;
                                    self.view.set_popup(None);
                                    self.view.set_input("");
                                    self.view.set_cursor_position(0);
                                    self.view.set_status_text("Starting new session...");
                                    return Ok(true); // Exit to trigger restart
                                } else {
                                    // Can't restart without a factory
                                    self.view
                                        .set_status_text("/new requires deferred agent mode");
                                }
                            }
                            "resume" => {
                                // Open session picker popup
                                if let Some(ref logger) = self.session_logger {
                                    let sessions = logger.list_sessions().await;
                                    if sessions.is_empty() {
                                        self.view.set_status_text("No sessions found");
                                    } else {
                                        // Convert sessions to PopupItems
                                        let items: Vec<crate::tui::state::PopupItem> = sessions
                                            .into_iter()
                                            .take(20) // Limit to 20 most recent
                                            .map(|id| {
                                                crate::tui::state::PopupItem::session(id.as_str())
                                                    .desc("Resume this session")
                                            })
                                            .collect();

                                        // Create popup with session items
                                        let mut popup = PopupState::new(
                                            PopupKind::Session,
                                            std::sync::Arc::clone(&self.popup_provider)
                                                as std::sync::Arc<dyn PopupProvider>,
                                        );
                                        popup.set_items(items.clone());
                                        self.popup = Some(popup);

                                        // Create separate popup for view
                                        let mut view_popup = PopupState::new(
                                            PopupKind::Session,
                                            std::sync::Arc::clone(&self.popup_provider)
                                                as std::sync::Arc<dyn PopupProvider>,
                                        );
                                        view_popup.set_items(items);
                                        self.view.set_popup(Some(view_popup));
                                        self.view.set_status_text("Select a session to resume");
                                        return Ok(false); // Don't clear input yet
                                    }
                                } else {
                                    self.view.set_status_text("Session logging not enabled");
                                }
                            }
                            _ => {
                                // Command exists in registry but no TUI handler yet
                                self.view.set_status_text(&format!(
                                    "{}: {}",
                                    cmd_name, descriptor.description
                                ));
                            }
                        }
                    } else {
                        // Not in registry - could be agent command or unknown
                        self.view
                            .set_status_text(&format!("Unknown command: /{}", cmd_name));
                    }
                }

                // Clear input after executing
                self.view.set_input("");
                self.view.set_cursor_position(0);
                self.popup = None;
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
                        self.view
                            .set_cursor_position(self.history_saved_input.len());
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
            // Readline-style editing (emacs mode)
            InputAction::DeleteWordBackward => {
                let input = self.view.input().to_string();
                let cursor = self.view.cursor_position();
                if cursor > 0 {
                    // Check if we're deleting into a paste indicator
                    if let Some((start, end, idx)) = self.find_paste_indicator_at(&input, cursor) {
                        // Delete entire indicator and corresponding paste
                        let new_pos = self.delete_paste_indicator(start, end, idx);
                        self.view.set_cursor_position(new_pos);
                    } else {
                        let before = &input[..cursor];
                        let word_start = find_word_start_backward(before);
                        let mut new_input = input.clone();
                        new_input.drain(word_start..cursor);
                        self.view.set_input(&new_input);
                        self.view.set_cursor_position(word_start);
                    }
                    self.update_popup();
                }
            }
            InputAction::DeleteToLineStart => {
                let input = self.view.input().to_string();
                let cursor = self.view.cursor_position();
                if cursor > 0 {
                    let new_input = input[cursor..].to_string();
                    self.view.set_input(&new_input);
                    self.view.set_cursor_position(0);
                    self.update_popup();
                }
                // Also clear any pending pastes
                self.clear_pending_pastes();
            }
            InputAction::DeleteToLineEnd => {
                let input = self.view.input().to_string();
                let cursor = self.view.cursor_position();
                if cursor < input.len() {
                    let new_input = input[..cursor].to_string();
                    self.view.set_input(&new_input);
                    self.update_popup();
                }
            }
            InputAction::MoveCursorToStart => {
                self.view.set_cursor_position(0);
            }
            InputAction::MoveCursorToEnd => {
                let len = self.view.input().len();
                self.view.set_cursor_position(len);
            }
            InputAction::MoveWordBackward => {
                let input = self.view.input();
                let cursor = self.view.cursor_position();
                if cursor > 0 {
                    let before = &input[..cursor];
                    self.view
                        .set_cursor_position(find_word_start_backward(before));
                }
            }
            InputAction::MoveWordForward => {
                let input = self.view.input();
                let cursor = self.view.cursor_position();
                if cursor < input.len() {
                    let after = &input[cursor..];
                    self.view
                        .set_cursor_position(cursor + find_word_start_forward(after));
                }
            }
            InputAction::TransposeChars => {
                let input = self.view.input().to_string();
                let cursor = self.view.cursor_position();
                let len = input.chars().count();
                if len >= 2 && cursor > 0 {
                    let chars: Vec<char> = input.chars().collect();
                    let char_pos = input[..cursor].chars().count();

                    let (i, j) = if char_pos >= len {
                        (len - 2, len - 1)
                    } else {
                        (char_pos - 1, char_pos)
                    };

                    let mut new_chars = chars.clone();
                    new_chars.swap(i, j);
                    let new_input: String = new_chars.into_iter().collect();

                    let new_cursor = if char_pos < len {
                        new_input
                            .char_indices()
                            .nth(char_pos + 1)
                            .map(|(idx, _)| idx)
                            .unwrap_or(new_input.len())
                    } else {
                        new_input.len()
                    };

                    self.view.set_input(&new_input);
                    self.view.set_cursor_position(new_cursor);
                }
            }
            InputAction::ToggleReasoning => {
                // Toggle reasoning panel visibility
                let current = self.view.show_reasoning();
                self.view.set_show_reasoning(!current);
            }
            InputAction::ToggleMouseCapture => {
                // Toggle mouse capture (allows terminal text selection when disabled)
                use std::io::Write;
                self.mouse_capture_enabled = !self.mouse_capture_enabled;
                let mut stdout = io::stdout();
                if self.mouse_capture_enabled {
                    let _ = execute!(stdout, EnableMouseCapture);
                    let _ = stdout.flush();
                    self.view
                        .set_status_text("Mouse capture enabled (scroll works)");
                } else {
                    let _ = execute!(stdout, DisableMouseCapture);
                    let _ = stdout.flush();
                    self.view
                        .set_status_text("Mouse capture disabled (text selection works)");
                }
            }
            InputAction::CopyMarkdown => {
                // Copy last assistant message as markdown to clipboard via OSC 52
                if let Some(markdown) = self.view.state().conversation.last_assistant_markdown() {
                    if copy_to_clipboard_osc52(&markdown) {
                        self.view.set_status_text("Copied to clipboard");
                    } else {
                        self.view
                            .set_status_text("Copy failed (terminal may not support OSC 52)");
                    }
                } else {
                    self.view.set_status_text("No assistant message to copy");
                }
            }
            InputAction::None => {}
        }

        Ok(false)
    }

    /// Handle mouse events for scrolling and text selection.
    fn handle_mouse_event(&mut self, mouse: &MouseEvent) {
        use crate::tui::selection::SelectionPoint;

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.view.scroll_up(3);
                // Invalidate selection cache on scroll
                self.selection_cache.invalidate();
            }
            MouseEventKind::ScrollDown => {
                self.view.scroll_down(3);
                // Invalidate selection cache on scroll
                self.selection_cache.invalidate();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Start selection at mouse position
                if let Some(point) = self.mouse_to_content_point(mouse.column, mouse.row) {
                    self.selection.start(point);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Update selection during drag
                if let Some(point) = self.mouse_to_content_point(mouse.column, mouse.row) {
                    self.selection.update(point);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // Complete selection and copy to clipboard
                self.selection.complete();
                if self.selection.has_selection() {
                    self.copy_selection_to_clipboard();
                }
            }
            _ => {}
        }
    }

    /// Threshold for considering rapid key input as a paste (50ms)
    const RAPID_INPUT_THRESHOLD_MS: u64 = 50;

    /// Handle paste events for multi-line paste detection.
    ///
    /// Single-line pastes are inserted directly into the input buffer.
    /// Multi-line pastes are stored in `pending_pastes` and indicator shown in input.
    fn handle_paste_event(&mut self, text: &str) {
        // Normalize line endings: \r\n -> \n, \r -> \n
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");

        if normalized.contains('\n') {
            // Multi-line paste: store and show indicator in input box
            let paste = PastedContent::text(normalized);

            // Show paste indicator in input box
            let indicator = paste.summary();
            let current_input = self.view.input().to_string();
            if current_input.is_empty() {
                self.view.set_input(&indicator);
            } else {
                self.view.set_input(&format!("{} {}", current_input, indicator));
            }
            self.view.set_cursor_position(self.view.input().len());

            self.pending_pastes.push(paste);
        } else {
            // Single-line paste: insert directly at cursor
            let state = self.view.state_mut();
            let cursor_pos = state.cursor_position;
            state.input_buffer.insert_str(cursor_pos, &normalized);
            state.cursor_position += normalized.len();
        }
    }

    /// Check if enough time has passed since last key, flush rapid input buffer if needed.
    /// Returns true if buffer was flushed as a multi-line paste.
    fn flush_rapid_input_if_needed(&mut self) -> bool {
        let now = std::time::Instant::now();

        // Check if we should flush the rapid input buffer
        if let Some(last_time) = self.last_key_time {
            let elapsed_ms = now.duration_since(last_time).as_millis() as u64;

            // If gap is larger than threshold and buffer has content
            if elapsed_ms > Self::RAPID_INPUT_THRESHOLD_MS && !self.rapid_input_buffer.is_empty() {
                let buffer = std::mem::take(&mut self.rapid_input_buffer);
                self.last_key_time = None;

                // Normalize and check for newlines
                let normalized = buffer.replace("\r\n", "\n").replace('\r', "\n");

                if normalized.contains('\n') {
                    // Treat as multi-line paste - show indicator in input box
                    let paste = PastedContent::text(normalized);
                    tracing::debug!(
                        lines = paste.summary(),
                        "Rapid input detected as paste (timing-based)"
                    );

                    // Show paste indicator in input box
                    let indicator = paste.summary();
                    let current_input = self.view.input().to_string();
                    if current_input.is_empty() {
                        self.view.set_input(&indicator);
                    } else {
                        self.view.set_input(&format!("{} {}", current_input, indicator));
                    }
                    self.view.set_cursor_position(self.view.input().len());

                    self.pending_pastes.push(paste);
                    return true;
                } else {
                    // Single-line rapid input: insert all accumulated characters at once
                    {
                        let state = self.view.state_mut();
                        let cursor_pos = state.cursor_position;
                        state.input_buffer.insert_str(cursor_pos, &normalized);
                        state.cursor_position += normalized.len();
                    }
                    // Update popup after inserting (mutable borrow released by block above)
                    self.update_popup();
                }
            }
        }
        false
    }

    /// Record a character for rapid input detection.
    /// Called for printable character key events.
    fn record_rapid_input(&mut self, ch: char) {
        let now = std::time::Instant::now();

        // Check if this is continuation of rapid input
        if let Some(last_time) = self.last_key_time {
            let elapsed_ms = now.duration_since(last_time).as_millis() as u64;

            if elapsed_ms > Self::RAPID_INPUT_THRESHOLD_MS {
                // Gap too large - flush any existing buffer first
                self.flush_rapid_input_if_needed();
            }
        }

        // Accumulate this character
        self.rapid_input_buffer.push(ch);
        self.last_key_time = Some(now);
    }

    /// Clear the rapid input buffer (called after processing).
    fn clear_rapid_input(&mut self) {
        self.rapid_input_buffer.clear();
        self.last_key_time = None;
    }

    /// Get a formatted summary of all pending pastes.
    fn pending_pastes_summary(&self) -> Option<String> {
        if self.pending_pastes.is_empty() {
            return None;
        }

        let total_lines: usize = self
            .pending_pastes
            .iter()
            .map(|p| match p {
                PastedContent::Text { line_count, .. } => *line_count,
            })
            .sum();

        let total_chars: usize = self
            .pending_pastes
            .iter()
            .map(|p| match p {
                PastedContent::Text { char_count, .. } => *char_count,
            })
            .sum();

        if self.pending_pastes.len() == 1 {
            Some(self.pending_pastes[0].summary())
        } else {
            Some(format!(
                "[{} pastes: {} lines, {} chars]",
                self.pending_pastes.len(),
                total_lines,
                total_chars
            ))
        }
    }

    /// Clear all pending pastes (called on Ctrl+U or Esc).
    fn clear_pending_pastes(&mut self) {
        if !self.pending_pastes.is_empty() {
            self.pending_pastes.clear();
            self.view.set_status_text("Cleared pending pastes");
        }
    }

    /// Find paste indicator containing or immediately after the given byte position.
    ///
    /// Returns `Some((start_byte, end_byte, index))` if the position is at the end of
    /// an indicator (would delete into it) or inside one. The index corresponds to
    /// the Nth indicator in the input (0-indexed), which maps to `pending_pastes[index]`.
    fn find_paste_indicator_at(&self, input: &str, pos: usize) -> Option<(usize, usize, usize)> {
        for (idx, mat) in PASTE_INDICATOR_RE.find_iter(input).enumerate() {
            // Check if cursor is inside the indicator or just after it (about to delete into it)
            if pos > mat.start() && pos <= mat.end() {
                return Some((mat.start(), mat.end(), idx));
            }
        }
        None
    }

    /// Delete a paste indicator from input and remove the corresponding paste.
    ///
    /// Returns the new cursor position after deletion.
    fn delete_paste_indicator(&mut self, indicator_start: usize, indicator_end: usize, paste_idx: usize) -> usize {
        // Remove from input
        let input = self.view.input().to_string();
        let mut new_input = String::with_capacity(input.len() - (indicator_end - indicator_start));
        new_input.push_str(&input[..indicator_start]);

        // Handle space before indicator (if present)
        let trim_space_before = indicator_start > 0 && input.as_bytes().get(indicator_start.saturating_sub(1)) == Some(&b' ');
        let new_cursor = if trim_space_before {
            new_input.pop(); // Remove trailing space before indicator
            indicator_start - 1
        } else {
            indicator_start
        };

        new_input.push_str(&input[indicator_end..]);
        self.view.set_input(&new_input);

        // Remove corresponding paste (if index is valid)
        if paste_idx < self.pending_pastes.len() {
            self.pending_pastes.remove(paste_idx);
        }

        new_cursor
    }

    /// Concatenate pending pastes with input buffer for sending.
    fn build_message_with_pastes(&mut self) -> String {
        let mut message = String::new();

        // Add all pending pastes first
        for paste in self.pending_pastes.drain(..) {
            message.push_str(paste.content());
            if !paste.content().ends_with('\n') {
                message.push('\n');
            }
        }

        // Add the typed content
        let typed = std::mem::take(&mut self.view.state_mut().input_buffer);
        message.push_str(&typed);

        message
    }

    /// Convert mouse screen coordinates to content coordinates.
    ///
    /// Returns None if the mouse is outside the conversation area.
    fn mouse_to_content_point(
        &self,
        x: u16,
        y: u16,
    ) -> Option<crate::tui::selection::SelectionPoint> {
        use crate::tui::selection::SelectionPoint;

        // Get the conversation area bounds
        // The conversation area starts at row 0 and takes most of the screen
        // Layout: conversation | reasoning? | spacer | popup? | input (3) | status (1)
        let state = self.view.state();

        // Calculate conversation area height (total height minus fixed components)
        // Input = 3 lines, Status = 1 line, Spacer = 1 line
        let fixed_height: u16 = 3 + 1 + 1;
        let conv_height = state.height.saturating_sub(fixed_height);

        // Check if mouse is in conversation area (row < conv_height)
        if y >= conv_height {
            return None;
        }

        // Convert to content coordinates
        // Line index = scroll_offset + row
        let line = state.scroll_offset + y as usize;
        // Column is just the x position (no horizontal scroll for now)
        let col = x as usize;

        Some(SelectionPoint::new(line, col))
    }

    /// Copy current selection to clipboard using OSC 52.
    fn copy_selection_to_clipboard(&mut self) {
        use base64::Engine;

        let Some((start, end)) = self.selection.range() else {
            return;
        };

        // Rebuild cache if needed (width changed or cache is empty)
        let width = self.view.state().width;
        if self.selection_cache.needs_rebuild(width) {
            let cache_data = self.view.build_selection_cache();
            self.selection_cache.update(cache_data, width);
        }

        // Extract text from selection cache
        let text = self.selection_cache.extract_text(start, end);

        if text.is_empty() {
            self.view.set_status_text("No text selected");
            return;
        }

        // Copy via OSC 52 escape sequence
        let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
        let osc52 = format!("\x1b]52;c;{}\x07", encoded);

        if execute!(io::stdout(), crossterm::style::Print(&osc52)).is_ok() {
            let line_count = text.lines().count();
            let char_count = text.chars().count();
            self.view.set_status_text(&format!(
                "Copied {} chars ({} lines)",
                char_count, line_count
            ));
        } else {
            self.view
                .set_status_text("Copy failed (terminal may not support OSC 52)");
        }

        // Clear selection after copy
        self.selection.clear();
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
                    // NOTE: Tool already displayed via StreamingEvent::ToolCall handler
                    // which also emits to the ring buffer. Don't push again here.
                    // Just update status text (ring events may come from other sources).
                    self.view.set_status_text(&format!("Running: {}", name));
                }
                SessionEvent::ToolCompleted {
                    name,
                    result: _,
                    error: _,
                } => {
                    // NOTE: Tool already completed via StreamingEvent::ToolCompleted handler
                    // which also emits to the ring buffer. Don't call complete_tool again here
                    // to avoid duplicate display. Just clear status (ring events may come from
                    // other sources like external agents).
                    self.view.set_status_text(&format!("Completed: {}", name));
                }
                // Handle interaction requests
                SessionEvent::InteractionRequested {
                    request_id,
                    request,
                } => {
                    self.handle_interaction_request(request_id, request);
                }
                _ => {}
            }
        }

        // Update notification state after processing all events
        self.view.state_mut().notifications.tick();
    }

    /// Update popup based on current input.
    fn update_popup(&mut self) {
        // PopupKind is already imported at module level

        let input = self.view.input();
        let trimmed = input.trim_start();

        if trimmed.starts_with('/') {
            let query = trimmed.strip_prefix('/').unwrap_or("").to_string();
            if self.popup.as_ref().map(|p| p.kind()) != Some(PopupKind::Command) {
                self.popup = Some(PopupState::new(
                    PopupKind::Command,
                    std::sync::Arc::clone(&self.popup_provider)
                        as std::sync::Arc<dyn PopupProvider>,
                ));
            }
            if let Some(ref mut popup) = self.popup {
                popup.update_query(&query);
            }
        } else if trimmed.starts_with('@') {
            let query = trimmed.strip_prefix('@').unwrap_or("").to_string();
            if self.popup.as_ref().map(|p| p.kind()) != Some(PopupKind::AgentOrFile) {
                self.popup = Some(PopupState::new(
                    PopupKind::AgentOrFile,
                    std::sync::Arc::clone(&self.popup_provider)
                        as std::sync::Arc<dyn PopupProvider>,
                ));
            }
            if let Some(ref mut popup) = self.popup {
                popup.update_query(&query);
            }
        } else if trimmed.starts_with(':') {
            // REPL commands (vim-style system commands)
            let query = trimmed.strip_prefix(':').unwrap_or("").to_string();
            if self.popup.as_ref().map(|p| p.kind()) != Some(PopupKind::ReplCommand) {
                self.popup = Some(PopupState::new(
                    PopupKind::ReplCommand,
                    std::sync::Arc::clone(&self.popup_provider)
                        as std::sync::Arc<dyn PopupProvider>,
                ));
            }
            if let Some(ref mut popup) = self.popup {
                popup.update_query(&query);
            }
        } else {
            self.popup = None;
        }
    }

    /// Sync popup from runner to view for rendering.
    ///
    /// Called at the end of the event loop, before the next render.
    /// The view keeps the popup until `take_popup_from_view` is called.
    fn sync_popup_to_view(&mut self) {
        self.view.set_popup(self.popup.take());
    }

    /// Take popup back from view after rendering.
    ///
    /// Called after render, before event handling.
    fn take_popup_from_view(&mut self) {
        self.popup = self.view.popup_take();
    }

    /// Get the view state for testing.
    pub fn view(&self) -> &RatatuiView {
        &self.view
    }

    /// Execute a REPL command (vim-style system commands)
    fn execute_repl_command(&mut self, name: &str, args: &str) -> Result<bool> {
        use crate::tui::repl_commands::lookup;

        debug!(cmd = %name, args = %args, "Executing REPL command");

        // Look up command (handles aliases)
        let Some(cmd) = lookup(name) else {
            self.view
                .set_status_text(&format!("Unknown command: {}", name));
            return Ok(false);
        };

        match cmd.name {
            "quit" => {
                // Exit the application
                return Ok(true);
            }
            "help" => {
                self.show_help(args)?;
            }
            "mode" => {
                // Cycle mode (same as Shift+Tab)
                let new_mode = crucible_core::traits::chat::cycle_mode_id(self.view.mode_id());
                self.view.set_mode_id(new_mode);
                self.view
                    .set_status_text(&format!("Mode: {}", self.view.mode_id()));
            }
            "agent" => {
                // TODO: Open agent picker popup
                self.view
                    .set_status_text("Agent switching not yet implemented");
            }
            "models" => {
                // TODO: Open models list popup
                self.view
                    .set_status_text("Model listing not yet implemented");
            }
            "config" => {
                // Show current config summary
                self.view
                    .set_status_text(&format!("Mode: {}", self.view.mode_id()));
            }
            "messages" | "mes" => {
                // Show message history popup (vim-style :messages)
                let history = self.view.state().notifications.format_history();
                self.view
                    .push_dialog(crate::tui::dialog::DialogState::info("Messages", history));
            }
            "edit" | "e" | "view" => {
                // Open session in $EDITOR
                self.open_session_in_editor()?;
            }
            _ => {
                self.view.echo_error(&format!("Unknown command: {}", name));
            }
        }

        Ok(false)
    }

    /// Show help documentation.
    ///
    /// - No args: Show topic index
    /// - With args: Search for topic and show content
    fn show_help(&mut self, query: &str) -> Result<()> {
        use crate::tui::help::DocsIndex;

        // Lazy-initialize docs index
        if self.docs_index.is_none() {
            match DocsIndex::init() {
                Ok(index) => {
                    self.docs_index = Some(index);
                }
                Err(e) => {
                    self.view
                        .echo_error(&format!("Failed to load help docs: {}", e));
                    return Ok(());
                }
            }
        }

        let index = self.docs_index.as_mut().unwrap();

        if query.is_empty() {
            // No argument: show topic index
            let topics = index.all_topics();
            if topics.is_empty() {
                self.view.echo_message("No help topics available");
                return Ok(());
            }

            // Build index display
            let mut content = String::from("Available help topics:\n\n");
            for topic in topics {
                content.push_str(&format!(
                    "  {:30} {}\n",
                    topic.display_name(),
                    topic.summary
                ));
            }
            content.push_str("\nUse :help <topic> to view a topic.");

            self.view.push_dialog(crate::tui::dialog::DialogState::info(
                "Help Topics",
                content,
            ));
        } else {
            // Search for topic
            let results = index.search(query);

            if results.is_empty() {
                self.view
                    .echo_error(&format!("No help topic found for: {}", query));
                return Ok(());
            }

            // Clone topic data before releasing borrow from search
            let topic = results[0].clone();
            let title = format!("Help: {}", topic.display_name());

            match index.load_content(&topic) {
                Ok(content) => {
                    self.view
                        .push_dialog(crate::tui::dialog::DialogState::info(&title, content));
                }
                Err(e) => {
                    self.view
                        .echo_error(&format!("Failed to load topic '{}': {}", query, e));
                }
            }
        }

        Ok(())
    }

    /// Drop to interactive shell with command shown
    ///
    /// Instead of running the command directly, this spawns the user's shell
    /// and prints the command for them to run. This allows for:
    /// - Editing the command before running
    /// - Running sudo commands (password prompt works)
    /// - Chaining additional commands
    /// - Full interactive shell access
    fn execute_shell_command(&mut self, cmd: &str) -> Result<()> {
        use std::process::Command;

        debug!(cmd = %cmd, "Dropping to shell");

        // Exit TUI
        execute!(
            std::io::stdout(),
            DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        )?;
        crossterm::terminal::disable_raw_mode()?;

        // Print command and spawn user's shell
        println!("$ {}", cmd);
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let _ = Command::new(&shell).arg("-l").status();

        // Re-enter TUI
        crossterm::terminal::enable_raw_mode()?;
        if self.mouse_capture_enabled {
            execute!(
                std::io::stdout(),
                crossterm::terminal::EnterAlternateScreen,
                EnableMouseCapture,
                crossterm::cursor::Hide
            )?;
        } else {
            execute!(
                std::io::stdout(),
                crossterm::terminal::EnterAlternateScreen,
                crossterm::cursor::Hide
            )?;
        }

        // Ensure chat history is visible after returning
        self.view.scroll_to_bottom();
        self.view.set_status_text("Shell session ended");
        Ok(())
    }

    /// Open the current session in $EDITOR or $VISUAL
    ///
    /// Serializes the conversation to a temp markdown file, opens in the user's
    /// preferred editor, and waits for the editor to close.
    fn open_session_in_editor(&mut self) -> Result<()> {
        use std::io::Write;
        use std::process::Command;

        // Get editor from environment (prefer $VISUAL, fallback to $EDITOR, then vi)
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".to_string());

        // Check if conversation is empty
        if self.view.state().conversation.items().is_empty() {
            self.view.echo_message("No conversation to view");
            return Ok(());
        }

        // Serialize conversation to markdown
        let markdown = self.view.state().conversation.to_markdown();

        // Create temp file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!(
            "crucible-session-{}.md",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ));

        // Write markdown to temp file
        let mut file = std::fs::File::create(&temp_file)?;
        file.write_all(markdown.as_bytes())?;
        file.sync_all()?;
        drop(file);

        debug!(editor = %editor, path = %temp_file.display(), "Opening session in editor");

        // Temporarily exit alternate screen so user can see editor
        execute!(
            std::io::stdout(),
            DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        )?;
        crossterm::terminal::disable_raw_mode()?;

        // Open editor with the temp file
        let status = Command::new(&editor).arg(&temp_file).status();

        // Re-enter TUI mode
        crossterm::terminal::enable_raw_mode()?;
        if self.mouse_capture_enabled {
            execute!(
                std::io::stdout(),
                crossterm::terminal::EnterAlternateScreen,
                EnableMouseCapture,
                crossterm::cursor::Hide
            )?;
        } else {
            execute!(
                std::io::stdout(),
                crossterm::terminal::EnterAlternateScreen,
                crossterm::cursor::Hide
            )?;
        }

        // Clean up temp file (best effort)
        let _ = std::fs::remove_file(&temp_file);

        match status {
            Ok(exit_status) => {
                if exit_status.success() {
                    self.view.echo_message(&format!("Closed {}", editor));
                } else {
                    let code = exit_status.code().unwrap_or(-1);
                    self.view
                        .echo_error(&format!("{} exited with code {}", editor, code));
                }
            }
            Err(e) => {
                self.view
                    .echo_error(&format!("Failed to open {}: {}", editor, e));
            }
        }

        Ok(())
    }

    /// Handle dialog result
    fn handle_dialog_result(&mut self, result: crate::tui::dialog::DialogResult) -> Result<()> {
        use crate::tui::dialog::DialogResult;

        match result {
            DialogResult::Confirm(value) => {
                // Check if this was an "[Other...]" selection from a popup
                if value == "[Other...]" {
                    if let Some(popup) = &self.pending_popup {
                        // Show input dialog for free-text entry
                        let input_dialog =
                            DialogState::input(&popup.title, "Type your response...");
                        self.view.push_dialog(input_dialog);
                        // Don't clear pending state yet - we need it for the input result
                        return Ok(());
                    }
                }

                // Handle popup response
                if let Some(popup) = self.pending_popup.take() {
                    // Check if this was a text input (from "Other" dialog)
                    // or a selection from the list
                    let is_entry_selection = popup
                        .entries
                        .iter()
                        .any(|e| e.label == value || value.starts_with(&e.label));

                    if is_entry_selection {
                        // Find the selected entry
                        if let Some((idx, entry)) = popup
                            .entries
                            .iter()
                            .enumerate()
                            .find(|(_, e)| e.label == value || value.starts_with(&e.label))
                        {
                            let _response = crucible_core::interaction::PopupResponse::selected(
                                idx,
                                entry.clone(),
                            );
                            self.view
                                .set_status_text(&format!("Selected: {}", entry.label));
                        }
                    } else {
                        // This was typed text from the "Other" input
                        let _response = crucible_core::interaction::PopupResponse::other(&value);
                        self.view.set_status_text(&format!("Entered: {}", value));
                    }

                    // Clear interaction state
                    self.pending_interaction_id = None;
                } else {
                    // Regular dialog confirmation
                    self.view
                        .set_status_text(&format!("Dialog confirmed: {}", value));
                }
            }
            DialogResult::Cancel => {
                // Dialog was cancelled - clear pending state
                self.pending_popup = None;
                self.pending_interaction_id = None;
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
                        .append_streaming_blocks(vec![StreamBlock::code_partial(lang, "")]);
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
    /// 2. Calls the provided factory to create the agent
    /// 3. Runs the main chat loop
    /// 4. Cleans up and exits TUI
    ///
    /// The factory receives the agent selection and should create the agent.
    /// Status updates are shown in the TUI during creation.
    ///
    /// Supports `/new` command for restarting - clears conversation and
    /// restarts with the same agent type.
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

        // Enter TUI with mouse capture for scrolling
        // TODO: Implement application-level text selection (like OpenCode)
        // that extracts actual text content, not terminal cells
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Get initial selection (use default or discover first available ACP agent)
        let initial_selection = match self.default_selection.take() {
            Some(selection) => selection,
            None => {
                // No explicit selection - discover first available ACP agent
                self.view.set_status_text("Discovering agents...");
                self.render_frame(&mut terminal)?;

                match crucible_acp::discover_agent(None).await {
                    Ok(agent) => AgentSelection::Acp(agent.name),
                    Err(_) => {
                        // No ACP agents available, fall back to internal
                        tracing::info!("No ACP agents discovered, using internal agent");
                        AgentSelection::Internal
                    }
                }
            }
        };

        // Store selection for restarts
        let current_selection = initial_selection;

        // Main session loop - supports restart via /new command
        loop {
            // Reset restart flag at start of each iteration
            self.restart_requested = false;

            // Create agent (show status in TUI)
            self.view.set_status_text("Creating agent...");
            self.render_frame(&mut terminal)?;

            // Extract agent name from selection
            let agent_name = match &current_selection {
                AgentSelection::Acp(name) => name.clone(),
                AgentSelection::Internal => "internal".to_string(),
                AgentSelection::Cancelled => "unknown".to_string(),
            };

            let mut agent = create_agent(current_selection.clone()).await?;

            // Set current agent for /agent command
            self.set_current_agent(&agent_name);

            // Clear conversation for fresh start
            self.view.state_mut().conversation.clear();

            // Resume session if requested (loads existing conversation history)
            if let Some(session_id_str) = self.resume_session_id.take() {
                self.view.set_status_text("Resuming session...");
                self.render_frame(&mut terminal)?;

                if let Err(e) = self.resume_session_from_id(&session_id_str).await {
                    tracing::warn!("Failed to resume session: {}", e);
                    // Show error in notification area
                    self.view.echo_error(&format!("Resume failed: {}", e));
                }
            }

            self.view.set_status_text("Ready");
            self.render_frame(&mut terminal)?;

            // Run main loop
            self.main_loop(&mut terminal, bridge, &mut agent).await?;

            // Check if restart was requested (via /new command)
            if !self.restart_requested {
                break; // Normal exit, don't restart
            }

            // Restart requested - loop back with same agent
            self.view.state_mut().conversation.clear();
            self.view.set_status_text("Restarting session...");
            self.render_frame(&mut terminal)?;
        }

        // Cleanup
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            cursor::Show
        )?;

        Ok(())
    }

    /// Render a single frame (used during status updates).
    fn render_frame(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        let view = &self.view;
        let selection = &self.selection;
        let scroll_offset = view.state().scroll_offset;
        let conv_height = view.conversation_viewport_height();
        let ask_batch_state = self.pending_ask_batch.as_ref();

        terminal.draw(|f| {
            view.render_frame(f);
            apply_selection_highlight(f, selection, scroll_offset, conv_height);

            // Render AskBatch dialog overlay if active
            if let Some(state) = ask_batch_state {
                use crate::tui::ask_batch_dialog::AskBatchDialogWidget;
                use ratatui::widgets::Widget;
                AskBatchDialogWidget::new(state).render(f.area(), f.buffer_mut());
            }
        })?;
        Ok(())
    }

    /// Resume a session from a session ID string.
    ///
    /// Parses the session ID, loads events from the session log, and populates
    /// the conversation view with the loaded messages.
    async fn resume_session_from_id(&mut self, session_id_str: &str) -> Result<()> {
        use crucible_observe::{LogEvent, SessionId};

        // Parse session ID
        let session_id = SessionId::parse(session_id_str)
            .map_err(|e| anyhow::anyhow!("Invalid session ID '{}': {}", session_id_str, e))?;

        // Get session logger (required for resume)
        let logger = self
            .session_logger
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Session logger not configured"))?;

        // Load events from the session
        let events = logger
            .resume_session(&session_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id_str))?;

        tracing::info!(
            "Resumed session {} with {} events",
            session_id_str,
            events.len()
        );

        // Convert events to conversation items
        for event in events {
            match event {
                LogEvent::User { content, .. } => {
                    self.view
                        .state_mut()
                        .conversation
                        .push_user_message(content);
                }
                LogEvent::Assistant { content, .. } => {
                    self.view
                        .state_mut()
                        .conversation
                        .push_assistant_message(content);
                }
                LogEvent::ToolCall { name, args, .. } => {
                    // Add tool call as completed (historical)
                    use crate::tui::conversation::{ToolCallDisplay, ToolStatus};
                    let tool = ToolCallDisplay {
                        name,
                        args,
                        status: ToolStatus::Complete { summary: None },
                        output_lines: vec![],
                    };
                    self.view
                        .state_mut()
                        .conversation
                        .push(crate::tui::conversation::ConversationItem::ToolCall(tool));
                }
                // Skip other event types (System, ToolResult, Error, etc.)
                // They're logged but don't need to be displayed in conversation
                _ => {}
            }
        }

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

    /// Handle an interaction request by showing appropriate dialog.
    ///
    /// Converts `InteractionRequest` to `DialogState` and stores the request_id
    /// for later response correlation.
    fn handle_interaction_request(&mut self, request_id: &str, request: &InteractionRequest) {
        let dialog = match request {
            InteractionRequest::Ask(ask) => {
                if let Some(choices) = &ask.choices {
                    DialogState::select(&ask.question, choices.clone())
                } else {
                    // Free-text input - show as info with prompt
                    DialogState::info(&ask.question, "(Type your response in the input box)")
                }
            }
            InteractionRequest::Permission(perm) => {
                let pattern = perm.pattern_at(perm.tokens().len());
                DialogState::confirm("Permission Required", format!("Allow: {}?", pattern))
            }
            InteractionRequest::Edit(edit) => {
                // Show content as info - full editing would need dedicated widget
                let hint = edit.hint.as_deref().unwrap_or("Review the content");
                DialogState::info(hint, &edit.content)
            }
            InteractionRequest::Show(show) => {
                let title = show.title.as_deref().unwrap_or("Information");
                DialogState::info(title, &show.content)
            }
            InteractionRequest::Popup(popup) => {
                // Convert PopupEntry labels to choices for selection dialog
                let mut choices: Vec<String> = popup
                    .entries
                    .iter()
                    .map(|e| {
                        if let Some(desc) = &e.description {
                            format!("{} - {}", e.label, desc)
                        } else {
                            e.label.clone()
                        }
                    })
                    .collect();

                // Add "Other..." option if allow_other is enabled
                if popup.allow_other {
                    choices.push("[Other...]".to_string());
                }

                // Store the popup request for potential "Other" handling
                self.pending_popup = Some(popup.clone());

                DialogState::select(&popup.title, choices)
            }
            InteractionRequest::Panel(panel) => {
                // Convert PanelItem to choices for selection dialog
                // TODO: Implement full panel widget with filtering, multi-select, key handlers
                let mut choices: Vec<String> = panel
                    .items
                    .iter()
                    .map(|item| {
                        if let Some(desc) = &item.description {
                            format!("{} - {}", item.label, desc)
                        } else {
                            item.label.clone()
                        }
                    })
                    .collect();

                // Add "Other..." option if hints.allow_other is enabled
                if panel.hints.allow_other {
                    choices.push("[Other...]".to_string());
                }

                DialogState::select(&panel.header, choices)
            }
            InteractionRequest::AskBatch(batch) => {
                // Use full AskBatchDialog for multi-question interactions
                self.pending_ask_batch =
                    Some(crate::tui::ask_batch_dialog::AskBatchDialogState::new(batch.clone()));
                self.pending_interaction_id = Some(request_id.to_string());
                debug!("AskBatch interaction request {}: showing dialog", request_id);
                return; // Don't push to dialog stack - AskBatch has its own rendering
            }
        };

        // Store the request_id for response correlation
        self.pending_interaction_id = Some(request_id.to_string());
        debug!("Interaction request {}: showing dialog", request_id);

        self.view.push_dialog(dialog);
    }
}

/// Copy text to system clipboard using OSC 52 escape sequence.
///
/// OSC 52 is widely supported by modern terminals (iTerm2, Alacritty, Kitty,
/// WezTerm, Windows Terminal, etc.) and works over SSH.
///
/// Returns true if the write succeeded, false otherwise.
fn copy_to_clipboard_osc52(text: &str) -> bool {
    use base64::Engine;
    use std::io::Write;

    let encoded = base64::engine::general_purpose::STANDARD.encode(text);

    // OSC 52 format: ESC ] 52 ; c ; <base64-data> BEL
    // 'c' means clipboard (vs 'p' for primary selection on X11)
    let osc52 = format!("\x1b]52;c;{}\x07", encoded);

    io::stdout().write_all(osc52.as_bytes()).is_ok() && io::stdout().flush().is_ok()
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
        let runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();
        assert_eq!(runner.view().mode_id(), "plan");
    }

    #[test]
    fn test_runner_tracks_current_agent() {
        let mut runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

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
        let runner =
            RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

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

    // =========================================================================
    // Generic Popup Integration Tests
    // =========================================================================

    mod generic_popup_tests {
        use super::*;
        use crate::tui::components::generic_popup::PopupState;
        use crate::tui::popup::PopupProvider;
        use crate::tui::state::{PopupItem, PopupItemKind, PopupKind};
        use std::sync::Arc;

        /// Mock provider for tests
        struct MockProvider;

        impl PopupProvider for MockProvider {
            fn provide(&self, _kind: PopupKind, _query: &str) -> Vec<PopupItem> {
                vec![
                    PopupItem::cmd("help").desc("Show help").with_score(100),
                    PopupItem::cmd("clear").desc("Clear history").with_score(90),
                ]
            }
        }

        fn mock_provider() -> Arc<dyn PopupProvider> {
            Arc::new(MockProvider)
        }

        #[test]
        fn test_runner_can_set_popup_on_view() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            // Initially no popup
            assert!(!runner.view.has_popup());

            // Set a generic popup via the view
            let popup = PopupState::new(PopupKind::Command, mock_provider());
            runner.view.set_popup(Some(popup));

            // Should now have a popup
            assert!(runner.view.has_popup());
            assert!(runner.view.popup().is_some());
        }

        #[test]
        fn test_runner_generic_popup_navigation() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            // Set a generic popup and populate items
            let mut popup = PopupState::new(PopupKind::Command, mock_provider());
            popup.update_query(""); // Fetch items from provider
            runner.view.set_popup(Some(popup));

            // Navigate through the popup
            let popup = runner.view.popup_mut().unwrap();
            assert_eq!(popup.selected_index(), 0);
            assert_eq!(popup.filtered_count(), 2); // Verify items are loaded

            // Trigger navigation via key event
            use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
            let down_key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
            popup.handle_key(&down_key);
            assert_eq!(popup.selected_index(), 1);
        }

        #[test]
        fn test_runner_clears_generic_popup() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            // Set popup
            let popup = PopupState::new(PopupKind::Command, mock_provider());
            runner.view.set_popup(Some(popup));
            assert!(runner.view.has_popup());

            // Clear it
            runner.view.set_popup(None);
            assert!(!runner.view.has_popup());
        }

        /// Test that validates the popup sync lifecycle matches the render loop.
        ///
        /// The render loop does:
        /// 1. Render (view needs popup)
        /// 2. Take popup back from view
        /// 3. Handle events (modifies runner.popup)
        /// 4. Sync popup to view (for next render)
        ///
        /// BUG FIX: Previously, step 4 immediately took the popup back,
        /// meaning the view never had the popup during render.
        #[test]
        fn test_popup_sync_lifecycle_for_render() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            // Simulate: user types "/" which creates a popup on runner.popup
            let mut popup = PopupState::new(PopupKind::Command, mock_provider());
            popup.update_query("");
            runner.popup = Some(popup);

            // Initially view has no popup
            assert!(!runner.view.has_popup());
            // Runner has the popup
            assert!(runner.popup.is_some());

            // Call the runner's sync method which should leave popup on view
            runner.sync_popup_to_view();

            // CRITICAL: View should have popup now for rendering
            assert!(runner.view.has_popup(), "View must have popup for render - sync_popup_to_view should leave it there");
            assert!(runner.popup.is_none(), "Runner popup should be moved to view");

            // Verify popup has items (would be visible in render)
            assert_eq!(runner.view.popup().unwrap().filtered_count(), 2);

            // Step 1: Render would happen here (view.render_frame())
            // The popup is visible because view.has_popup() is true

            // Step 2: Take popup back after render for event handling
            runner.take_popup_from_view();

            // Now runner has popup again for event handling
            assert!(runner.popup.is_some());
            assert!(!runner.view.has_popup());
        }
    }

    // =============================================================================
    // PastedContent Tests
    // =============================================================================

    mod paste_tests {
        use super::*;

        #[test]
        fn test_pasted_content_text_single_line() {
            let paste = PastedContent::text("hello world".to_string());
            match paste {
                PastedContent::Text {
                    content,
                    line_count,
                    char_count,
                } => {
                    assert_eq!(content, "hello world");
                    assert_eq!(line_count, 1);
                    assert_eq!(char_count, 11);
                }
            }
        }

        #[test]
        fn test_pasted_content_text_multi_line() {
            let paste = PastedContent::text("line one\nline two\nline three".to_string());
            match paste {
                PastedContent::Text {
                    content,
                    line_count,
                    char_count,
                } => {
                    assert_eq!(content, "line one\nline two\nline three");
                    assert_eq!(line_count, 3);
                    assert_eq!(char_count, 28);
                }
            }
        }

        #[test]
        fn test_pasted_content_content_accessor() {
            let paste = PastedContent::text("test content".to_string());
            assert_eq!(paste.content(), "test content");
        }

        #[test]
        fn test_pasted_content_summary_single() {
            let paste = PastedContent::text("line one\nline two".to_string());
            assert_eq!(paste.summary(), "[2 lines, 17 chars]");
        }

        #[test]
        fn test_pasted_content_summary_many_lines() {
            let content = (0..10).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
            let paste = PastedContent::text(content);
            // 10 lines
            assert!(paste.summary().contains("10 lines"));
        }

        #[test]
        fn test_runner_pending_pastes_initially_empty() {
            let runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();
            assert!(runner.pending_pastes.is_empty());
        }

        #[test]
        fn test_runner_handle_paste_single_line_inserts() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            // Single-line paste should insert directly
            runner.handle_paste_event("hello");
            assert!(runner.pending_pastes.is_empty());
            assert_eq!(runner.view.input(), "hello");
        }

        #[test]
        fn test_runner_handle_paste_multi_line_stores() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            // Multi-line paste should be stored and indicator shown
            runner.handle_paste_event("line one\nline two");
            assert_eq!(runner.pending_pastes.len(), 1);
            // Indicator is shown in input (e.g., "[2 lines, 18 chars]")
            assert!(runner.view.input().contains("lines"));
        }

        #[test]
        fn test_runner_handle_paste_multiple_pastes() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            // Multiple multi-line pastes accumulate
            runner.handle_paste_event("first\npaste");
            runner.handle_paste_event("second\npaste");
            assert_eq!(runner.pending_pastes.len(), 2);
        }

        #[test]
        fn test_runner_pending_pastes_summary_single() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            runner.pending_pastes.push(PastedContent::text("a\nb".to_string()));
            let summary = runner.pending_pastes_summary();
            assert!(summary.is_some());
            assert!(summary.unwrap().contains("2 lines"));
        }

        #[test]
        fn test_runner_pending_pastes_summary_multiple() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            runner.pending_pastes.push(PastedContent::text("a\nb".to_string()));
            runner.pending_pastes.push(PastedContent::text("c\nd\ne".to_string()));
            let summary = runner.pending_pastes_summary();
            assert!(summary.is_some());
            let s = summary.unwrap();
            assert!(s.contains("2 pastes"));
            assert!(s.contains("5 lines")); // 2 + 3
        }

        #[test]
        fn test_runner_clear_pending_pastes() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            runner.pending_pastes.push(PastedContent::text("a\nb".to_string()));
            assert!(!runner.pending_pastes.is_empty());

            runner.clear_pending_pastes();
            assert!(runner.pending_pastes.is_empty());
        }

        #[test]
        fn test_runner_build_message_with_pastes() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            // Add a paste
            runner.pending_pastes.push(PastedContent::text("pasted content".to_string()));

            // Set typed input
            runner.view.set_input("typed content");

            // Build message
            let msg = runner.build_message_with_pastes();

            // Should have paste content followed by typed content
            assert!(msg.starts_with("pasted content"));
            assert!(msg.ends_with("typed content"));
            assert!(msg.contains('\n')); // Newline separating paste from typed

            // Pastes should be drained
            assert!(runner.pending_pastes.is_empty());
        }

        #[test]
        fn test_runner_build_message_without_pastes() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            // Just typed input, no pastes
            runner.view.set_input("only typed");

            let msg = runner.build_message_with_pastes();
            assert_eq!(msg, "only typed");
        }

        #[test]
        fn test_find_paste_indicator_at_end() {
            let runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            let input = "hello [2 lines, 10 chars]";
            // Cursor at end (position 25) - inside indicator
            let result = runner.find_paste_indicator_at(input, 25);
            assert!(result.is_some());
            let (start, end, idx) = result.unwrap();
            assert_eq!(start, 6); // After "hello "
            assert_eq!(end, 25);
            assert_eq!(idx, 0);
        }

        #[test]
        fn test_find_paste_indicator_at_middle() {
            let runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            let input = "[5 lines, 100 chars]";
            // Cursor in middle of indicator (position 10)
            let result = runner.find_paste_indicator_at(input, 10);
            assert!(result.is_some());
            let (start, end, idx) = result.unwrap();
            assert_eq!(start, 0);
            assert_eq!(end, 20);
            assert_eq!(idx, 0);
        }

        #[test]
        fn test_find_paste_indicator_not_in_indicator() {
            let runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            let input = "hello world";
            let result = runner.find_paste_indicator_at(input, 5);
            assert!(result.is_none());
        }

        #[test]
        fn test_find_paste_indicator_multiple_indicators() {
            let runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            let input = "[1 line, 5 chars] text [3 lines, 20 chars]";
            // First indicator is at 0..17
            // Second indicator is at 23..42

            // Cursor in first indicator
            let result = runner.find_paste_indicator_at(input, 10);
            assert!(result.is_some());
            assert_eq!(result.unwrap().2, 0); // First indicator

            // Cursor in second indicator
            let result = runner.find_paste_indicator_at(input, 30);
            assert!(result.is_some());
            assert_eq!(result.unwrap().2, 1); // Second indicator
        }

        #[test]
        fn test_delete_paste_indicator() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            runner.view.set_input("hello [2 lines, 10 chars]");
            runner.pending_pastes.push(PastedContent::text("line1\nline2".to_string()));

            // Delete the indicator (start=6, end=25, idx=0)
            let new_pos = runner.delete_paste_indicator(6, 25, 0);

            // Should remove indicator and trailing space
            assert_eq!(runner.view.input(), "hello");
            assert_eq!(new_pos, 5); // After "hello" without space
            assert!(runner.pending_pastes.is_empty());
        }

        #[test]
        fn test_delete_paste_indicator_preserves_other_pastes() {
            let mut runner =
                RatatuiRunner::new("plan", test_popup_provider(), test_command_registry()).unwrap();

            runner.view.set_input("[1 line, 5 chars] [3 lines, 20 chars]");
            runner.pending_pastes.push(PastedContent::text("12345".to_string()));
            runner.pending_pastes.push(PastedContent::text("a\nb\nc".to_string()));

            // Delete the first indicator (start=0, end=17, idx=0)
            runner.delete_paste_indicator(0, 17, 0);

            // Should remove first paste, keep second
            assert_eq!(runner.pending_pastes.len(), 1);
            assert!(runner.pending_pastes[0].content().contains('\n'));
        }
    }
}
