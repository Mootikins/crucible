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
pub mod input_box;
pub mod input_state;
pub mod layer_stack;
pub mod session_history;
pub mod status_bar;

use crossterm::event::Event;
use ratatui::widgets::Widget;

pub use dialog::DialogWidget;
pub use dialog_state::{DialogResult, DialogState};
pub use generic_popup::{LegacyPopupItem, PopupState};
pub use input_box::{InputBoxWidget, DEFAULT_MAX_INPUT_LINES};
pub use input_state::InputState;
pub use layer_stack::LayerStack;
pub use session_history::SessionHistoryWidget;
pub use status_bar::StatusBarWidget;

/// Result of handling an input event (widget-level)
///
/// Note: For the unified event system, see `crate::tui::event_result::EventResult`.
/// This type is used by the `InteractiveWidget` trait for widget-internal events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetEventResult {
    /// Event was consumed, stop propagation to other widgets
    Consumed,
    /// Event was not handled, continue propagation
    Ignored,
    /// Event produced an action for the runner to handle
    Action(WidgetAction),
}

/// Actions that widgets can request from the runner (widget-level)
///
/// Note: For the unified action system, see `crate::tui::event_result::TuiAction`.
/// This type is used by the `InteractiveWidget` trait for widget-internal actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetAction {
    /// Scroll the conversation by the given number of lines (positive = down)
    Scroll(isize),
    /// Scroll to absolute position (0 = top, usize::MAX = bottom)
    ScrollTo(usize),
    /// Confirm popup selection with the selected item index
    ConfirmPopup(usize),
    /// Dismiss the current popup
    DismissPopup,
    /// Cycle through session modes (plan -> act -> auto)
    CycleMode,
    /// Request focus change to a different widget
    RequestFocus(FocusTarget),
    /// Close the current dialog with a result
    CloseDialog(DialogAction),
}

/// Target for focus changes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    /// Main input box
    Input,
    /// Conversation/history area
    History,
    /// Active popup (if any)
    Popup,
    /// Active dialog (if any)
    Dialog,
}

/// Actions for dialog responses
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogAction {
    /// User confirmed the dialog
    Confirm,
    /// User cancelled the dialog
    Cancel,
    /// User selected an item (for select dialogs)
    Select(usize),
}

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
    /// Returns a `WidgetEventResult` indicating how the event was processed:
    /// - `Consumed` if the widget handled the event
    /// - `Ignored` if the event should propagate to other widgets
    /// - `Action(WidgetAction)` if the widget needs the runner to do something
    ///
    /// # Default
    ///
    /// Returns `WidgetEventResult::Ignored` (event not handled).
    fn handle_event(&mut self, _event: &Event) -> WidgetEventResult {
        WidgetEventResult::Ignored
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
    fn widget_event_result_equality() {
        assert_eq!(WidgetEventResult::Consumed, WidgetEventResult::Consumed);
        assert_eq!(WidgetEventResult::Ignored, WidgetEventResult::Ignored);
        assert_ne!(WidgetEventResult::Consumed, WidgetEventResult::Ignored);
    }

    #[test]
    fn widget_action_scroll() {
        let action = WidgetAction::Scroll(5);
        assert_eq!(action, WidgetAction::Scroll(5));
        assert_ne!(action, WidgetAction::Scroll(-5));
    }

    #[test]
    fn focus_target_variants() {
        assert_ne!(FocusTarget::Input, FocusTarget::History);
        assert_ne!(FocusTarget::Popup, FocusTarget::Dialog);
    }

    #[test]
    fn dialog_action_variants() {
        assert_eq!(DialogAction::Confirm, DialogAction::Confirm);
        assert_eq!(DialogAction::Select(0), DialogAction::Select(0));
        assert_ne!(DialogAction::Select(0), DialogAction::Select(1));
    }
}
