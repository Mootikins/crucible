//! TUI Component System
//!
//! Provides composable, interactive widgets that extend ratatui's Widget trait
//! with event handling capabilities.
//!
//! # Architecture
//!
//! The component system builds on ratatui's existing `Widget` trait:
//!
//! - `Widget` - ratatui's trait for stateless rendering
//! - `InteractiveWidget` - our extension adding event handling and focus
//!
//! # Event Propagation
//!
//! Events flow top-down through widget layers. Each widget returns an
//! `EventResult` indicating whether it consumed the event:
//!
//! - `Consumed` - event handled, stop propagation
//! - `Ignored` - event not handled, continue to next widget
//! - `Action(TuiAction)` - event produced an action for the runner
//!
//! # Example
//!
//! ```ignore
//! impl InteractiveWidget for MyWidget {
//!     fn handle_event(&mut self, event: &Event) -> EventResult {
//!         if let Event::Key(key) = event {
//!             if key.code == KeyCode::Up {
//!                 self.scroll_up();
//!                 return EventResult::Consumed;
//!             }
//!         }
//!         EventResult::Ignored
//!     }
//!
//!     fn focusable(&self) -> bool { true }
//! }
//! ```

pub mod dialog;
pub mod dialog_state;
pub mod generic_popup;
pub mod horizontal_scroll;
pub mod input_box;
pub mod input_state;
pub mod layer_stack;
pub mod session_history;
pub mod status_bar;

use crate::tui::event_result::EventResult;
use crossterm::event::Event;
use ratatui::widgets::Widget;

pub use dialog::DialogWidget;
pub use dialog_state::{DialogResult, DialogState};
pub use generic_popup::PopupState;
pub use horizontal_scroll::{HorizontalScrollState, HorizontalScrollWidget};
pub use input_box::{InputBoxWidget, DEFAULT_MAX_INPUT_LINES};
pub use input_state::InputState;
pub use layer_stack::LayerStack;
pub use session_history::SessionHistoryWidget;
pub use status_bar::StatusBarWidget;

/// Extension trait for widgets that handle input events
///
/// This trait extends ratatui's `Widget` with interactive capabilities.
/// Widgets implementing this trait can:
///
/// - Handle keyboard/mouse events
/// - Indicate whether they want focus
/// - Return actions for the runner to process
///
/// # Default Implementation
///
/// The default implementation ignores all events and is not focusable,
/// making it safe to implement for display-only widgets.
pub trait InteractiveWidget: Widget {
    /// Handle an input event
    ///
    /// Returns an `EventResult` indicating how the event was processed:
    /// - `Consumed` if the widget handled the event
    /// - `Ignored` if the event should propagate to other widgets
    /// - `Action(TuiAction)` if the widget needs the runner to do something
    ///
    /// # Default
    ///
    /// Returns `EventResult::Ignored` (event not handled).
    fn handle_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    /// Whether this widget can receive focus
    ///
    /// Focusable widgets receive keyboard events when focused.
    /// Non-focusable widgets only receive events during propagation.
    ///
    /// # Default
    ///
    /// Returns `false` (not focusable).
    fn focusable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_widget_default_implementation() {
        // Test that a default widget returns Ignored
        struct TestWidget;
        impl Widget for TestWidget {
            fn render(self, _area: ratatui::layout::Rect, _buf: &mut ratatui::buffer::Buffer) {}
        }
        impl InteractiveWidget for TestWidget {}

        let mut widget = TestWidget;
        let event = Event::Key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Char('a'),
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        });

        assert_eq!(widget.handle_event(&event), EventResult::Ignored);
        assert!(!widget.focusable());
    }
}
