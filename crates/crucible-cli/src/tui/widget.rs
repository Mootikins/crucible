//! Widget renderer for terminal chat interface

//!
//! Renders a bottom-anchored widget with:
//! - Status line (bottom)
//! - Lower separator
//! - Input area (grows upward)
//! - Upper separator
//! - Streaming area (caps at ~1/3 terminal height)

// ============================================================================
// Dynamic Mode Support (Phase 6)
// ============================================================================

/// Get the icon for a mode by its ID
///
/// Known modes (plan, act, auto) have specific icons.
/// Unknown modes get a fallback bullet icon.
pub fn mode_icon(mode_id: &str) -> &'static str {
    match mode_id {
        "plan" => "📖",
        "act" => "✏️",
        "auto" => "⚡",
        _ => "●", // Fallback for unknown modes
    }
}

/// Get the ANSI color code for a mode by its ID
///
/// Known modes (plan, act, auto) have specific colors.
/// Unknown modes get the reset code (default terminal color).
pub fn mode_color(mode_id: &str) -> &'static str {
    match mode_id {
        "plan" => ansi::CYAN,
        "act" => ansi::YELLOW,
        "auto" => ansi::RED,
        _ => ansi::RESET, // Fallback for unknown modes
    }
}

use std::io::{self, Write};

/// Component heights for the widget layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetHeights {
    /// Status line height (always 1)
    pub status: u16,
    /// Lower separator height (always 1)
    pub lower_separator: u16,
    /// Input area height (1 + extra lines)
    pub input: u16,
    /// Upper separator height (always 1)
    pub upper_separator: u16,
    /// Streaming area height (0 to streaming_cap)
    pub streaming: u16,
    /// Maximum streaming area height (~1/3 terminal)
    pub streaming_cap: u16,
}

impl WidgetHeights {
    /// Minimum widget height (status + 2 separators + 1 input line)
    pub const MIN_HEIGHT: u16 = 4;

    /// Calculate total widget height
    pub fn total(&self) -> u16 {
        self.status + self.lower_separator + self.input + self.upper_separator + self.streaming
    }
}

/// Calculate widget heights based on terminal size and content
///
/// # Arguments
/// * `terminal_height` - Total terminal height in rows
/// * `input_lines` - Number of lines in input buffer (minimum 1)
/// * `streaming_lines` - Number of lines in streaming content (0 when idle)
///
/// # Returns
/// * `WidgetHeights` with calculated values for each component
pub fn calculate_heights(
    terminal_height: u16,
    input_lines: u16,
    streaming_lines: u16,
) -> WidgetHeights {
    // Fixed components
    let status: u16 = 1;
    let lower_separator: u16 = 1;
    let upper_separator: u16 = 1;

    // Input area: at least 1 line
    let input = input_lines.max(1);

    // Streaming cap: ~1/3 of terminal height
    let streaming_cap = terminal_height / 3;

    // Streaming area: 0 to cap
    let streaming = streaming_lines.min(streaming_cap);

    WidgetHeights {
        status,
        lower_separator,
        input,
        upper_separator,
        streaming,
        streaming_cap,
    }
}

/// Position info for widget rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetPosition {
    /// Row where widget starts (0-indexed from top)
    pub start_row: u16,
    /// Total widget height
    pub height: u16,
}

/// Calculate the starting row for the widget
///
/// The widget is anchored to the bottom of the terminal.
///
/// # Arguments
/// * `terminal_height` - Total terminal height in rows
/// * `widget_height` - Total widget height from WidgetHeights::total()
///
/// # Returns
/// * `WidgetPosition` with start_row and height
pub fn calculate_position(terminal_height: u16, widget_height: u16) -> WidgetPosition {
    // Widget anchored to bottom - start_row = terminal_height - widget_height
    // If widget_height > terminal_height, clamp to 0
    let start_row = terminal_height.saturating_sub(widget_height);
    WidgetPosition {
        start_row,
        height: widget_height,
    }
}

