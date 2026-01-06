//! Terminal User Interface for interactive chat
//!
//! This module provides a ratatui-based TUI that:
//! - Streams responses token-by-token via TextDelta events
//! - Shows tool call progress with spinners
//! - Supports mode switching (Plan/Act/AutoApprove)
//! - Handles cancellation (Ctrl+C)

pub mod action_dispatch;
pub mod agent_picker;
pub mod chat_view;
pub mod components;
pub mod content_block;
pub mod conversation;
pub mod conversation_view;
pub mod dialog;
pub mod event_result;
pub mod help;
pub mod help_assets;
pub mod input;
pub mod interaction;
pub mod keymap;
pub mod markdown;
pub mod notification;
pub mod popup;
pub mod registries;
pub mod render;
pub mod repl_commands;
pub mod runner;
pub mod scrollback_runner;
pub mod selection;
pub mod state;
pub mod streaming;
pub mod streaming_channel;
pub mod streaming_parser;
pub mod styles;
#[cfg(any(test, feature = "test-utils"))]
pub mod testing;
pub mod viewport;
pub mod widget;
pub mod widgets;

#[cfg(test)]
mod conversation_ordering_tests;

pub use action_dispatch::{
    dispatch, popup_item_to_effect, ContextResolver, DefaultContextResolver, DialogEffect,
    PopupEffect, PopupHook, PopupHooks, RunnerEffect, ScrollEffect,
};
pub use agent_picker::AgentSelection;
pub use chat_view::ChatView;
pub use components::{DialogAction, FocusTarget, InteractiveWidget};
// Note: EventResult and TuiAction have two versions:
// - components::{EventResult, TuiAction} - widget-level events (legacy)
// - event_result::{EventResult, TuiAction} - unified event system (new)
// Using the new unified versions as the default export:
pub use content_block::{ParseEvent, StreamBlock};
pub use dialog::{DialogResult, DialogStack, DialogState, DialogWidget};
pub use event_result::{EventResult, ScrollAction, TuiAction};
pub use help::{DocsIndex, HelpTopic};
pub use input::{map_key_event, InputAction};
pub use markdown::MarkdownRenderer;
pub use notification::{NotificationLevel, NotificationState};
pub use popup::{DynamicPopupProvider, PopupProvider, StaticPopupProvider};
pub use registries::{CommandRegistry, CompositeRegistry, ContextRegistry, ReplCommandRegistry};
pub use render::render;
pub use runner::RatatuiRunner;
pub use selection::{RenderedLineInfo, SelectableContentCache, SelectionPoint, SelectionState};
pub use state::{ContextAttachment, ContextKind, TuiState};
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
