//! Conversation view abstraction
//!
//! Provides a trait for rendering conversation history with full ratatui control.

use crate::tui::components::{
    InputBoxWidget, PopupState, SessionHistoryWidget, StatusBarWidget, DEFAULT_MAX_INPUT_LINES,
};
use crate::tui::conversation::{render_item_to_lines, ConversationState, StatusKind};
use crate::tui::dialog::{DialogResult, DialogStack, DialogWidget};
use crate::tui::notification::NotificationState;
use anyhow::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
    Frame,
};

// =============================================================================
// View Trait
// =============================================================================

/// Abstraction for conversation rendering
pub trait ConversationView {
    /// Push a user message to the view
    fn push_user_message(&mut self, content: &str) -> Result<()>;

    /// Push an assistant message to the view
    fn push_assistant_message(&mut self, content: &str) -> Result<()>;

    /// Set the current status (thinking, generating, etc.)
    fn set_status(&mut self, status: StatusKind);

    /// Clear the status indicator
    fn clear_status(&mut self);

    /// Push a tool call (running state) with arguments
    fn push_tool_running(&mut self, name: &str, args: serde_json::Value);

    /// Update tool output (streaming)
    fn update_tool_output(&mut self, name: &str, output: &str);

    /// Mark a tool as complete
    fn complete_tool(&mut self, name: &str, summary: Option<String>);

    /// Mark a tool as errored
    fn error_tool(&mut self, name: &str, message: &str);

    /// Render the view (implementation-specific)
    fn render(&mut self) -> Result<()>;

    /// Handle terminal resize
    fn handle_resize(&mut self, width: u16, height: u16) -> Result<()>;

    /// Get/set input state (for the input box)
    fn input(&self) -> &str;
    fn set_input(&mut self, input: &str);
    fn cursor_position(&self) -> usize;
    fn set_cursor_position(&mut self, pos: usize);

    /// Mode and status for status bar
    fn mode_id(&self) -> &str;
    fn set_mode_id(&mut self, mode: &str);
    fn token_count(&self) -> Option<usize>;
    fn set_token_count(&mut self, count: Option<usize>);
    fn status_text(&self) -> &str;
    fn set_status_text(&mut self, status: &str);
    /// Set status text and record to message history for :messages
    fn echo_message(&mut self, message: &str);
    /// Record an error to message history
    fn echo_error(&mut self, message: &str);

    /// Scroll control (vertical)
    fn scroll_up(&mut self, lines: usize);
    fn scroll_down(&mut self, lines: usize);
    fn scroll_to_top(&mut self);
    fn scroll_to_bottom(&mut self);

    /// Scroll control (horizontal) - for wide content like tables
    fn scroll_left(&mut self, cols: usize);
    fn scroll_right(&mut self, cols: usize);
    fn scroll_to_left_edge(&mut self);
    fn scroll_to_right_edge(&mut self);

    /// Check if horizontal scrolling is available (content wider than viewport)
    fn has_horizontal_overflow(&self) -> bool;
}

// =============================================================================
// View State
// =============================================================================

/// State for the ratatui view
#[derive(Debug)]
pub struct ViewState {
    pub conversation: ConversationState,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub mode_id: String,
    pub token_count: Option<usize>,
    pub status_text: String,
    pub scroll_offset: usize,
    /// True if user is at bottom (auto-scroll enabled)
    pub at_bottom: bool,
    pub width: u16,
    pub height: u16,
    /// Popup state for slash commands / agents / files
    pub popup: Option<PopupState>,
    /// Dialog stack for modal dialogs
    pub dialog_stack: DialogStack,
    /// Notification state for file watch events
    pub notifications: NotificationState,
    /// Whether to show reasoning/thinking content (Alt+T toggle)
    pub show_reasoning: bool,
    /// Accumulated reasoning content from thinking models
    pub reasoning_content: String,
    /// Animation frame for reasoning ellipsis (cycles 0-3)
    pub reasoning_anim_frame: u8,
    /// Index of the conversation item currently focused for horizontal scroll
    /// None means no wide content or auto-focus most recent
    pub focused_wide_item: Option<usize>,
    /// Horizontal scroll offset for the focused wide item
    pub horizontal_scroll_offset: usize,
    /// Width of the focused wide item's content (for scroll bounds)
    pub focused_item_width: usize,
}

impl ViewState {
    pub fn new(mode_id: &str, width: u16, height: u16) -> Self {
        Self {
            conversation: ConversationState::new(),
            input_buffer: String::new(),
            cursor_position: 0,
            mode_id: mode_id.to_string(),
            token_count: None,
            status_text: "Ready".to_string(),
            scroll_offset: 0,
            at_bottom: true,
            width,
            height,
            popup: None,
            dialog_stack: DialogStack::new(),
            notifications: NotificationState::new(),
            show_reasoning: false,
            reasoning_content: String::new(),
            reasoning_anim_frame: 0,
            focused_wide_item: None,
            horizontal_scroll_offset: 0,
            focused_item_width: 0,
        }
    }