/// Move cursor to widget start position and clear widget region
///
/// Uses ANSI escape codes to position cursor and clear lines.
///
/// # Arguments
/// * `writer` - Output writer (typically stdout)
/// * `position` - Widget position from calculate_position
pub fn move_to_widget<W: Write>(writer: &mut W, position: &WidgetPosition) -> io::Result<()> {
    // Move cursor to start row (1-indexed for ANSI)
    // \x1b[{row};{col}H - moves cursor to row, col
    let ansi_row = position.start_row + 1; // ANSI is 1-indexed
    write!(writer, "\x1b[{};1H", ansi_row)?;

    // Clear from cursor to end of screen
    // \x1b[J - clear from cursor to end of screen
    write!(writer, "\x1b[J")?;

    writer.flush()
}

/// ANSI escape codes for styling
pub mod ansi {
    /// Dim text (faint)
    pub const DIM: &str = "\x1b[2m";
    /// Reset all attributes
    pub const RESET: &str = "\x1b[0m";
    /// Cyan foreground
    pub const CYAN: &str = "\x1b[36m";
    /// Yellow foreground
    pub const YELLOW: &str = "\x1b[33m";
    /// Red foreground
    pub const RED: &str = "\x1b[31m";
    /// Green foreground
    pub const GREEN: &str = "\x1b[32m";
    /// Move cursor to column 1
    pub const COL1: &str = "\x1b[1G";
    /// Clear line from cursor to end
    pub const CLEAR_LINE: &str = "\x1b[K";
}

/// Render the status line with dynamic mode (mode_id + mode_name)
///
/// Shows mode indicator with appropriate color styling from mode registry.
/// Uses dim styling for unobtrusive appearance.
///
/// # Arguments
/// * `writer` - Output writer
/// * `mode_id` - Mode identifier for icon/color lookup
/// * `mode_name` - Human-readable mode name for display
/// * `width` - Terminal width for padding
pub fn render_status_line_dynamic<W: Write>(
    writer: &mut W,
    mode_id: &str,
    mode_name: &str,
    _width: u16,
) -> io::Result<()> {
    let icon = mode_icon(mode_id);
    let color = mode_color(mode_id);

    // Format: [icon Mode] │ Ready
    // No trailing newline - this is the bottom line
    write!(
        writer,
        "{}{}[{} {}]{} │ Ready{}",
        ansi::DIM,
        color,
        icon,
        mode_name,
        ansi::RESET,
        ansi::CLEAR_LINE,
    )
}

/// Widget state for rendering with dynamic mode support
pub struct WidgetStateDynamic<'a> {
    /// Mode identifier (e.g., "plan", "act", "agent-custom")
    pub mode_id: &'a str,
    /// Human-readable mode name (e.g., "Plan", "Custom Mode")
    pub mode_name: &'a str,
    /// Input buffer content
    pub input: &'a str,
    /// Cursor position in input
    pub cursor_col: usize,
    /// Streaming content (empty when idle)
    pub streaming: &'a str,
    /// Terminal width
    pub width: u16,
    /// Terminal height
    pub height: u16,
}

/// Render the complete widget with dynamic mode support
///
/// Similar to render_widget but uses mode_id and mode_name with string-based mode IDs.
///
/// # Arguments
/// * `writer` - Output writer
/// * `state` - Widget state with dynamic mode info
pub fn render_widget_dynamic<W: Write>(
    writer: &mut W,
    state: &WidgetStateDynamic,
) -> io::Result<()> {
    // Calculate heights
    let input_lines = if state.input.is_empty() {
        1
    } else {
        state.input.lines().count().max(1) as u16
    };
    let streaming_lines = if state.streaming.is_empty() {
        0
    } else {
        state.streaming.lines().count() as u16
    };
    let heights = calculate_heights(state.height, input_lines, streaming_lines);
    let position = calculate_position(state.height, heights.total());

    // Move to widget position
    move_to_widget(writer, &position)?;

    // Render from top to bottom (streaming first if present)
    if heights.streaming > 0 {
        render_streaming_area(writer, state.streaming, heights.streaming)?;
    }

    // Upper separator (always show - provides spacing above prompt)
    render_separator(writer, state.width)?;

    // Input area - render_input_area uses mode_id string directly
    render_input_area(writer, "plan", state.input, state.cursor_col)?;

    // Lower separator
    render_separator(writer, state.width)?;

    // Status line with dynamic mode
    render_status_line_dynamic(writer, state.mode_id, state.mode_name, state.width)?;

    // Position cursor in input area
    let input_row = position.start_row + heights.streaming + heights.upper_separator + 1;
    let cursor_col = state.cursor_col as u16 + 3;
    write!(writer, "\x1b[{};{}H", input_row, cursor_col)?;

    // Show cursor
    write!(writer, "\x1b[?25h")?;

    writer.flush()
}

