//! Message rendering for scrollback
//!
//! This module provides functionality for rendering chat messages into the terminal
//! scrollback buffer using ratatui's `insert_before()` method. Each message has a
//! role (user/assistant/system) with distinct styling and is properly wrapped to fit
//! the terminal width.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// A chat message to display in scrollback
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessageDisplay {
    /// The role (user/assistant/system)
    pub role: MessageRole,
    /// The content
    pub content: String,
}

impl ChatMessageDisplay {
    /// Create a new chat message
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageRole::System, content)
    }
}

/// The role of a message sender
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    /// User-submitted message
    User,
    /// Assistant/agent response
    Assistant,
    /// System notification
    System,
}

impl MessageRole {
    /// Get the display prefix for this role
    pub fn prefix(&self) -> &'static str {
        match self {
            MessageRole::User => "You",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
        }
    }

    /// Get the style for this role
    pub fn style(&self) -> Style {
        match self {
            MessageRole::User => Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            MessageRole::Assistant => Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            MessageRole::System => Style::default().fg(Color::Yellow),
        }
    }
}

/// Calculate the height needed to render a message at a given width
///
/// This accounts for:
/// - One line for the role prefix
/// - Word-wrapped content lines
/// - One blank line separator after the message
pub fn calculate_message_height(msg: &ChatMessageDisplay, width: u16) -> u16 {
    if width == 0 {
        return 1; // Minimum height
    }

    // Prefix line (e.g., "You: ")
    let prefix_height = 1;

    // Content lines (wrapped to width)
    let wrapped = wrap_text(&msg.content, width as usize);
    let content_height = wrapped.len().max(1) as u16; // At least 1 line even if empty

    // Separator line
    let separator_height = 1;

    prefix_height + content_height + separator_height
}

/// Render a message into a buffer for insert_before
///
/// This renders the message with:
/// - A styled prefix line (role name in color)
/// - Word-wrapped content
/// - A blank separator line
pub fn render_message(buf: &mut Buffer, msg: &ChatMessageDisplay) {
    let area = buf.area;

    if area.width == 0 || area.height == 0 {
        return; // Nothing to render
    }

    let mut y = area.y;

    // Render prefix line (e.g., "You: ")
    if y < area.y + area.height {
        let prefix_line = Line::from(vec![Span::styled(
            format!("{}: ", msg.role.prefix()),
            msg.role.style(),
        )]);

        let prefix_area = Rect::new(area.x, y, area.width, 1);
        Paragraph::new(prefix_line).render(prefix_area, buf);
        y += 1;
    }

    // Render content (word-wrapped)
    let wrapped_lines = wrap_text(&msg.content, area.width as usize);
    for line in wrapped_lines {
        if y >= area.y + area.height {
            break; // No more space
        }

        let line_area = Rect::new(area.x, y, area.width, 1);
        Paragraph::new(line.as_str()).render(line_area, buf);
        y += 1;
    }

    // Render separator line (blank)
    // This is implicitly handled by the buffer being initialized with blank cells
}