    /// Returns the display offset for cursor positioning.
    ///
    /// When the input starts with `:` or `!`, the prefix is shown as the prompt
    /// and stripped from the content display. This returns 1 in that case so
    /// the cursor position can be adjusted.
    pub fn input_display_offset(&self) -> usize {
        let trimmed = self.input_buffer.trim_start();
        if trimmed.starts_with(':') || trimmed.starts_with('!') {
            1
        } else {
            0
        }
    }

    /// Count lines in the input buffer for dynamic height calculation
    pub fn input_line_count(&self) -> usize {
        if self.input_buffer.is_empty() {
            1
        } else {
            self.input_buffer.lines().count().max(1)
                + if self.input_buffer.ends_with('\n') {
                    1
                } else {
                    0
                }
        }
    }

    /// Calculate the required input box height
    ///
    /// Returns the visible height including padding (2 lines for top/bottom).
    pub fn input_box_height(&self) -> u16 {
        let lines = self.input_line_count() as u16;
        lines.min(DEFAULT_MAX_INPUT_LINES) + 2 // +2 for padding
    }

    /// Convert cursor byte offset to (line, column) position
    ///
    /// Returns (line_index, column_index) where both are 0-based.
    pub fn cursor_to_line_col(&self) -> (usize, usize) {
        let before_cursor = &self.input_buffer[..self.cursor_position.min(self.input_buffer.len())];
        let line = before_cursor.matches('\n').count();
        let last_newline = before_cursor.rfind('\n');
        let col = match last_newline {
            Some(pos) => before_cursor.len() - pos - 1,
            None => before_cursor.len(),
        };
        (line, col)
    }

    /// Calculate scroll offset to keep cursor visible in input area
    pub fn input_scroll_offset(&self) -> usize {
        let (cursor_line, _) = self.cursor_to_line_col();
        let visible_lines = DEFAULT_MAX_INPUT_LINES as usize;
        let total_lines = self.input_line_count();

        if total_lines <= visible_lines {
            // No scrolling needed
            0
        } else if cursor_line < visible_lines.saturating_sub(1) {
            // Cursor near top
            0
        } else {
            // Scroll to keep cursor visible (prefer keeping cursor near bottom)
            cursor_line.saturating_sub(visible_lines - 1)
        }
    }
}

// =============================================================================
// Ratatui Implementation
// =============================================================================

/// Full ratatui-controlled view
///
/// Uses alternate screen with complete viewport control.
/// Manages its own scrollback buffer.
pub struct RatatuiView {
    state: ViewState,
}

impl RatatuiView {
    pub fn new(mode_id: &str, width: u16, height: u16) -> Self {
        Self {
            state: ViewState::new(mode_id, width, height),
        }
    }

    /// Maximum popup items to display
    const MAX_POPUP_ITEMS: usize = 5;

    /// Maximum height for reasoning panel (lines)
    const MAX_REASONING_HEIGHT: u16 = 6;

