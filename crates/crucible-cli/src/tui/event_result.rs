//! Event result types for TUI event handling
//!
//! This module defines the outcome types for event handlers, enabling
//! a clean separation between event handling and action dispatch.

use crate::tui::state::PopupItem;

/// Result of handling an event
#[derive(Debug, Clone, PartialEq)]
pub enum EventResult {
    /// Event was not handled by this component
    Ignored,
    /// Event was handled, no visual change needed
    Handled,
    /// Event was handled, UI needs repaint
    NeedsRender,
    /// Event produced an action that bubbles up to the runner
    Action(TuiAction),
}

/// Actions that bubble up from event handling to the runner
#[derive(Debug, Clone, PartialEq)]
pub enum TuiAction {
    /// Send a message to the agent
    SendMessage(String),
    /// Execute a slash command
    ExecuteCommand(String),
    /// Cycle through modes (plan/act/auto)
    CycleMode,
    /// Cancel current operation (single Ctrl+C)
    Cancel,
    /// Exit the application
    Exit,
    /// Scroll the conversation
    Scroll(ScrollAction),
    /// Popup confirmed with selection
    PopupConfirm(PopupItem),
    /// Popup closed without selection
    PopupClose,
    /// Dialog confirmed
    DialogConfirm,
    /// Dialog cancelled
    DialogCancel,
    /// Dialog option selected
    DialogSelect(usize),
    /// Dialog dismissed (info only)
    DialogDismiss,
}

/// Scroll actions for conversation navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAction {
    Up(usize),
    Down(usize),
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    ToTop,
    ToBottom,
}