/// Render the status line
///
/// Shows mode indicator with appropriate color styling.
/// Uses dim styling for unobtrusive appearance.
///
/// # Arguments
/// * `writer` - Output writer
/// * `mode` - Current chat mode
/// * `width` - Terminal width for padding
pub fn render_status_line<W: Write>(writer: &mut W, mode_id: &str, _width: u16) -> io::Result<()> {
    let (mode_str, mode_color) = match mode_id {
        "plan" => ("Plan", ansi::CYAN),
        "act" => ("Act", ansi::YELLOW),
        "auto" => ("Auto", ansi::RED),
        _ => ("Unknown", ansi::RESET),
    };

    // Format: [Mode] │ Ready
    // No trailing newline - this is the bottom line
    write!(
        writer,
        "{}{}[{}]{} │ Ready{}",
        ansi::DIM,
        mode_color,
        mode_str,
        ansi::RESET,
        ansi::CLEAR_LINE,
    )
}

/// Render a dim horizontal separator
///
/// Uses box-drawing character (─) for visual separation.
///
/// # Arguments
/// * `writer` - Output writer
/// * `width` - Terminal width
pub fn render_separator<W: Write>(writer: &mut W, width: u16) -> io::Result<()> {
    write!(writer, "{}", ansi::DIM)?;
    for _ in 0..width {
        write!(writer, "─")?;
    }
    write!(writer, "{}\r\n", ansi::RESET)
}

/// Render the input area
///
/// Shows simple prompt with input buffer.
///
/// # Arguments
/// * `writer` - Output writer
/// * `_mode` - Current chat mode (unused, mode shown in status line)
/// * `input` - Input buffer content
/// * `_cursor_col` - Cursor column within input (for cursor positioning)
pub fn render_input_area<W: Write>(
    writer: &mut W,
    _mode_id: &str,
    input: &str,
    _cursor_col: usize,
) -> io::Result<()> {
    // Render each line of input
    let lines: Vec<&str> = if input.is_empty() {
        vec![""]
    } else {
        input.lines().collect()
    };

    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            // First line has simple prompt
            write!(writer, "> {}{}\r\n", line, ansi::CLEAR_LINE)?;
        } else {
            // Continuation lines are indented
            write!(writer, "  {}{}\r\n", line, ansi::CLEAR_LINE)?;
        }
    }

    Ok(())
}

/// Render the streaming area
///
/// Shows streaming content with visual indicator.
/// Handles overflow by showing only the last N lines.
///
/// # Arguments
/// * `writer` - Output writer
/// * `content` - Streaming content (may be empty)
/// * `max_lines` - Maximum lines to display
pub fn render_streaming_area<W: Write>(
    writer: &mut W,
    content: &str,
    max_lines: u16,
) -> io::Result<()> {
    if content.is_empty() || max_lines == 0 {
        return Ok(());
    }

    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();

    // Show last N lines if content exceeds max
    let start_idx = line_count.saturating_sub(max_lines as usize);

    // Add streaming indicator on first line
    for (i, line) in lines.iter().skip(start_idx).enumerate() {
        if i == 0 && start_idx > 0 {
            // Indicate truncation
            write!(
                writer,
                "{}... ({} lines hidden){}\r\n",
                ansi::DIM,
                start_idx,
                ansi::RESET
            )?;
        }
        write!(
            writer,
            "{}{}{}{}",
            ansi::GREEN,
            line,
            ansi::RESET,
            ansi::CLEAR_LINE
        )?;
        write!(writer, "\r\n")?;
    }

    Ok(())
}

/// Widget state for rendering
pub struct WidgetState<'a> {
    /// Current chat mode
    pub mode_id: &'a str,
    /// Input buffer content
    pub input: &'a str,
    /// Cursor position in input
    pub cursor_col: usize,
    /// Streaming content (empty when idle)
    pub streaming: &'a str,
    /// Terminal width
    pub width: u16,
    /// Terminal height
    pub height: u16,
}