    /// Render to a ratatui frame
    pub fn render_frame(&self, frame: &mut Frame) {
        // Calculate popup height (no border, so no +2)
        let popup_height = self
            .state
            .popup
            .as_ref()
            .filter(|p| p.filtered_count() > 0)
            .map(|p| p.filtered_count().min(Self::MAX_POPUP_ITEMS) as u16)
            .unwrap_or(0);

        // Calculate reasoning panel height (when visible and has content)
        let reasoning_height =
            if self.state.show_reasoning && !self.state.reasoning_content.is_empty() {
                // Count lines in reasoning content (min 3 for border + header + 1 line)
                let content_lines = self.state.reasoning_content.lines().count() as u16;
                (content_lines + 2).min(Self::MAX_REASONING_HEIGHT) // +2 for borders
            } else {
                0
            };

        let mut constraints = vec![Constraint::Min(3)]; // Conversation area

        // Add reasoning panel if visible
        if reasoning_height > 0 {
            constraints.push(Constraint::Length(reasoning_height));
        }

        constraints.push(Constraint::Length(1)); // Spacer above input

        // Add popup if active
        if popup_height > 0 {
            constraints.push(Constraint::Length(popup_height));
        }

        constraints.push(Constraint::Length(self.state.input_box_height())); // Input box (dynamic)
        constraints.push(Constraint::Length(1)); // Status bar

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(frame.area());

        let mut idx = 0;

        // Conversation (using SessionHistoryWidget)
        let conv_area = chunks[idx];
        let conv_widget = SessionHistoryWidget::new(&self.state.conversation)
            .scroll_offset(self.state.scroll_offset)
            .viewport_height(conv_area.height)
            .horizontal_offset(self.state.horizontal_scroll_offset);
        frame.render_widget(conv_widget, conv_area);
        idx += 1;

        // Reasoning panel (if visible)
        if reasoning_height > 0 {
            self.render_reasoning_panel(frame, chunks[idx]);
            idx += 1;
        }

        // Spacer (visual separation before input - just skip it, it remains empty)
        idx += 1;

        // Popup (if active)
        if popup_height > 0 {
            self.render_popup(frame, chunks[idx]);
            idx += 1;
        }

        // Input box (dynamic height with multiline support)
        let input_area = chunks[idx];
        let input_scroll = self.state.input_scroll_offset();
        let input_widget =
            InputBoxWidget::new(&self.state.input_buffer, self.state.cursor_position)
                .scroll_offset(input_scroll);
        frame.render_widget(input_widget, input_area);
        idx += 1;

        // Status bar with notification support
        let notification = self.state.notifications.current();
        let mut status_widget = StatusBarWidget::new(&self.state.mode_id, &self.state.status_text);
        if let Some(count) = self.state.token_count {
            status_widget = status_widget.token_count(count);
        }
        status_widget = status_widget.notification(notification);
        frame.render_widget(status_widget, chunks[idx]);

        // Render dialog on top if present (overlays everything)
        if let Some(dialog) = self.state.dialog_stack.current() {
            let widget = DialogWidget::new(dialog);
            frame.render_widget(widget, frame.area());
            // Hide cursor when dialog is active
        } else {
            // Position cursor in input box (accounting for multiline)
            let (cursor_line, cursor_col) = self.state.cursor_to_line_col();
            let scroll_offset = self.state.input_scroll_offset();
            let visible_cursor_line = cursor_line.saturating_sub(scroll_offset);

            // When prefix (: or !) is shown as prompt, adjust cursor column
            let display_offset = self.state.input_display_offset();
            let display_col = if cursor_line == 0 {
                cursor_col.saturating_sub(display_offset)
            } else {
                cursor_col
            };

            // Calculate vertical centering offset (same as InputBoxWidget)
            let content_lines = self
                .state
                .input_line_count()
                .min(DEFAULT_MAX_INPUT_LINES as usize);
            let content_height = content_lines as u16;
            let start_y = if content_height < input_area.height {
                input_area.y + (input_area.height - content_height) / 2
            } else {
                input_area.y
            };

            // Prompt is 3 chars (" > " or "   ")
            let prompt_width = 3;
            let cursor_x = input_area.x + prompt_width + display_col as u16;
            let cursor_y = start_y + visible_cursor_line as u16;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    /// Render the reasoning/thinking panel
    fn render_reasoning_panel(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Truncate content to fit in available height (minus borders)
        let max_lines = (area.height.saturating_sub(2)) as usize;
        let content: String = self
            .state
            .reasoning_content
            .lines()
            .take(max_lines)
            .collect::<Vec<_>>()
            .join("\n");

        // Animated ellipsis based on frame (cycles: "" -> "." -> ".." -> "...")
        let ellipsis = match self.state.reasoning_anim_frame % 4 {
            0 => "",
            1 => ".",
            2 => "..",
            _ => "...",
        };
        let title = format!("Thinking{}", ellipsis);

        let reasoning_widget = Paragraph::new(content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(Span::styled(
                        title,
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::ITALIC),
                    )),
            )
            .style(
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(reasoning_widget, area);
    }

    /// Render popup overlay
    fn render_popup(&self, frame: &mut Frame, area: Rect) {
        if let Some(ref popup) = self.state.popup {
            let renderer = popup.renderer();
            renderer.render(area, frame.buffer_mut());
        }
    }

    /// Get inner state reference
    pub fn state(&self) -> &ViewState {
        &self.state
    }

    /// Get mutable inner state reference
    pub fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Set popup state
    pub fn set_popup(&mut self, popup: Option<PopupState>) {
        self.state.popup = popup;
    }

    /// Get popup state reference
    pub fn popup(&self) -> Option<&PopupState> {
        self.state.popup.as_ref()
    }

    /// Get mutable popup state reference
    pub fn popup_mut(&mut self) -> Option<&mut PopupState> {
        self.state.popup.as_mut()
    }

    /// Take the popup state (for passing back to runner)
    pub fn popup_take(&mut self) -> Option<PopupState> {
        self.state.popup.take()
    }

    /// Check if popup is active
    pub fn has_popup(&self) -> bool {
        self.state.popup.is_some()
    }

    /// Calculate total content height for scroll bounds
    fn content_height(&self) -> usize {
        // Content width minus prefix (" · " = 3 chars) and right margin (1 char)
        // Must match ConversationWidget::render
        let content_width = (self.state.width as usize).saturating_sub(4);
        self.state
            .conversation
            .items()
            .iter()
            .map(|item| render_item_to_lines(item, content_width).len())
            .sum()
    }

    /// Update max_content_width based on current content
    ///
    /// Should be called after content changes to enable horizontal scrolling
    /// for wide content (tables, code blocks).
    /// Detect wide items and auto-focus the most recent one for horizontal scrolling.
    ///
    /// Scans conversation items to find those wider than viewport.
    /// Auto-focuses the most recent (last) wide item for shift+scroll.
    pub fn update_wide_content_focus(&mut self) {
        let viewport_width = self.state.width as usize;
        let content_width = viewport_width.saturating_sub(4); // Account for margins

        let items = self.state.conversation.items();
        let mut last_wide_item: Option<(usize, usize)> = None; // (index, width)

        for (idx, item) in items.iter().enumerate() {
            let lines = render_item_to_lines(item, content_width);
            let item_max_width: usize = lines
                .iter()
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.content.chars().count())
                        .sum()
                })
                .max()
                .unwrap_or(0);

            // Check if this item is wider than viewport
            if item_max_width > viewport_width {
                last_wide_item = Some((idx, item_max_width));
            }
        }

