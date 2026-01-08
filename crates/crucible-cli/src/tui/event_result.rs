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
    /// Event was handled and consumed, stop propagation
    Consumed,
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
    // === Scroll actions (from WidgetAction) ===
    /// Scroll the conversation by the given number of lines (positive = down)
    ScrollLines(isize),
    /// Scroll to absolute position (0 = top, usize::MAX = bottom)
    ScrollTo(usize),
    /// Scroll with page navigation
    ScrollPage(ScrollDirection),

    // === Input actions ===
    /// Send a message to the agent
    SendMessage(String),
    /// Execute a slash command
    ExecuteCommand(String),

    // === Popup actions (from WidgetAction) ===
    /// Confirm popup selection with the selected item index
    ConfirmPopup(usize),
    /// Dismiss the current popup
    DismissPopup,
    /// Popup confirmed with selection (with resolved item)
    PopupConfirm(PopupItem),
    /// Popup closed without selection
    PopupClose,

    // === Dialog actions (from WidgetAction) ===
    /// Close the current dialog with a result
    CloseDialog(DialogResult),
    /// Dialog confirmed
    DialogConfirm,
    /// Dialog cancelled
    DialogCancel,
    /// Dialog option selected
    DialogSelect(usize),
    /// Dialog dismissed (info only)
    DialogDismiss,

    // === Mode/focus actions (from WidgetAction) ===
    /// Cycle through session modes (plan -> act -> auto)
    CycleMode,
    /// Request focus change to a different widget
    RequestFocus(FocusTarget),

    // === Control actions ===
    /// Cancel current operation (single Ctrl+C)
    Cancel,
    /// Exit the application
    Exit,
}

/// Direction for page scrolling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
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

/// Result of closing a dialog
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult {
    /// User confirmed the dialog
    Confirm,
    /// User cancelled the dialog
    Cancel,
    /// User selected an item (for select dialogs)
    Select(usize),
}

impl EventResult {
    /// Combine two results, preferring the more significant one.
    ///
    /// Priority order: Action > NeedsRender > Consumed > Handled > Ignored
    pub fn or(self, other: EventResult) -> EventResult {
        match (&self, &other) {
            (EventResult::Action(_), _) => self,
            (_, EventResult::Action(_)) => other,
            (EventResult::NeedsRender, _) => self,
            (_, EventResult::NeedsRender) => other,
            (EventResult::Consumed, _) => self,
            (_, EventResult::Consumed) => other,
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
        let consumed = EventResult::Consumed;
        let handled = EventResult::Handled;
        let needs_render = EventResult::NeedsRender;
        let action = EventResult::Action(TuiAction::Exit);

        assert!(matches!(ignored, EventResult::Ignored));
        assert!(matches!(consumed, EventResult::Consumed));
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
            EventResult::Consumed.or(action.clone()),
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
            action.clone().or(EventResult::Consumed),
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
    fn test_or_needs_render_wins_over_consumed_handled_and_ignored() {
        assert!(matches!(
            EventResult::Consumed.or(EventResult::NeedsRender),
            EventResult::NeedsRender
        ));
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
    fn test_or_consumed_wins_over_handled_and_ignored() {
        assert!(matches!(
            EventResult::Handled.or(EventResult::Consumed),
            EventResult::Consumed
        ));
        assert!(matches!(
            EventResult::Consumed.or(EventResult::Handled),
            EventResult::Consumed
        ));
        assert!(matches!(
            EventResult::Ignored.or(EventResult::Consumed),
            EventResult::Consumed
        ));
        assert!(matches!(
            EventResult::Consumed.or(EventResult::Ignored),
            EventResult::Consumed
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
        let lines = TuiAction::ScrollLines(5);
        let to = TuiAction::ScrollTo(100);
        let page_up = TuiAction::ScrollPage(ScrollDirection::Up);
        let page_down = TuiAction::ScrollPage(ScrollDirection::Down);

        assert!(matches!(lines, TuiAction::ScrollLines(5)));
        assert!(matches!(to, TuiAction::ScrollTo(100)));
        assert!(matches!(page_up, TuiAction::ScrollPage(ScrollDirection::Up)));
        assert!(matches!(page_down, TuiAction::ScrollPage(ScrollDirection::Down)));
    }

    #[test]
    fn test_tui_action_popup_variants() {
        let confirm = TuiAction::ConfirmPopup(0);
        let dismiss = TuiAction::DismissPopup;

        assert!(matches!(confirm, TuiAction::ConfirmPopup(0)));
        assert!(matches!(dismiss, TuiAction::DismissPopup));
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
    fn test_tui_action_close_dialog() {
        let confirm = TuiAction::CloseDialog(DialogResult::Confirm);
        let cancel = TuiAction::CloseDialog(DialogResult::Cancel);
        let select = TuiAction::CloseDialog(DialogResult::Select(1));

        assert!(matches!(confirm, TuiAction::CloseDialog(DialogResult::Confirm)));
        assert!(matches!(cancel, TuiAction::CloseDialog(DialogResult::Cancel)));
        assert!(matches!(select, TuiAction::CloseDialog(DialogResult::Select(1))));
    }

    #[test]
    fn test_tui_action_popup_close() {
        assert!(matches!(TuiAction::PopupClose, TuiAction::PopupClose));
    }

    #[test]
    fn test_tui_action_focus_target() {
        let input = TuiAction::RequestFocus(FocusTarget::Input);
        let history = TuiAction::RequestFocus(FocusTarget::History);
        let popup = TuiAction::RequestFocus(FocusTarget::Popup);
        let dialog = TuiAction::RequestFocus(FocusTarget::Dialog);

        assert!(matches!(input, TuiAction::RequestFocus(FocusTarget::Input)));
        assert!(matches!(history, TuiAction::RequestFocus(FocusTarget::History)));
        assert!(matches!(popup, TuiAction::RequestFocus(FocusTarget::Popup)));
        assert!(matches!(dialog, TuiAction::RequestFocus(FocusTarget::Dialog)));
    }

    #[test]
    fn test_tui_action_simple_variants() {
        assert!(matches!(TuiAction::CycleMode, TuiAction::CycleMode));
        assert!(matches!(TuiAction::Cancel, TuiAction::Cancel));
        assert!(matches!(TuiAction::Exit, TuiAction::Exit));
    }

    // ==========================================================================
    // ScrollDirection tests
    // ==========================================================================

    #[test]
    fn test_scroll_direction_copy() {
        let dir = ScrollDirection::Up;
        let copy = dir;
        assert_eq!(dir, copy);
    }

    #[test]
    fn test_scroll_direction_eq() {
        assert_eq!(ScrollDirection::Up, ScrollDirection::Up);
        assert_ne!(ScrollDirection::Up, ScrollDirection::Down);
    }

    // ==========================================================================
    // FocusTarget tests
    // ==========================================================================

    #[test]
    fn test_focus_target_variants() {
        assert_ne!(FocusTarget::Input, FocusTarget::History);
        assert_ne!(FocusTarget::Popup, FocusTarget::Dialog);
    }

    // ==========================================================================
    // DialogResult tests
    // ==========================================================================

    #[test]
    fn test_dialog_result_variants() {
        assert_eq!(DialogResult::Confirm, DialogResult::Confirm);
        assert_eq!(DialogResult::Select(0), DialogResult::Select(0));
        assert_ne!(DialogResult::Select(0), DialogResult::Select(1));
    }
}