/// Render the complete widget
///
/// Composes all widget components from bottom to top:
/// 1. Status line (bottom)
/// 2. Lower separator
/// 3. Input area
/// 4. Upper separator
/// 5. Streaming area (top, if active)
///
/// # Arguments
/// * `writer` - Output writer
/// * `state` - Widget state
pub fn render_widget<W: Write>(writer: &mut W, state: &WidgetState) -> io::Result<()> {
    // Calculate heights
    let input_lines = if state.input.is_empty() {
        1
    } else {
        state.input.lines().count().max(1) as u16
    };
    let streaming_lines = if state.streaming.is_empty() {
        0
    } else {
        state.streaming.lines().count() as u16
    };
    let heights = calculate_heights(state.height, input_lines, streaming_lines);
    let position = calculate_position(state.height, heights.total());

    // Move to widget position
    move_to_widget(writer, &position)?;

    // Render from top to bottom (streaming first if present)
    if heights.streaming > 0 {
        render_streaming_area(writer, state.streaming, heights.streaming)?;
    }

    // Upper separator (always show - provides spacing above prompt)
    render_separator(writer, state.width)?;

    // Input area
    render_input_area(writer, state.mode_id, state.input, state.cursor_col)?;

    // Lower separator
    render_separator(writer, state.width)?;

    // Status line
    render_status_line(writer, state.mode_id, state.width)?;

    // Position cursor in input area
    // Input is at: start_row + streaming + upper_separator
    let input_row = position.start_row + heights.streaming + heights.upper_separator + 1; // +1 for ANSI 1-indexing
    let cursor_col = state.cursor_col as u16 + 3; // +2 for "> " prefix, +1 for ANSI 1-indexing
    write!(writer, "\x1b[{};{}H", input_row, cursor_col)?;

    // Show cursor
    write!(writer, "\x1b[?25h")?;

    writer.flush()
}

// ============================================================================
// Help Rendering (Phase 6)
// ============================================================================

use crucible_core::traits::chat::{CommandDescriptor, CommandOption};

/// Format a command descriptor for display in help output
///
/// Shows the command name and description, with optional input hint.
/// Handles namespaced commands (e.g., "crucible:search") by displaying them properly.
pub fn format_help_command(desc: &CommandDescriptor) -> String {
    let hint = desc
        .input_hint
        .as_ref()
        .map(|h| format!(" <{}>", h))
        .unwrap_or_default();
    let options_suffix = if desc.secondary_options.is_empty() {
        String::new()
    } else {
        let labels: Vec<_> = desc
            .secondary_options
            .iter()
            .map(|o| o.label.as_str())
            .collect();
        format!(" [options: {}]", labels.join(", "))
    };

    format!(
        "  /{}{} - {}{}",
        desc.name, hint, desc.description, options_suffix
    )
}

/// Render help text for a list of command descriptors
///
/// Groups commands by source:
/// - Client commands (including namespaced ones like crucible:search)
/// - Agent commands
///
/// Returns formatted help text ready for display.
pub fn render_help_text(commands: &[CommandDescriptor], agent_name: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str("Available Commands:\n");
    output.push_str("==================\n\n");

    // Separate namespaced and regular commands
    let (namespaced, regular): (Vec<_>, Vec<_>) =
        commands.iter().partition(|c| c.name.contains(':'));

    // Client commands
    output.push_str("Client Commands:\n");
    for desc in regular.iter().filter(|c| !c.name.contains(':')) {
        output.push_str(&format_help_command(desc));
        output.push('\n');
    }

    // Namespaced client commands (if any)
    if !namespaced.is_empty() {
        output.push_str("\nNamespaced Commands (use crucible: prefix):\n");
        for desc in &namespaced {
            output.push_str(&format_help_command(desc));
            output.push('\n');
        }
    }

    // Agent commands section
    if let Some(name) = agent_name {
        output.push_str(&format!("\nAgent Commands ({}):\n", name));
        output.push_str("  (Commands provided by the connected agent)\n");
    }

    output.push_str("\nPress Ctrl+C to cancel, Ctrl+D or /exit to quit\n");
    output
}
#[cfg(test)]
mod tests {
    use super::*;

    // 6.1.1: Test dynamic mode rendering
    #[test]
    fn test_mode_icon_known_modes() {
        assert_eq!(mode_icon("plan"), "📖");
        assert_eq!(mode_icon("act"), "✏️");
        assert_eq!(mode_icon("auto"), "⚡");
    }