        // Update focus to most recent wide item
        if let Some((idx, width)) = last_wide_item {
            // If focus changed, reset scroll offset
            if self.state.focused_wide_item != Some(idx) {
                self.state.horizontal_scroll_offset = 0;
            }
            self.state.focused_wide_item = Some(idx);
            self.state.focused_item_width = width;
        } else {
            self.state.focused_wide_item = None;
            self.state.focused_item_width = 0;
            self.state.horizontal_scroll_offset = 0;
        }
    }

    /// Calculate the actual conversation viewport height based on current state.
    ///
    /// This accounts for UI elements that reduce the available conversation area:
    /// - Input box (dynamic height based on line count)
    /// - Status bar (1 line)
    /// - Spacer above input (1 line)
    /// - Reasoning panel (variable, if visible and has content)
    /// - Popup (variable, if active)
    pub fn conversation_viewport_height(&self) -> usize {
        let input_height = self.state.input_box_height();
        let mut overhead: u16 = input_height + 2; // input (dynamic) + status (1) + spacer (1)

        // Add reasoning panel height if visible
        if self.state.show_reasoning && !self.state.reasoning_content.is_empty() {
            let content_lines = self.state.reasoning_content.lines().count() as u16;
            overhead += (content_lines + 2).min(Self::MAX_REASONING_HEIGHT);
        }

        // Add popup height if active
        if let Some(ref popup) = self.state.popup {
            let count = popup.filtered_count();
            if count > 0 {
                overhead += (count.min(Self::MAX_POPUP_ITEMS) + 2) as u16;
            }
        }

        (self.state.height as usize).saturating_sub(overhead as usize)
    }

    /// Build selection cache data for text extraction.
    ///
    /// Returns cache info for all rendered lines in the conversation.
    /// Call this after content changes or when the selection cache needs rebuilding.
    pub fn build_selection_cache(&self) -> Vec<crate::tui::selection::RenderedLineInfo> {
        use crate::tui::components::SessionHistoryWidget;

        // Content width must match what's used in render
        let content_width = (self.state.width as usize).saturating_sub(4);
        let widget = SessionHistoryWidget::new(&self.state.conversation);
        let (_lines, cache_info) = widget.render_to_lines_with_cache(content_width);
        cache_info
    }

    /// Start streaming an assistant message (creates empty message with streaming indicator)
    pub fn start_assistant_streaming(&mut self) {
        self.state.conversation.start_assistant_streaming();
    }

    /// Append content blocks to the streaming assistant message
    pub fn append_streaming_blocks(&mut self, blocks: Vec<crate::tui::StreamBlock>) {
        self.state.conversation.append_streaming_blocks(blocks);
        // Only auto-scroll if user was at bottom (allows reading while streaming)
        if self.state.at_bottom {
            self.scroll_to_bottom();
        }
    }

    /// Mark the streaming assistant message as complete
    pub fn complete_assistant_streaming(&mut self) {
        self.state.conversation.complete_streaming();
    }

    /// Append content to the last block of the streaming message
    pub fn append_to_last_block(&mut self, content: &str) {
        self.state.conversation.append_to_last_block(content);
    }

    /// Mark the last block as complete
    pub fn complete_last_block(&mut self) {
        self.state.conversation.complete_last_block();
    }

    /// Append text to the last prose block, or create a new one if needed
    /// Used for streaming to consolidate continuous prose text
    pub fn append_or_create_prose(&mut self, text: &str) {
        self.state.conversation.append_or_create_prose(text);
        // Only auto-scroll if user was at bottom (allows reading while streaming)
        if self.state.at_bottom {
            self.scroll_to_bottom();
        }
    }

    // =========================================================================
    // Reasoning Panel Methods
    // =========================================================================

    /// Get current reasoning content
    pub fn reasoning(&self) -> &str {
        &self.state.reasoning_content
    }

    /// Set reasoning content
    pub fn set_reasoning(&mut self, content: &str) {
        self.state.reasoning_content = content.to_string();
    }

    /// Clear reasoning content
    pub fn clear_reasoning(&mut self) {
        self.state.reasoning_content.clear();
    }

    /// Append to reasoning content
    pub fn append_reasoning(&mut self, content: &str) {
        self.state.reasoning_content.push_str(content);
    }

    /// Check if reasoning panel is visible
    pub fn show_reasoning(&self) -> bool {
        self.state.show_reasoning
    }

    /// Set reasoning panel visibility
    pub fn set_show_reasoning(&mut self, show: bool) {
        self.state.show_reasoning = show;
    }

    /// Advance reasoning animation frame (call on each reasoning delta)
    pub fn tick_reasoning_animation(&mut self) {
        self.state.reasoning_anim_frame = (self.state.reasoning_anim_frame + 1) % 4;
    }

    // =========================================================================
    // Dialog Methods
    // =========================================================================

    /// Push a dialog onto the stack
    pub fn push_dialog(&mut self, dialog: crate::tui::dialog::DialogState) {
        self.state.dialog_stack.push(dialog);
    }

    /// Check if a dialog is currently active
    pub fn has_dialog(&self) -> bool {
        !self.state.dialog_stack.is_empty()
    }

    /// Handle key event for the current dialog
    pub fn handle_dialog_key(&mut self, key: crossterm::event::KeyEvent) -> Option<DialogResult> {
        self.state.dialog_stack.handle_key(key)
    }

    /// Example: Route events using LayerStack for composable event handling
    ///
    /// This demonstrates how LayerStack can be used to route events through
    /// the layer hierarchy (base → popup → modal) with proper focus management.
    ///
    /// Note: Currently unused, as the runner uses direct event handling.
    /// Kept as reference for the component architecture pattern.
    #[allow(dead_code)]
    fn route_event_via_layer_stack(
        &mut self,
        event: &crossterm::event::Event,
    ) -> crate::tui::components::WidgetEventResult {
        use crate::tui::components::{FocusTarget, LayerStack};

        // Determine current focus based on active layers
        let focus = if self.state.dialog_stack.current().is_some() {
            FocusTarget::Dialog
        } else if self.state.popup.is_some() {
            FocusTarget::Popup
        } else {
            FocusTarget::Input
        };

        // Create layer stack for event routing
        let mut stack = LayerStack::new(focus);

        // Add popup layer if active
        // Note: This is a demonstration pattern. Actual popup widgets would need
        // to implement InteractiveWidget trait.
        // if let Some(popup) = &mut self.state.popup {
        //     stack.set_popup(popup_widget);
        // }

        // Add modal layer if active
        // if let Some(dialog) = self.state.dialog_stack.current_mut() {
        //     stack.set_modal(dialog_widget);
        // }

        // Route the event through the stack
        stack.route_event(event)

        // In practice, the runner would then handle the WidgetEventResult:
        // - Consumed: event was handled, stop propagation
        // - Ignored: continue to base layer handlers
        // - Action(action): process the requested action
    }
}

