//! TUI styling constants and utilities
//!
//! Defines colors, indicators, and styling for the chat interface.
//! Design philosophy: Color-based hierarchy, no borders on messages.

use ratatui::style::{Color, Modifier, Style};

// =============================================================================
// Color Palette
// =============================================================================

/// Colors for the TUI interface
pub mod colors {
    use super::*;

    // --- Message colors ---

    /// User message background (inverted display)
    pub const USER_BG: Color = Color::Rgb(60, 60, 80); // Subtle blue-gray

    /// User message foreground (inverted display)
    pub const USER_FG: Color = Color::White;

    /// Assistant message foreground (normal, bright)
    pub const ASSISTANT_FG: Color = Color::Reset; // Terminal default

    /// Dim text (status, metadata)
    pub const DIM: Color = Color::DarkGray;

    // --- Input box colors ---

    /// Input box background (darker)
    pub const INPUT_BG: Color = Color::Rgb(35, 35, 50); // Dark blue-gray

    /// Input text color
    pub const INPUT_FG: Color = Color::White;

    /// Shell passthrough (!) background - red tint
    pub const INPUT_SHELL_BG: Color = Color::Rgb(60, 30, 30); // Dark red

    /// REPL command (:) background - green tint
    pub const INPUT_REPL_BG: Color = Color::Rgb(30, 50, 30); // Dark green

    // --- Mode colors ---

    /// Plan mode color
    pub const MODE_PLAN: Color = Color::Cyan;

    /// Act mode color
    pub const MODE_ACT: Color = Color::Yellow;

    /// Auto mode color
    pub const MODE_AUTO: Color = Color::Red;

    // --- Status indicators ---

    /// Thinking/processing indicator
    pub const THINKING: Color = Color::Cyan;

    /// Generating/streaming indicator
    pub const STREAMING: Color = Color::Green;

    /// Tool running indicator (white spinner)
    pub const TOOL_RUNNING: Color = Color::White;

    /// Tool complete indicator
    pub const TOOL_COMPLETE: Color = Color::Green;

    /// Tool error indicator
    pub const TOOL_ERROR: Color = Color::Red;

    /// Token count / metrics
    pub const METRICS: Color = Color::DarkGray;
}

// =============================================================================
// Status Indicators
// =============================================================================

/// Unicode indicators for various states
pub mod indicators {
    /// Thinking indicator (filled circle)
    pub const THINKING: &str = "●";

    /// Generating/streaming indicator (half circle, animated would cycle)
    pub const STREAMING: &str = "◐";

    /// Tool running spinner frames
    pub const SPINNER_FRAMES: &[&str] = &["◐", "◓", "◑", "◒"];

    /// Tool complete indicator (dot, matches assistant prefix)
    pub const TOOL_COMPLETE: &str = "●";

    /// Tool error X
    pub const TOOL_ERROR: &str = "✗";

    /// Legacy checkmark (kept for backwards compatibility)
    pub const COMPLETE: &str = "✓";

    /// User message prefix (for historical messages)
    pub const USER_PREFIX: &str = ">";

    /// Assistant message prefix (aligns with user prefix column)
    /// Options considered: · (too subtle), ▸ (used for mode), › (too similar to >)
    pub const ASSISTANT_PREFIX: &str = "●";

    /// Mode indicator arrow
    pub const MODE_ARROW: &str = "▸";

    /// Separator character
    pub const SEPARATOR: char = '─';
}

// =============================================================================
// Pre-built Styles
// =============================================================================

/// Pre-built styles for common elements
pub mod presets {
    use super::*;

    /// User message style (inverted)
    pub fn user_message() -> Style {
        Style::default().fg(colors::USER_FG).bg(colors::USER_BG)
    }

    /// User message prefix style
    pub fn user_prefix() -> Style {
        Style::default()
            .fg(colors::USER_FG)
            .bg(colors::USER_BG)
            .add_modifier(Modifier::BOLD)
    }

    /// Assistant message style (normal, readable)
    pub fn assistant_message() -> Style {
        Style::default().fg(colors::ASSISTANT_FG)
    }

    /// Assistant message prefix style (subtle, aligns with user prefix)
    pub fn assistant_prefix() -> Style {
        Style::default().fg(colors::DIM)
    }

    /// Dim text style (status, metadata)
    pub fn dim() -> Style {
        Style::default().fg(colors::DIM)
    }

    /// Thinking indicator style
    pub fn thinking() -> Style {
        Style::default()
            .fg(colors::THINKING)
            .add_modifier(Modifier::BOLD)
    }

    /// Streaming indicator style
    pub fn streaming() -> Style {
        Style::default()
            .fg(colors::STREAMING)
            .add_modifier(Modifier::BOLD)
    }

    /// Tool running style
    pub fn tool_running() -> Style {
        Style::default().fg(colors::TOOL_RUNNING)
    }

    /// Tool complete style
    pub fn tool_complete() -> Style {
        Style::default().fg(colors::TOOL_COMPLETE)
    }

    /// Tool error style
    pub fn tool_error() -> Style {
        Style::default()
            .fg(colors::TOOL_ERROR)
            .add_modifier(Modifier::BOLD)
    }

    /// Tool output style (indented, dim)
    pub fn tool_output() -> Style {
        Style::default().fg(colors::DIM)
    }

