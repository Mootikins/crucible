//! Terminal User Interface for interactive chat
//!
//! This module provides a ratatui-based TUI that:
//! - Streams responses token-by-token via TextDelta events
//! - Shows tool call progress with spinners
//! - Supports mode switching (Plan/Act/AutoApprove)
//! - Handles cancellation (Ctrl+C)

pub mod agent_picker;
pub mod content_block;
pub mod conversation;
pub mod conversation_view;
pub mod dialog;
pub mod input;
pub mod markdown;
pub mod notification;
pub mod popup;
pub mod render;
pub mod runner;
pub mod scrollback_runner;
pub mod splash;
pub mod state;
pub mod streaming;
pub mod streaming_channel;
pub mod streaming_parser;
pub mod styles;
#[cfg(test)]
pub mod testing;
pub mod viewport;
pub mod widget;

#[cfg(test)]
mod conversation_ordering_tests;
#[cfg(test)]
mod runner_picker_tests;

pub use agent_picker::{pick_agent, AgentSelection};
pub use content_block::{ContentBlock, ParseEvent};
pub use dialog::{DialogKind, DialogResult, DialogStack, DialogState, DialogWidget};
pub use input::{map_key_event, InputAction};
pub use markdown::MarkdownRenderer;
pub use notification::{NotificationLevel, NotificationState};
pub use popup::{DynamicPopupProvider, PopupProvider, StaticPopupProvider};
pub use render::render;
pub use runner::RatatuiRunner;
pub use splash::{AgentOption, SplashState, SplashWidget};
pub use state::TuiState;
pub use streaming::StreamingBuffer;
pub use streaming_channel::{
    create_streaming_channel, ChatStream, StreamingEvent, StreamingReceiver, StreamingSender,
    StreamingTask,
};
pub use streaming_parser::StreamingParser;
pub use widget::{
    ansi, calculate_heights, calculate_position, format_help_command, mode_color, mode_icon,
    move_to_widget, render_help_text, render_input_area, render_separator, render_status_line,
    render_status_line_dynamic, render_streaming_area, render_widget, render_widget_dynamic,
    WidgetHeights, WidgetPosition, WidgetState, WidgetStateDynamic,
};