impl ConversationView for RatatuiView {
    fn push_user_message(&mut self, content: &str) -> Result<()> {
        self.state.conversation.push_user_message(content);
        self.update_wide_content_focus();
        self.scroll_to_bottom();
        Ok(())
    }

    fn push_assistant_message(&mut self, content: &str) -> Result<()> {
        self.state.conversation.push_assistant_message(content);
        self.update_wide_content_focus();
        // Only auto-scroll if user was at bottom (allows reading while assistant responds)
        if self.state.at_bottom {
            self.scroll_to_bottom();
        }
        Ok(())
    }

    fn set_status(&mut self, status: StatusKind) {
        self.state.conversation.set_status(status);
    }

    fn clear_status(&mut self) {
        self.state.conversation.clear_status();
    }

    fn push_tool_running(&mut self, name: &str, args: serde_json::Value) {
        self.state.conversation.push_tool_running(name, args);
    }

    fn update_tool_output(&mut self, name: &str, output: &str) {
        self.state.conversation.update_tool_output(name, output);
    }

    fn complete_tool(&mut self, name: &str, summary: Option<String>) {
        self.state.conversation.complete_tool(name, summary);
    }

    fn error_tool(&mut self, name: &str, message: &str) {
        self.state.conversation.error_tool(name, message);
    }

    fn render(&mut self) -> Result<()> {
        // This is a no-op - actual rendering happens via render_frame()
        // which is called by the terminal.draw() in the runner
        Ok(())
    }

    fn handle_resize(&mut self, width: u16, height: u16) -> Result<()> {
        self.state.width = width;
        self.state.height = height;
        Ok(())
    }

    fn input(&self) -> &str {
        &self.state.input_buffer
    }

    fn set_input(&mut self, input: &str) {
        self.state.input_buffer = input.to_string();
    }

    fn cursor_position(&self) -> usize {
        self.state.cursor_position
    }

    fn set_cursor_position(&mut self, pos: usize) {
        self.state.cursor_position = pos;
    }

    fn mode_id(&self) -> &str {
        &self.state.mode_id
    }

    fn set_mode_id(&mut self, mode: &str) {
        self.state.mode_id = mode.to_string();
    }

    fn token_count(&self) -> Option<usize> {
        self.state.token_count
    }

    fn set_token_count(&mut self, count: Option<usize>) {
        self.state.token_count = count;
    }

    fn status_text(&self) -> &str {
        &self.state.status_text
    }

    fn set_status_text(&mut self, status: &str) {
        self.state.status_text = status.to_string();
    }

    fn echo_message(&mut self, message: &str) {
        use crate::tui::notification::NotificationLevel;
        self.state.status_text = message.to_string();
        self.state
            .notifications
            .push_message(message, NotificationLevel::Info);
    }

    fn echo_error(&mut self, message: &str) {
        use crate::tui::notification::NotificationLevel;
        self.state.status_text = message.to_string();
        self.state
            .notifications
            .push_message(message, NotificationLevel::Error);
    }