/// Wrap text to fit within a given width
///
/// This performs simple word-wrapping:
/// - Words are kept together if possible
/// - Long words that exceed width are broken
/// - Empty lines are preserved
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()]; // Can't wrap to zero width
    }

    let mut result = Vec::new();

    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            result.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        let words: Vec<&str> = paragraph.split_whitespace().collect();

        for word in &words {
            // Handle words longer than width - must be broken
            if word.len() > width {
                // Flush current line if non-empty
                if !current_line.is_empty() {
                    result.push(current_line.clone());
                    current_line.clear();
                }

                // Break the long word into chunks
                let mut remaining = *word;
                while remaining.len() > width {
                    result.push(remaining[..width].to_string());
                    remaining = &remaining[width..];
                }
                if !remaining.is_empty() {
                    current_line = remaining.to_string();
                }
                continue;
            }

            // Check if adding this word would exceed width
            let space_needed = if current_line.is_empty() {
                word.len()
            } else {
                current_line.len() + 1 + word.len() // +1 for space
            };

            if space_needed > width {
                // Flush current line and start new one
                result.push(current_line.clone());
                current_line = word.to_string();
            } else {
                // Add word to current line
                if !current_line.is_empty() {
                    current_line.push(' ');
                }
                current_line.push_str(word);
            }
        }

        // Flush remaining content
        if !current_line.is_empty() {
            result.push(current_line);
        } else if words.is_empty() {
            // Preserve empty paragraph
            result.push(String::new());
        }
    }

    if result.is_empty() {
        result.push(String::new());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_message_role_prefix() {
        assert_eq!(MessageRole::User.prefix(), "You");
        assert_eq!(MessageRole::Assistant.prefix(), "Assistant");
        assert_eq!(MessageRole::System.prefix(), "System");
    }

    #[test]
    fn test_message_role_styling() {
        let user_style = MessageRole::User.style();
        assert_eq!(user_style.fg, Some(Color::Green));
        assert!(user_style.add_modifier.contains(Modifier::BOLD));

        let assistant_style = MessageRole::Assistant.style();
        assert_eq!(assistant_style.fg, Some(Color::Blue));
        assert!(assistant_style.add_modifier.contains(Modifier::BOLD));

        let system_style = MessageRole::System.style();
        assert_eq!(system_style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_chat_message_constructors() {
        let user_msg = ChatMessageDisplay::user("Hello");
        assert_eq!(user_msg.role, MessageRole::User);
        assert_eq!(user_msg.content, "Hello");

        let assistant_msg = ChatMessageDisplay::assistant("Hi there");
        assert_eq!(assistant_msg.role, MessageRole::Assistant);
        assert_eq!(assistant_msg.content, "Hi there");

        let system_msg = ChatMessageDisplay::system("Connected");
        assert_eq!(system_msg.role, MessageRole::System);
        assert_eq!(system_msg.content, "Connected");
    }

    #[test]
    fn test_wrap_text_simple() {
        // Simple case: text fits on one line
        let result = wrap_text("Hello world", 20);
        assert_eq!(result, vec!["Hello world"]);

        // Text needs wrapping
        let result = wrap_text("Hello world this is a test", 15);
        assert_eq!(result.len(), 2);
        assert!(result[0].len() <= 15);
        assert!(result[1].len() <= 15);
    }

    #[test]
    fn test_wrap_text_long_word() {
        // Word longer than width must be broken
        let result = wrap_text("supercalifragilisticexpialidocious", 10);
        assert!(result.len() >= 4); // Word is 34 chars, needs at least 4 lines

        for line in &result {
            assert!(line.len() <= 10, "Line '{}' exceeds width 10", line);
        }
    }

    #[test]
    fn test_wrap_text_preserves_paragraphs() {
        let text = "First paragraph\n\nSecond paragraph";
        let result = wrap_text(text, 50);

        // Should have at least 3 elements: first para, empty line, second para
        assert!(result.len() >= 3);
        assert_eq!(result[0], "First paragraph");
        assert_eq!(result[1], ""); // Empty line preserved
        assert_eq!(result[2], "Second paragraph");
    }

    #[test]
    fn test_wrap_text_empty_input() {
        let result = wrap_text("", 20);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_wrap_text_zero_width() {
        let result = wrap_text("Hello", 0);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_wrap_text_exact_width() {
        // Text exactly fills width
        let result = wrap_text("Hello", 5);
        assert_eq!(result, vec!["Hello"]);

        // Two words that exactly fill width with space
        let result = wrap_text("Hi there", 8);
        assert_eq!(result, vec!["Hi there"]);
    }

    #[test]
    fn test_message_height_calculation() {
        // Short message
        let msg = ChatMessageDisplay::user("Hello");
        let height = calculate_message_height(&msg, 80);
        // 1 prefix + 1 content + 1 separator = 3
        assert_eq!(height, 3);

        // Message that wraps to 2 lines at width 10
        let msg = ChatMessageDisplay::user("Hello world this is a test");
        let height = calculate_message_height(&msg, 10);
        assert!(height >= 3); // At least prefix + 1 content + separator
    }

    #[test]
    fn test_message_height_calculation_multiline() {
        let msg = ChatMessageDisplay::user("Line 1\nLine 2\nLine 3");
        let height = calculate_message_height(&msg, 80);
        // 1 prefix + 3 content + 1 separator = 5
        assert_eq!(height, 5);
    }

    #[test]
    fn test_message_height_zero_width() {
        let msg = ChatMessageDisplay::user("Hello");
        let height = calculate_message_height(&msg, 0);
        assert_eq!(height, 1); // Minimum height
    }

    #[test]
    fn test_render_message_user() {
        let msg = ChatMessageDisplay::user("Test message");
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                let buf = f.buffer_mut();

                // Create a temporary buffer for the message
                let msg_height = calculate_message_height(&msg, area.width);
                let msg_area = Rect::new(0, 0, area.width, msg_height);
                let mut temp_buf = Buffer::empty(msg_area);

                render_message(&mut temp_buf, &msg);

                // Copy to main buffer
                for y in 0..temp_buf.area.height {
                    for x in 0..temp_buf.area.width {
                        let cell = temp_buf.cell((x, y)).unwrap();
                        if let Some(target) = buf.cell_mut((x, y)) {
                            *target = cell.clone();
                        }
                    }
                }
            })
            .unwrap();

        // Verify the buffer contains "You: " prefix
        let buffer = terminal.backend().buffer();
        let first_line: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, 0)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();

        assert!(
            first_line.starts_with("You: "),
            "Expected 'You: ' prefix, got: '{}'",
            first_line.trim()
        );
    }

    #[test]
    fn test_render_message_assistant() {
        let msg = ChatMessageDisplay::assistant("Hello there");
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                let buf = f.buffer_mut();

                let msg_height = calculate_message_height(&msg, area.width);
                let msg_area = Rect::new(0, 0, area.width, msg_height);
                let mut temp_buf = Buffer::empty(msg_area);

                render_message(&mut temp_buf, &msg);

                for y in 0..temp_buf.area.height {
                    for x in 0..temp_buf.area.width {
                        let cell = temp_buf.cell((x, y)).unwrap();
                        if let Some(target) = buf.cell_mut((x, y)) {
                            *target = cell.clone();
                        }
                    }
                }
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let first_line: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, 0)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();

        assert!(
            first_line.starts_with("Assistant: "),
            "Expected 'Assistant: ' prefix"
        );
    }

    #[test]
    fn test_render_message_wrapping() {
        // Message that should wrap
        let msg =
            ChatMessageDisplay::user("This is a long message that should wrap to multiple lines");
        let backend = TestBackend::new(20, 10); // Narrow width
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                let buf = f.buffer_mut();

                let msg_height = calculate_message_height(&msg, area.width);
                let msg_area = Rect::new(0, 0, area.width, msg_height);
                let mut temp_buf = Buffer::empty(msg_area);

                render_message(&mut temp_buf, &msg);

                for y in 0..temp_buf.area.height.min(area.height) {
                    for x in 0..temp_buf.area.width.min(area.width) {
                        let cell = temp_buf.cell((x, y)).unwrap();
                        if let Some(target) = buf.cell_mut((x, y)) {
                            *target = cell.clone();
                        }
                    }
                }
            })
            .unwrap();

        // Verify multiple lines were rendered
        let buffer = terminal.backend().buffer();

        // Check prefix line
        let line0: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, 0)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();
        assert!(line0.starts_with("You: "));

        // Check that content continues on subsequent lines
        let line1: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, 1)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();
        assert!(!line1.trim().is_empty(), "Second line should have content");
    }

    #[test]
    fn test_render_message_empty_area() {
        let msg = ChatMessageDisplay::user("Test");
        let mut buf = Buffer::empty(Rect::new(0, 0, 0, 0));

        // Should not panic
        render_message(&mut buf, &msg);
    }

    #[test]
    fn test_wrap_text_word_boundary() {
        // Verify wrapping happens at word boundaries when possible
        let result = wrap_text("one two three four", 10);

        // Each line should contain complete words
        for line in &result {
            if !line.is_empty() {
                // No partial words - each token should be complete
                for word in line.split_whitespace() {
                    assert!(["one", "two", "three", "four"].contains(&word));
                }
            }
        }
    }

    #[test]
    fn test_message_height_empty_content() {
        let msg = ChatMessageDisplay::user("");
        let height = calculate_message_height(&msg, 80);
        // 1 prefix + 1 content (even if empty) + 1 separator = 3
        assert_eq!(height, 3);
    }
}