    #[test]
    fn test_mode_icon_unknown_mode_fallback() {
        // Unknown modes should get fallback icon
        assert_eq!(mode_icon("custom-agent-mode"), "●");
        assert_eq!(mode_icon(""), "●");
        assert_eq!(mode_icon("something-else"), "●");
    }

    #[test]
    fn test_mode_color_known_modes() {
        assert_eq!(mode_color("plan"), ansi::CYAN);
        assert_eq!(mode_color("act"), ansi::YELLOW);
        assert_eq!(mode_color("auto"), ansi::RED);
    }

    #[test]
    fn test_mode_color_unknown_mode_fallback() {
        // Unknown modes should get fallback color (white/reset)
        assert_eq!(mode_color("custom-agent-mode"), ansi::RESET);
        assert_eq!(mode_color(""), ansi::RESET);
    }

    #[test]
    fn test_render_status_line_with_mode_id() {
        let mut buffer = Vec::new();
        render_status_line_dynamic(&mut buffer, "plan", "Plan", 80).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("📖"), "Should contain plan icon");
        assert!(output.contains("Plan"), "Should contain mode name");
        assert!(output.contains(ansi::CYAN), "Plan mode should use cyan");
    }

    #[test]
    fn test_render_status_line_with_unknown_mode_id() {
        let mut buffer = Vec::new();
        render_status_line_dynamic(&mut buffer, "agent-custom", "Custom Mode", 80).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("●"), "Should contain fallback icon");
        assert!(output.contains("Custom Mode"), "Should contain mode name");
    }

    #[test]
    fn test_widget_state_dynamic_uses_mode_id() {
        let mut buffer = Vec::new();
        let state = WidgetStateDynamic {
            mode_id: "act",
            mode_name: "Act",
            input: "",
            cursor_col: 0,
            streaming: "",
            width: 80,
            height: 24,
        };

        render_widget_dynamic(&mut buffer, &state).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("✏️"), "Should contain act icon");
        assert!(output.contains("Act"), "Should contain mode name");
        assert!(output.contains(ansi::YELLOW), "Act mode should use yellow");
    }

    // 2.1.1: Test widget height calculation
    #[test]
    fn test_min_widget_height() {
        // Min height = 4 (status + 2 separators + 1 input line)
        let heights = calculate_heights(24, 1, 0);
        assert_eq!(heights.total(), WidgetHeights::MIN_HEIGHT);
        assert_eq!(heights.status, 1);
        assert_eq!(heights.lower_separator, 1);
        assert_eq!(heights.input, 1);
        assert_eq!(heights.upper_separator, 1);
        assert_eq!(heights.streaming, 0);
    }

    #[test]
    fn test_streaming_cap_one_third() {
        // Streaming cap = terminal_height / 3
        let heights = calculate_heights(24, 1, 0);
        assert_eq!(heights.streaming_cap, 8); // 24 / 3 = 8

        let heights = calculate_heights(30, 1, 0);
        assert_eq!(heights.streaming_cap, 10); // 30 / 3 = 10

        let heights = calculate_heights(100, 1, 0);
        assert_eq!(heights.streaming_cap, 33); // 100 / 3 = 33
    }

    #[test]
    fn test_streaming_respects_cap() {
        let heights = calculate_heights(24, 1, 20);
        // streaming_cap = 8, so streaming should be capped at 8
        assert_eq!(heights.streaming, 8);
        assert_eq!(heights.streaming_cap, 8);
    }

    #[test]
    fn test_streaming_under_cap() {
        let heights = calculate_heights(24, 1, 5);
        // streaming_lines (5) < streaming_cap (8)
        assert_eq!(heights.streaming, 5);
    }

    #[test]
    fn test_input_grows_with_content() {
        let heights = calculate_heights(24, 3, 0);
        assert_eq!(heights.input, 3);
        assert_eq!(heights.total(), 6); // status(1) + sep(1) + input(3) + sep(1) + stream(0)
    }

    #[test]
    fn test_input_minimum_one_line() {
        let heights = calculate_heights(24, 0, 0);
        assert_eq!(heights.input, 1); // Minimum 1 line
    }

    #[test]
    fn test_full_widget_height() {
        // All components active
        let heights = calculate_heights(30, 3, 5);
        // status(1) + lower_sep(1) + input(3) + upper_sep(1) + streaming(5)
        assert_eq!(heights.total(), 11);
    }

    // 2.2.1: Test cursor positioning
    #[test]
    fn test_widget_position_bottom_anchored() {
        // Widget at bottom of 24-line terminal
        let pos = calculate_position(24, 4);
        assert_eq!(pos.start_row, 20); // 24 - 4 = 20
        assert_eq!(pos.height, 4);
    }

    #[test]
    fn test_widget_position_with_streaming() {
        // Widget with streaming area
        let pos = calculate_position(24, 10);
        assert_eq!(pos.start_row, 14); // 24 - 10 = 14
    }

    #[test]
    fn test_widget_position_larger_than_terminal() {
        // Widget taller than terminal (edge case)
        let pos = calculate_position(10, 15);
        assert_eq!(pos.start_row, 0); // Clamp to 0
        assert_eq!(pos.height, 15);
    }

    #[test]
    fn test_widget_position_equal_to_terminal() {
        // Widget exactly fills terminal
        let pos = calculate_position(24, 24);
        assert_eq!(pos.start_row, 0);
    }

    // 2.2.2: Test move_to_widget cursor positioning
    #[test]
    fn test_move_to_widget_output() {
        let mut buffer = Vec::new();
        let pos = WidgetPosition {
            start_row: 20,
            height: 4,
        };

        move_to_widget(&mut buffer, &pos).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        // ANSI row is 1-indexed, so row 20 becomes 21
        assert!(
            output.contains("\x1b[21;1H"),
            "Should position cursor at row 21"
        );
        assert!(output.contains("\x1b[J"), "Should clear to end of screen");
    }

    #[test]
    fn test_move_to_widget_row_zero() {
        let mut buffer = Vec::new();
        let pos = WidgetPosition {
            start_row: 0,
            height: 24,
        };

        move_to_widget(&mut buffer, &pos).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        // Row 0 becomes ANSI row 1
        assert!(output.contains("\x1b[1;1H"));
    }

    // 2.3.1: Test status line render
    #[test]
    fn test_status_line_plan_mode() {
        let mut buffer = Vec::new();
        render_status_line(&mut buffer, "plan", 80).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(
            output.contains("[Plan]"),
            "Should contain Plan mode indicator"
        );
        assert!(output.contains("Ready"), "Should contain Ready status");
        assert!(output.contains(ansi::CYAN), "Plan mode should use cyan");
        assert!(output.contains(ansi::DIM), "Should use dim styling");
    }

    #[test]
    fn test_status_line_act_mode() {
        let mut buffer = Vec::new();
        render_status_line(&mut buffer, "act", 80).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(
            output.contains("[Act]"),
            "Should contain Act mode indicator"
        );
        assert!(output.contains(ansi::YELLOW), "Act mode should use yellow");
    }

    #[test]
    fn test_status_line_auto_mode() {
        let mut buffer = Vec::new();
        render_status_line(&mut buffer, "auto", 80).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(
            output.contains("[Auto]"),
            "Should contain Auto mode indicator"
        );
        assert!(output.contains(ansi::RED), "Auto mode should use red");
    }

    // 2.3.2: Test separator render
    #[test]
    fn test_separator_contains_box_drawing() {
        let mut buffer = Vec::new();
        render_separator(&mut buffer, 10).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        // Should contain box-drawing horizontal line
        assert!(output.contains("─"), "Should contain box-drawing character");
        assert!(output.contains(ansi::DIM), "Should use dim styling");
        assert!(output.contains(ansi::RESET), "Should reset styling");
    }

    #[test]
    fn test_separator_respects_width() {
        let mut buffer = Vec::new();
        render_separator(&mut buffer, 20).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        // Count box-drawing characters
        let char_count = output.chars().filter(|c| *c == '─').count();
        assert_eq!(char_count, 20, "Should have exactly width characters");
    }

    #[test]
    fn test_separator_zero_width() {
        let mut buffer = Vec::new();
        render_separator(&mut buffer, 0).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        let char_count = output.chars().filter(|c| *c == '─').count();
        assert_eq!(char_count, 0, "Zero width should have no separator chars");
    }

    // 2.3.3: Test input area render
    #[test]
    fn test_input_area_empty() {
        let mut buffer = Vec::new();
        render_input_area(&mut buffer, "plan", "", 0).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("> "), "Should show prompt");
    }

    #[test]
    fn test_input_area_with_content() {
        let mut buffer = Vec::new();
        render_input_area(&mut buffer, "act", "Hello", 5).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("> "), "Should show prompt");
        assert!(output.contains("Hello"), "Should show input content");
    }

    #[test]
    fn test_input_area_multiline() {
        let mut buffer = Vec::new();
        render_input_area(&mut buffer, "plan", "Line 1\nLine 2", 6).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("Line 1"), "Should show first line");
        assert!(output.contains("Line 2"), "Should show second line");
        // Second line should be indented (no mode indicator)
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() >= 2, "Should have at least 2 lines");
    }

    #[test]
    fn test_input_area_simple_prompt() {
        // All modes now use simple "> " prompt (mode shown in status line)
        for mode in ["plan", "act", "auto"] {
            let mut buffer = Vec::new();
            render_input_area(&mut buffer, mode, "", 0).unwrap();
            let output = String::from_utf8(buffer).unwrap();
            assert!(output.contains("> "), "Should show simple prompt");
        }
    }

    // 2.3.4: Test streaming area render
    #[test]
    fn test_streaming_area_empty() {
        let mut buffer = Vec::new();
        render_streaming_area(&mut buffer, "", 10).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.is_empty(), "Empty content should produce no output");
    }

    #[test]
    fn test_streaming_area_with_content() {
        let mut buffer = Vec::new();
        render_streaming_area(&mut buffer, "Streaming text", 10).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("Streaming text"), "Should contain content");
        assert!(
            output.contains(ansi::GREEN),
            "Should use green for streaming"
        );
    }

    #[test]
    fn test_streaming_area_multiline() {
        let mut buffer = Vec::new();
        render_streaming_area(&mut buffer, "Line 1\nLine 2\nLine 3", 10).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 2"));
        assert!(output.contains("Line 3"));
    }

    #[test]
    fn test_streaming_area_overflow() {
        let mut buffer = Vec::new();
        let content = "L1\nL2\nL3\nL4\nL5\nL6\nL7\nL8\nL9\nL10";
        render_streaming_area(&mut buffer, content, 5).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        // Should show truncation indicator and last 5 lines
        assert!(
            output.contains("lines hidden"),
            "Should indicate hidden lines"
        );
        assert!(output.contains("L10"), "Should show last line");
        // First lines should be hidden
        assert!(!output.contains("L1\n"), "First line should be hidden");
    }

    #[test]
    fn test_streaming_area_zero_max_lines() {
        let mut buffer = Vec::new();
        render_streaming_area(&mut buffer, "Content", 0).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.is_empty(), "Zero max lines should produce no output");
    }

    // 2.3.5: Test render_widget composition
    #[test]
    fn test_render_widget_minimal() {
        let mut buffer = Vec::new();
        let state = WidgetState {
            mode_id: "plan",
            input: "",
            cursor_col: 0,
            streaming: "",
            width: 80,
            height: 24,
        };

        render_widget(&mut buffer, &state).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        // Should contain cursor positioning
        assert!(output.contains("\x1b["), "Should have ANSI escape codes");
        // Should contain status line
        assert!(output.contains("[Plan]"), "Should render status");
        // Should contain separator
        assert!(output.contains("─"), "Should render separator");
        // Should contain input area with simple prompt
        assert!(output.contains("> "), "Should render input prompt");
    }

    #[test]
    fn test_render_widget_with_streaming() {
        let mut buffer = Vec::new();
        let state = WidgetState {
            mode_id: "act",
            input: "My input",
            cursor_col: 8,
            streaming: "Response content",
            width: 80,
            height: 24,
        };

        render_widget(&mut buffer, &state).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(
            output.contains("Response content"),
            "Should render streaming"
        );
        assert!(output.contains("My input"), "Should render input");
        assert!(output.contains("[Act]"), "Should render mode");
    }

    #[test]
    fn test_render_widget_multiline_input() {
        let mut buffer = Vec::new();
        let state = WidgetState {
            mode_id: "plan",
            input: "Line 1\nLine 2\nLine 3",
            cursor_col: 0,
            streaming: "",
            width: 80,
            height: 24,
        };

        render_widget(&mut buffer, &state).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 2"));
        assert!(output.contains("Line 3"));
    }

    // Edge case tests
    #[test]
    fn test_widget_with_unicode_input() {
        let mut buffer = Vec::new();
        let state = WidgetState {
            mode_id: "plan",
            input: "こんにちは 🎉",
            cursor_col: 0,
            streaming: "",
            width: 80,
            height: 24,
        };

        render_widget(&mut buffer, &state).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("こんにちは"));
        assert!(output.contains("🎉"));
    }

    #[test]
    fn test_streaming_with_ansi_content() {
        // Streaming content might contain ANSI from agent responses
        let mut buffer = Vec::new();
        let content = "Normal text \x1b[1mbold text\x1b[0m";
        render_streaming_area(&mut buffer, content, 10).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        // Should preserve the content (including any embedded ANSI)
        assert!(output.contains("Normal text"));
        assert!(output.contains("bold text"));
    }

    // ========================================================================
    // 6.4.1: Help Rendering Tests
    // ========================================================================

    #[test]
    fn test_format_help_command_simple() {
        let desc = CommandDescriptor {
            name: "exit".to_string(),
            description: "Exit the session".to_string(),
            input_hint: None,
            secondary_options: Vec::new(),
        };
        let output = format_help_command(&desc);
        assert!(output.contains("/exit"));
        assert!(output.contains("Exit the session"));
    }

    #[test]
    fn test_format_help_command_with_hint() {
        let desc = CommandDescriptor {
            name: "search".to_string(),
            description: "Search knowledge base".to_string(),
            input_hint: Some("query".to_string()),
            secondary_options: Vec::new(),
        };
        let output = format_help_command(&desc);
        assert!(output.contains("/search"));
        assert!(output.contains("<query>"));
        assert!(output.contains("Search knowledge base"));
    }

    #[test]
    fn test_format_help_command_with_secondary_options() {
        let desc = CommandDescriptor {
            name: "models".to_string(),
            description: "Select a model".to_string(),
            input_hint: None,
            secondary_options: vec![
                CommandOption {
                    label: "claude-3.5-sonnet".to_string(),
                    value: "claude-3.5-sonnet".to_string(),
                },
                CommandOption {
                    label: "claude-3-opus".to_string(),
                    value: "claude-3-opus".to_string(),
                },
            ],
        };
        let output = format_help_command(&desc);
        assert!(output.contains("options: claude-3.5-sonnet, claude-3-opus"));
    }

    #[test]
    fn test_format_help_command_namespaced() {
        let desc = CommandDescriptor {
            name: "crucible:search".to_string(),
            description: "Client search".to_string(),
            input_hint: Some("query".to_string()),
            secondary_options: Vec::new(),
        };
        let output = format_help_command(&desc);
        assert!(output.contains("/crucible:search"));
        assert!(output.contains("<query>"));
    }

    #[test]
    fn test_render_help_text_basic() {
        let commands = vec![
            CommandDescriptor {
                name: "exit".to_string(),
                description: "Exit".to_string(),
                input_hint: None,
                secondary_options: Vec::new(),
            },
            CommandDescriptor {
                name: "help".to_string(),
                description: "Show help".to_string(),
                input_hint: None,
                secondary_options: Vec::new(),
            },
        ];
        let output = render_help_text(&commands, None);
        assert!(output.contains("Available Commands"));
        assert!(output.contains("/exit"));
        assert!(output.contains("/help"));
        assert!(output.contains("Client Commands"));
    }

    #[test]
    fn test_render_help_text_with_namespaced() {
        let commands = vec![
            CommandDescriptor {
                name: "search".to_string(),
                description: "Agent search".to_string(),
                input_hint: None,
                secondary_options: Vec::new(),
            },
            CommandDescriptor {
                name: "crucible:search".to_string(),
                description: "Client search".to_string(),
                input_hint: Some("query".to_string()),
                secondary_options: Vec::new(),
            },
        ];
        let output = render_help_text(&commands, Some("TestAgent"));
        // Should show namespaced section
        assert!(output.contains("Namespaced Commands"));
        assert!(output.contains("crucible:search"));
        // Should show agent info
        assert!(output.contains("TestAgent"));
        assert!(output.contains("Agent Commands"));
    }

    #[test]
    fn test_render_help_text_shows_hints() {
        let commands = vec![CommandDescriptor {
            name: "search".to_string(),
            description: "Search".to_string(),
            input_hint: Some("query".to_string()),
            secondary_options: Vec::new(),
        }];
        let output = render_help_text(&commands, None);
        assert!(output.contains("<query>"));
    }
}