    /// Input box style
    pub fn input_box() -> Style {
        Style::default().fg(colors::INPUT_FG).bg(colors::INPUT_BG)
    }

    /// Input box style for shell passthrough (!)
    pub fn input_shell() -> Style {
        Style::default()
            .fg(colors::INPUT_FG)
            .bg(colors::INPUT_SHELL_BG)
    }

    /// Input box style for REPL commands (:)
    pub fn input_repl() -> Style {
        Style::default()
            .fg(colors::INPUT_FG)
            .bg(colors::INPUT_REPL_BG)
    }

    /// Status line style (no background, dim text)
    pub fn status_line() -> Style {
        Style::default().fg(colors::DIM)
    }

    /// Token/metrics style
    pub fn metrics() -> Style {
        Style::default().fg(colors::METRICS)
    }

    /// Mode style based on mode_id
    pub fn mode(mode_id: &str) -> Style {
        let color = match mode_id {
            "plan" => colors::MODE_PLAN,
            "act" => colors::MODE_ACT,
            "auto" => colors::MODE_AUTO,
            _ => colors::DIM,
        };
        Style::default().fg(color)
    }
}

// =============================================================================
// Formatting Helpers
// =============================================================================

/// Format a user message for display (with prefix, for scrollback)
pub fn format_user_message(content: &str) -> String {
    format!(" {} {} ", indicators::USER_PREFIX, content)
}

/// Format thinking status
pub fn format_thinking() -> String {
    format!("{} Thinking...", indicators::THINKING)
}

/// Format streaming status with token count
pub fn format_streaming(token_count: usize) -> String {
    if token_count > 0 {
        format!(
            "{} Generating... ({} tokens)",
            indicators::STREAMING,
            token_count
        )
    } else {
        format!("{} Generating...", indicators::STREAMING)
    }
}

/// Format tool running status
pub fn format_tool_running(name: &str) -> String {
    format!("{} {}", indicators::SPINNER_FRAMES[0], name)
}

/// Format tool complete status with summary
pub fn format_tool_complete(name: &str, summary: Option<&str>) -> String {
    match summary {
        Some(s) => format!("{} {} → {}", indicators::COMPLETE, name, s),
        None => format!("{} {}", indicators::COMPLETE, name),
    }
}

/// Format tool error
pub fn format_tool_error(name: &str, error: &str) -> String {
    format!("{} {} → {}", indicators::TOOL_ERROR, name, error)
}

/// Format status line
pub fn format_status_line(mode_id: &str, token_count: Option<usize>, status: &str) -> String {
    let mode_name = match mode_id {
        "plan" => "Plan",
        "act" => "Act",
        "auto" => "Auto",
        _ => mode_id,
    };

    match token_count {
        Some(count) => format!(
            "{} {} │ {} tokens │ {}",
            indicators::MODE_ARROW,
            mode_name,
            count,
            status
        ),
        None => format!("{} {} │ {}", indicators::MODE_ARROW, mode_name, status),
    }
}

/// Truncate tool output to last N lines
pub fn truncate_output(output: &str, max_lines: usize) -> Vec<&str> {
    let lines: Vec<&str> = output.lines().collect();
    let len = lines.len();
    if len <= max_lines {
        lines
    } else {
        lines.into_iter().skip(len - max_lines).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_user_message() {
        let msg = format_user_message("Hello world");
        assert!(msg.contains(indicators::USER_PREFIX));
        assert!(msg.contains("Hello world"));
    }

    #[test]
    fn test_format_thinking() {
        let status = format_thinking();
        assert!(status.contains(indicators::THINKING));
        assert!(status.contains("Thinking"));
    }

    #[test]
    fn test_format_streaming_with_tokens() {
        let status = format_streaming(127);
        assert!(status.contains("127 tokens"));
    }

    #[test]
    fn test_format_streaming_no_tokens() {
        let status = format_streaming(0);
        assert!(!status.contains("tokens"));
        assert!(status.contains("Generating"));
    }

    #[test]
    fn test_format_tool_running() {
        let status = format_tool_running("grep");
        assert!(status.contains("grep"));
        assert!(status.contains(indicators::SPINNER_FRAMES[0]));
    }

    #[test]
    fn test_format_tool_complete_with_summary() {
        let status = format_tool_complete("glob", Some("3 files"));
        assert!(status.contains(indicators::COMPLETE));
        assert!(status.contains("glob"));
        assert!(status.contains("3 files"));
    }

    #[test]
    fn test_format_tool_error() {
        let status = format_tool_error("read", "file not found");
        assert!(status.contains(indicators::TOOL_ERROR));
        assert!(status.contains("file not found"));
    }

    #[test]
    fn test_format_status_line() {
        let status = format_status_line("plan", Some(127), "Ready");
        assert!(status.contains("Plan"));
        assert!(status.contains("127 tokens"));
        assert!(status.contains("Ready"));
    }

    #[test]
    fn test_truncate_output_short() {
        let output = "line1\nline2";
        let lines = truncate_output(output, 5);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_truncate_output_long() {
        let output = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10";
        let lines = truncate_output(output, 3);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "8");
        assert_eq!(lines[2], "10");
    }

    #[test]
    fn test_mode_style() {
        let plan_style = presets::mode("plan");
        let act_style = presets::mode("act");
        // Just verify they don't panic and return different styles
        assert_ne!(format!("{:?}", plan_style), format!("{:?}", act_style));
    }
}
