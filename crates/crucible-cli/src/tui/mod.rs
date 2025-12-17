//! Terminal User Interface for interactive chat
//!
//! This module provides a ratatui-based TUI that:
//! - Streams responses token-by-token via TextDelta events
//! - Shows tool call progress with spinners
//! - Supports mode switching (Plan/Act/AutoApprove)
//! - Handles cancellation (Ctrl+C)

pub mod input;
pub mod render;
pub mod runner;
pub mod state;
pub mod streaming;
pub mod widget;

pub use input::{map_key_event, InputAction};
pub use render::render;
pub use runner::TuiRunner;
pub use state::TuiState;
pub use streaming::StreamingBuffer;
pub use widget::{
    ansi, calculate_heights, calculate_position, move_to_widget, render_input_area,
    render_separator, render_status_line, render_streaming_area, render_widget, WidgetHeights,
    WidgetPosition, WidgetState, WidgetStateDynamic,
    mode_icon, mode_color, render_status_line_dynamic, render_widget_dynamic,
    format_help_command, render_help_text,
};