    fn scroll_up(&mut self, lines: usize) {
        self.state.scroll_offset = self.state.scroll_offset.saturating_add(lines);
        // Clamp to content bounds using actual conversation viewport height
        let viewport_height = self.conversation_viewport_height();
        let max_scroll = self.content_height().saturating_sub(viewport_height);
        self.state.scroll_offset = self.state.scroll_offset.min(max_scroll);
        self.state.at_bottom = false; // User scrolled up
    }

    fn scroll_down(&mut self, lines: usize) {
        self.state.scroll_offset = self.state.scroll_offset.saturating_sub(lines);
        if self.state.scroll_offset == 0 {
            self.state.at_bottom = true; // Back at bottom
        }
    }

    fn scroll_to_top(&mut self) {
        // Use actual conversation viewport height for max scroll calculation
        let viewport_height = self.conversation_viewport_height();
        let max_scroll = self.content_height().saturating_sub(viewport_height);
        self.state.scroll_offset = max_scroll;
        self.state.at_bottom = false;
    }

    fn scroll_to_bottom(&mut self) {
        self.state.scroll_offset = 0;
        self.state.at_bottom = true;
    }

    fn scroll_left(&mut self, cols: usize) {
        if self.state.focused_wide_item.is_some() {
            self.state.horizontal_scroll_offset =
                self.state.horizontal_scroll_offset.saturating_sub(cols);
        }
    }

    fn scroll_right(&mut self, cols: usize) {
        if self.state.focused_wide_item.is_some() {
            let viewport_width = self.state.width as usize;
            let max_offset = self
                .state
                .focused_item_width
                .saturating_sub(viewport_width);
            self.state.horizontal_scroll_offset =
                (self.state.horizontal_scroll_offset + cols).min(max_offset);
        }
    }

    fn scroll_to_left_edge(&mut self) {
        self.state.horizontal_scroll_offset = 0;
    }

    fn scroll_to_right_edge(&mut self) {
        if self.state.focused_wide_item.is_some() {
            let viewport_width = self.state.width as usize;
            self.state.horizontal_scroll_offset = self
                .state
                .focused_item_width
                .saturating_sub(viewport_width);
        }
    }

    fn has_horizontal_overflow(&self) -> bool {
        self.state.focused_wide_item.is_some()
            && self.state.focused_item_width > self.state.width as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::components::PopupState;
    use crate::tui::state::{PopupItem, PopupItemKind, PopupKind};
    use ratatui::{backend::TestBackend, Terminal};
    use std::sync::Arc;
    #[allow(unused_imports)]
    use std::time::Instant;

    #[test]
    fn test_view_state_new() {
        let state = ViewState::new("plan", 80, 24);
        assert_eq!(state.mode_id, "plan");
        assert_eq!(state.width, 80);
        assert_eq!(state.height, 24);
        assert!(state.input_buffer.is_empty());
    }

    #[test]
    fn test_ratatui_view_push_messages() {
        let mut view = RatatuiView::new("plan", 80, 24);

        view.push_user_message("Hello").unwrap();
        view.push_assistant_message("Hi there!").unwrap();

        assert_eq!(view.state().conversation.items().len(), 2);
    }

    #[test]
    fn test_ratatui_view_scroll() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add enough content to exceed viewport (24 lines)
        // Each user message is 2 lines (blank + content), so need >12 messages
        for i in 0..15 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }

        // Should be at bottom
        assert_eq!(view.state().scroll_offset, 0);

        // Scroll up - should work since 15 * 2 = 30 lines > 24 viewport
        view.scroll_up(5);
        assert!(
            view.state().scroll_offset > 0,
            "scroll_offset should be > 0 after scrolling up with 30 lines of content"
        );