impl EventResult {
    /// Combine two results, preferring the more significant one.
    ///
    /// Priority order: Action > NeedsRender > Handled > Ignored
    pub fn or(self, other: EventResult) -> EventResult {
        match (&self, &other) {
            (EventResult::Action(_), _) => self,
            (_, EventResult::Action(_)) => other,
            (EventResult::NeedsRender, _) => self,
            (_, EventResult::NeedsRender) => other,
            (EventResult::Handled, _) => self,
            (_, EventResult::Handled) => other,
            _ => EventResult::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // EventResult variant tests
    // ==========================================================================

    #[test]
    fn test_event_result_variants_exist() {
        let ignored = EventResult::Ignored;
        let handled = EventResult::Handled;
        let needs_render = EventResult::NeedsRender;
        let action = EventResult::Action(TuiAction::Exit);

        assert!(matches!(ignored, EventResult::Ignored));
        assert!(matches!(handled, EventResult::Handled));
        assert!(matches!(needs_render, EventResult::NeedsRender));
        assert!(matches!(action, EventResult::Action(TuiAction::Exit)));
    }

    // ==========================================================================
    // EventResult::or() priority tests
    // ==========================================================================

    #[test]
    fn test_or_action_wins_over_everything() {
        let action = EventResult::Action(TuiAction::Exit);

        assert!(matches!(
            EventResult::Ignored.or(action.clone()),
            EventResult::Action(_)
        ));
        assert!(matches!(
            EventResult::Handled.or(action.clone()),
            EventResult::Action(_)
        ));
        assert!(matches!(
            EventResult::NeedsRender.or(action.clone()),
            EventResult::Action(_)
        ));
    }

    #[test]
    fn test_or_action_on_left_wins() {
        let action = EventResult::Action(TuiAction::Cancel);

        assert!(matches!(
            action.clone().or(EventResult::Ignored),
            EventResult::Action(TuiAction::Cancel)
        ));
        assert!(matches!(
            action.clone().or(EventResult::Handled),
            EventResult::Action(TuiAction::Cancel)
        ));
        assert!(matches!(
            action.clone().or(EventResult::NeedsRender),
            EventResult::Action(TuiAction::Cancel)
        ));
    }

    #[test]
    fn test_or_needs_render_wins_over_handled_and_ignored() {
        assert!(matches!(
            EventResult::Handled.or(EventResult::NeedsRender),
            EventResult::NeedsRender
        ));
        assert!(matches!(
            EventResult::NeedsRender.or(EventResult::Handled),
            EventResult::NeedsRender
        ));
        assert!(matches!(
            EventResult::Ignored.or(EventResult::NeedsRender),
            EventResult::NeedsRender
        ));
        assert!(matches!(
            EventResult::NeedsRender.or(EventResult::Ignored),
            EventResult::NeedsRender
        ));
    }

    #[test]
    fn test_or_handled_wins_over_ignored() {
        assert!(matches!(
            EventResult::Ignored.or(EventResult::Handled),
            EventResult::Handled
        ));
        assert!(matches!(
            EventResult::Handled.or(EventResult::Ignored),
            EventResult::Handled
        ));
    }

    #[test]
    fn test_or_ignored_with_ignored() {
        assert!(matches!(
            EventResult::Ignored.or(EventResult::Ignored),
            EventResult::Ignored
        ));
    }

    // ==========================================================================
    // TuiAction variant tests
    // ==========================================================================

    #[test]
    fn test_tui_action_send_message() {
        let action = TuiAction::SendMessage("hello world".into());
        assert!(matches!(
            action,
            TuiAction::SendMessage(s) if s == "hello world"
        ));
    }

    #[test]
    fn test_tui_action_execute_command() {
        let action = TuiAction::ExecuteCommand("/help".into());
        assert!(matches!(
            action,
            TuiAction::ExecuteCommand(s) if s == "/help"
        ));
    }

    #[test]
    fn test_tui_action_scroll_variants() {
        let up = TuiAction::Scroll(ScrollAction::Up(3));
        let down = TuiAction::Scroll(ScrollAction::Down(5));
        let page_up = TuiAction::Scroll(ScrollAction::PageUp);
        let page_down = TuiAction::Scroll(ScrollAction::PageDown);
        let half_up = TuiAction::Scroll(ScrollAction::HalfPageUp);
        let half_down = TuiAction::Scroll(ScrollAction::HalfPageDown);
        let to_top = TuiAction::Scroll(ScrollAction::ToTop);
        let to_bottom = TuiAction::Scroll(ScrollAction::ToBottom);

        assert!(matches!(up, TuiAction::Scroll(ScrollAction::Up(3))));
        assert!(matches!(down, TuiAction::Scroll(ScrollAction::Down(5))));
        assert!(matches!(page_up, TuiAction::Scroll(ScrollAction::PageUp)));
        assert!(matches!(page_down, TuiAction::Scroll(ScrollAction::PageDown)));
        assert!(matches!(half_up, TuiAction::Scroll(ScrollAction::HalfPageUp)));
        assert!(matches!(half_down, TuiAction::Scroll(ScrollAction::HalfPageDown)));
        assert!(matches!(to_top, TuiAction::Scroll(ScrollAction::ToTop)));
        assert!(matches!(to_bottom, TuiAction::Scroll(ScrollAction::ToBottom)));
    }

    #[test]
    fn test_tui_action_dialog_variants() {
        assert!(matches!(TuiAction::DialogConfirm, TuiAction::DialogConfirm));
        assert!(matches!(TuiAction::DialogCancel, TuiAction::DialogCancel));
        assert!(matches!(TuiAction::DialogDismiss, TuiAction::DialogDismiss));

        let select = TuiAction::DialogSelect(2);
        assert!(matches!(select, TuiAction::DialogSelect(2)));
    }

    #[test]
    fn test_tui_action_popup_close() {
        assert!(matches!(TuiAction::PopupClose, TuiAction::PopupClose));
    }

    #[test]
    fn test_tui_action_simple_variants() {
        assert!(matches!(TuiAction::CycleMode, TuiAction::CycleMode));
        assert!(matches!(TuiAction::Cancel, TuiAction::Cancel));
        assert!(matches!(TuiAction::Exit, TuiAction::Exit));
    }

    // ==========================================================================
    // ScrollAction tests
    // ==========================================================================

    #[test]
    fn test_scroll_action_copy() {
        let scroll = ScrollAction::Up(5);
        let copy = scroll;
        assert_eq!(scroll, copy);
    }

    #[test]
    fn test_scroll_action_eq() {
        assert_eq!(ScrollAction::Up(3), ScrollAction::Up(3));
        assert_ne!(ScrollAction::Up(3), ScrollAction::Up(5));
        assert_ne!(ScrollAction::Up(3), ScrollAction::Down(3));
        assert_eq!(ScrollAction::PageUp, ScrollAction::PageUp);
        assert_ne!(ScrollAction::PageUp, ScrollAction::PageDown);
    }
}