        // Scroll back down
        view.scroll_to_bottom();
        assert_eq!(view.state().scroll_offset, 0);
    }

    /// Test that popup is rendered in RatatuiView::render_frame
    /// This test would have FAILED before the fix because render_frame
    /// didn't render the popup at all.
    #[test]
    fn test_ratatui_view_renders_popup() {
        use crate::tui::popup::PopupProvider;

        struct TestProvider;
        impl PopupProvider for TestProvider {
            fn provide(&self, _kind: PopupKind, _query: &str) -> Vec<PopupItem> {
                vec![PopupItem::cmd("help").desc("Show help")]
            }
        }

        let mut view = RatatuiView::new("plan", 80, 24);
        let mut popup = PopupState::new(PopupKind::Command, Arc::new(TestProvider));
        popup.update_query(""); // Load items from provider
        view.set_popup(Some(popup));

        // Render to test backend
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view.render_frame(f)).unwrap();

        // Get the buffer content as string
        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // The popup should contain "help" (no prefix icon now)
        assert!(
            content.contains("help"),
            "Popup should render 'help' command. Buffer content: {}",
            content
        );
        // Kind labels are no longer shown - trigger char indicates type
        assert!(
            !content.contains("[cmd]"),
            "Popup should NOT render '[cmd]' label (removed). Buffer content: {}",
            content
        );
    }

    /// Test that skill items render (kind labels removed)
    #[test]
    fn test_ratatui_view_renders_skill_popup() {
        use crate::tui::popup::PopupProvider;

        struct SkillProvider;
        impl PopupProvider for SkillProvider {
            fn provide(&self, _kind: PopupKind, _query: &str) -> Vec<PopupItem> {
                vec![PopupItem::skill("git-commit").desc("Create commits (personal)")]
            }
        }

        let mut view = RatatuiView::new("plan", 80, 24);
        let mut popup = PopupState::new(PopupKind::Command, Arc::new(SkillProvider));
        popup.update_query("");
        view.set_popup(Some(popup));

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view.render_frame(f)).unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // Kind labels are no longer shown - trigger char indicates type
        assert!(
            !content.contains("[skill]"),
            "Popup should NOT render '[skill]' label (removed). Buffer: {}",
            content
        );
        // Skill items show the skill name (the display format doesn't include "skill:" prefix)
        assert!(
            content.contains("git-commit"),
            "Popup should render skill name. Buffer: {}",
            content
        );
    }

    // =============================================================================
    // Bottom-Anchored Rendering Tests
    // =============================================================================

    #[test]
    fn test_at_bottom_tracking() {
        let mut view = RatatuiView::new("plan", 80, 24);
        assert!(view.state().at_bottom);

        // Add content
        for i in 0..20 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }
        assert!(view.state().at_bottom);

        // Scroll up - no longer at bottom
        view.scroll_up(5);
        assert!(!view.state().at_bottom);

        // Scroll back to bottom
        view.scroll_to_bottom();
        assert!(view.state().at_bottom);
    }

    #[test]
    fn test_at_bottom_scroll_down_to_zero() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add content and scroll up
        for i in 0..20 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }
        view.scroll_up(10);
        assert!(!view.state().at_bottom);
        assert_eq!(view.state().scroll_offset, 10);

        // Scroll down to exactly 0
        view.scroll_down(10);
        assert_eq!(view.state().scroll_offset, 0);
        assert!(
            view.state().at_bottom,
            "Should be at_bottom when scroll_offset reaches 0"
        );
    }

    #[test]
    fn test_at_bottom_scroll_down_partial() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add content and scroll up
        for i in 0..20 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }
        view.scroll_up(10);
        assert!(!view.state().at_bottom);

        // Scroll down partially (not all the way to bottom)
        view.scroll_down(5);
        assert_eq!(view.state().scroll_offset, 5);
        assert!(
            !view.state().at_bottom,
            "Should NOT be at_bottom when scroll_offset > 0"
        );
    }

    /// Test that new content doesn't auto-scroll when user has scrolled up.
    ///
    /// Regression test for: auto-scroll ignoring at_bottom flag.
    /// Users should be able to read old messages while new content streams.
    #[test]
    fn test_no_auto_scroll_when_scrolled_up() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add initial content
        for i in 0..20 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }

        // User scrolls up to read older messages
        view.scroll_up(10);
        assert!(!view.state().at_bottom);
        let offset_before = view.state().scroll_offset;
        assert!(offset_before > 0);

        // New assistant message arrives (simulating streaming)
        view.push_assistant_message("New content from assistant")
            .unwrap();

        // Scroll position should NOT change - user should stay where they were
        assert_eq!(
            view.state().scroll_offset,
            offset_before,
            "Scroll offset should remain unchanged when user scrolled up. \
             Got {} but expected {} (the position before new content)",
            view.state().scroll_offset,
            offset_before
        );
        assert!(
            !view.state().at_bottom,
            "at_bottom should remain false when user has scrolled up"
        );
    }

    /// Test that new content DOES auto-scroll when user is at bottom.
    #[test]
    fn test_auto_scroll_when_at_bottom() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add initial content - user starts at bottom
        view.push_user_message("First message").unwrap();
        assert!(view.state().at_bottom);
        assert_eq!(view.state().scroll_offset, 0);

        // New assistant message arrives
        view.push_assistant_message("Response").unwrap();

        // Should remain at bottom
        assert!(view.state().at_bottom);
        assert_eq!(view.state().scroll_offset, 0);
    }

    /// Test that scrolling works correctly for messages with many lines.
    ///
    /// BUG: content_height() uses items.len() * 3, which severely underestimates
    /// actual content height for messages with code blocks or multiple paragraphs.
    /// This causes scroll_up to clamp scroll_offset too early, preventing users
    /// from scrolling back to see older content.
    #[test]
    fn test_scroll_with_multiline_messages() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add 5 messages, each with 10+ lines (code block)
        // Total content should be ~50+ lines, not 15 (5 * 3)
        for i in 0..5 {
            let multiline_content = format!(
                "Message {} with code:\n```\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\n```",
                i
            );
            view.push_assistant_message(&multiline_content).unwrap();
        }

        // With viewport height 24 and actual content ~50+ lines,
        // max_scroll should be ~26+ lines
        // But buggy content_height() = 5 * 3 = 15
        // Buggy max_scroll = 15 - 24 = 0 (saturating_sub)

        // Try to scroll up significantly
        view.scroll_up(100); // Request more than possible

        // scroll_offset should be clamped to actual max_scroll (26+),
        // not the buggy estimate (0)
        // If content_height is calculated correctly, we should be able to scroll
        assert!(
            view.state().scroll_offset > 0,
            "Should be able to scroll up when content exceeds viewport. \
             scroll_offset={}, but expected > 0 (content should be ~50+ lines, viewport 24)",
            view.state().scroll_offset
        );

        // More specifically: with 5 messages each having 10+ lines = 50+ content lines,
        // and viewport of 24, max_scroll should be at least 26.
        // The buggy implementation gives max_scroll = max(0, 15-24) = 0
        assert!(
            view.state().scroll_offset >= 20,
            "scroll_offset should be at least 20 for 50+ lines of content with viewport 24. \
             Got scroll_offset={}, which suggests content_height is being underestimated.",
            view.state().scroll_offset
        );
    }

    // NOTE: Viewport rendering tests removed - the new PopupState manages viewport
    // internally and has its own renderer. See generic_popup.rs for viewport tests.

    // =============================================================================
    // Generic Popup Integration Tests
    // =============================================================================

    mod popup_tests {
        use super::*;
        use crate::tui::components::PopupState;
        use crate::tui::popup::PopupProvider;
        use crate::tui::state::{PopupItem, PopupKind};
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
        fn test_view_state_has_popup_field() {
            let state = ViewState::new("plan", 80, 24);
            // ViewState should have a popup field that's None by default
            assert!(state.popup.is_none());
        }

        #[test]
        fn test_ratatui_view_set_popup() {
            let mut view = RatatuiView::new("plan", 80, 24);

            // Create a generic popup
            let popup = PopupState::new(PopupKind::Command, mock_provider());

            // Set it on the view
            view.set_popup(Some(popup));

            // Should be set
            assert!(view.popup().is_some());
        }

        #[test]
        fn test_ratatui_view_popup_mutable_access() {
            let mut view = RatatuiView::new("plan", 80, 24);

            let popup = PopupState::new(PopupKind::Command, mock_provider());
            view.set_popup(Some(popup));

            // Should be able to get mutable access
            let popup_mut = view.popup_mut().unwrap();
            popup_mut.set_filter_query("hel");

            // Filter query should be updated
            assert_eq!(view.popup().unwrap().filter_query(), "hel");
        }

        #[test]
        fn test_ratatui_view_has_popup() {
            let mut view = RatatuiView::new("plan", 80, 24);

            // No popup set
            assert!(!view.has_popup());

            // Set popup
            let popup = PopupState::new(PopupKind::Command, mock_provider());
            view.set_popup(Some(popup));
            assert!(view.has_popup());

            // Clear popup
            view.set_popup(None);
            assert!(!view.has_popup());
        }

        #[test]
        fn test_ratatui_view_clear_popup() {
            let mut view = RatatuiView::new("plan", 80, 24);

            let popup = PopupState::new(PopupKind::Command, mock_provider());
            view.set_popup(Some(popup));
            assert!(view.popup().is_some());

            // Clear it
            view.set_popup(None);
            assert!(view.popup().is_none());
        }
    }

    // =========================================================================
    // Reasoning Panel Tests (TDD - RED PHASE)
    // =========================================================================

    #[test]
    fn test_view_state_reasoning_default() {
        let state = ViewState::new("plan", 80, 24);
        assert!(!state.show_reasoning);
        assert!(state.reasoning_content.is_empty());
    }

    #[test]
    fn test_ratatui_view_set_reasoning() {
        let mut view = RatatuiView::new("plan", 80, 24);

        view.set_reasoning("Thinking about the problem...");
        assert_eq!(view.reasoning(), "Thinking about the problem...");

        view.clear_reasoning();
        assert!(view.reasoning().is_empty());
    }

    #[test]
    fn test_ratatui_view_toggle_reasoning() {
        let mut view = RatatuiView::new("plan", 80, 24);

        assert!(!view.show_reasoning());

        view.set_show_reasoning(true);
        assert!(view.show_reasoning());

        view.set_show_reasoning(false);
        assert!(!view.show_reasoning());
    }

    #[test]
    fn test_ratatui_view_renders_reasoning_panel_when_visible() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Set up reasoning content and enable display
        view.set_reasoning("Hmm, let me think about this carefully...");
        view.set_show_reasoning(true);

        // Render to test backend
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view.render_frame(f)).unwrap();

        // Get the buffer content as string
        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // Should show thinking indicator
        assert!(
            content.contains("Thinking"),
            "Reasoning panel should be visible. Buffer: {}",
            content
        );
    }

    #[test]
    fn test_ratatui_view_hides_reasoning_panel_when_disabled() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Set up reasoning content but keep display disabled
        view.set_reasoning("Hmm, let me think about this carefully...");
        view.set_show_reasoning(false);

        // Render to test backend
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view.render_frame(f)).unwrap();

        // Get the buffer content as string
        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // Should NOT show reasoning content when disabled
        assert!(
            !content.contains("let me think"),
            "Reasoning panel should be hidden when show_reasoning=false. Buffer: {}",
            content
        );
    }
}
