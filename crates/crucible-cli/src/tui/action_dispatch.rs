//! Action dispatch for TUI runner
//!
//! This module translates `TuiAction` from the ChatView into effects
//! that the runner can execute. This keeps the runner thin and focused
//! on coordination rather than decision-making.

use crate::tui::event_result::TuiAction;
use crate::tui::state::PopupItem;

/// Effects that the runner should execute
///
/// These are the side effects that require runner coordination,
/// such as starting async operations or exiting the application.
#[derive(Debug, Clone, PartialEq)]
pub enum RunnerEffect {
    /// Exit the main loop
    Exit,

    /// Cancel current operation (streaming or clear input)
    Cancel,

    /// Start sending a message (initiates streaming)
    SendMessage(String),

    /// Execute a slash command
    ExecuteCommand(String),

    /// Cycle through session modes
    CycleMode,

    /// Scroll the conversation view
    Scroll(ScrollEffect),

    /// Apply popup selection to input
    ApplyPopupSelection(PopupItem),

    /// Handle dialog result
    DialogResult(DialogEffect),

    /// Just render (no side effect needed)
    Render,

    /// No effect needed
    None,
}

/// Scroll effects for the conversation view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollEffect {
    /// Scroll up by lines
    Up(usize),
    /// Scroll down by lines
    Down(usize),
    /// Scroll to top
    ToTop,
    /// Scroll to bottom
    ToBottom,
}

/// Dialog result effects
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogEffect {
    /// Dialog was confirmed
    Confirmed,
    /// Dialog was cancelled
    Cancelled,
    /// Item was selected (index)
    Selected(usize),
    /// Dialog was dismissed (info dialogs)
    Dismissed,
}

/// Dispatch a TuiAction to a RunnerEffect
///
/// This is the central translation point that keeps the runner
/// from needing to understand the details of TuiAction.
pub fn dispatch(action: TuiAction) -> RunnerEffect {
    use crate::tui::event_result::ScrollAction;

    match action {
        TuiAction::Exit => RunnerEffect::Exit,
        TuiAction::Cancel => RunnerEffect::Cancel,
        TuiAction::SendMessage(msg) => RunnerEffect::SendMessage(msg),
        TuiAction::ExecuteCommand(cmd) => RunnerEffect::ExecuteCommand(cmd),
        TuiAction::CycleMode => RunnerEffect::CycleMode,

        TuiAction::Scroll(scroll) => {
            let effect = match scroll {
                ScrollAction::Up(n) => ScrollEffect::Up(n),
                ScrollAction::Down(n) => ScrollEffect::Down(n),
                ScrollAction::PageUp => ScrollEffect::Up(10),
                ScrollAction::PageDown => ScrollEffect::Down(10),
                ScrollAction::HalfPageUp => ScrollEffect::Up(5),
                ScrollAction::HalfPageDown => ScrollEffect::Down(5),
                ScrollAction::ToTop => ScrollEffect::ToTop,
                ScrollAction::ToBottom => ScrollEffect::ToBottom,
            };
            RunnerEffect::Scroll(effect)
        }

        TuiAction::PopupConfirm(item) => RunnerEffect::ApplyPopupSelection(item),
        TuiAction::PopupClose => RunnerEffect::Render,

        TuiAction::DialogConfirm => RunnerEffect::DialogResult(DialogEffect::Confirmed),
        TuiAction::DialogCancel => RunnerEffect::DialogResult(DialogEffect::Cancelled),
        TuiAction::DialogSelect(idx) => RunnerEffect::DialogResult(DialogEffect::Selected(idx)),
        TuiAction::DialogDismiss => RunnerEffect::DialogResult(DialogEffect::Dismissed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::event_result::ScrollAction;
    use crate::tui::state::{PopupItem, PopupItemKind};

    // ==========================================================================
    // Basic action dispatch
    // ==========================================================================

    #[test]
    fn test_dispatch_exit() {
        let effect = dispatch(TuiAction::Exit);
        assert_eq!(effect, RunnerEffect::Exit);
    }

    #[test]
    fn test_dispatch_cancel() {
        let effect = dispatch(TuiAction::Cancel);
        assert_eq!(effect, RunnerEffect::Cancel);
    }

    #[test]
    fn test_dispatch_send_message() {
        let effect = dispatch(TuiAction::SendMessage("hello".into()));
        assert_eq!(effect, RunnerEffect::SendMessage("hello".into()));
    }

    #[test]
    fn test_dispatch_execute_command() {
        let effect = dispatch(TuiAction::ExecuteCommand("/help".into()));
        assert_eq!(effect, RunnerEffect::ExecuteCommand("/help".into()));
    }

    #[test]
    fn test_dispatch_cycle_mode() {
        let effect = dispatch(TuiAction::CycleMode);
        assert_eq!(effect, RunnerEffect::CycleMode);
    }

    // ==========================================================================
    // Scroll action dispatch
    // ==========================================================================

    #[test]
    fn test_dispatch_scroll_up() {
        let effect = dispatch(TuiAction::Scroll(ScrollAction::Up(3)));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Up(3)));
    }

    #[test]
    fn test_dispatch_scroll_down() {
        let effect = dispatch(TuiAction::Scroll(ScrollAction::Down(5)));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Down(5)));
    }

    #[test]
    fn test_dispatch_scroll_page_up() {
        let effect = dispatch(TuiAction::Scroll(ScrollAction::PageUp));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Up(10)));
    }

    #[test]
    fn test_dispatch_scroll_page_down() {
        let effect = dispatch(TuiAction::Scroll(ScrollAction::PageDown));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Down(10)));
    }

    #[test]
    fn test_dispatch_scroll_half_page_up() {
        let effect = dispatch(TuiAction::Scroll(ScrollAction::HalfPageUp));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Up(5)));
    }

    #[test]
    fn test_dispatch_scroll_half_page_down() {
        let effect = dispatch(TuiAction::Scroll(ScrollAction::HalfPageDown));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::Down(5)));
    }

    #[test]
    fn test_dispatch_scroll_to_top() {
        let effect = dispatch(TuiAction::Scroll(ScrollAction::ToTop));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::ToTop));
    }

    #[test]
    fn test_dispatch_scroll_to_bottom() {
        let effect = dispatch(TuiAction::Scroll(ScrollAction::ToBottom));
        assert_eq!(effect, RunnerEffect::Scroll(ScrollEffect::ToBottom));
    }

    // ==========================================================================
    // Popup action dispatch
    // ==========================================================================

    #[test]
    fn test_dispatch_popup_confirm() {
        let item = PopupItem::cmd("help").desc("Show help").with_score(100);

        let effect = dispatch(TuiAction::PopupConfirm(item.clone()));
        assert_eq!(effect, RunnerEffect::ApplyPopupSelection(item));
    }

    #[test]
    fn test_dispatch_popup_close() {
        let effect = dispatch(TuiAction::PopupClose);
        assert_eq!(effect, RunnerEffect::Render);
    }

    // ==========================================================================
    // Dialog action dispatch
    // ==========================================================================

    #[test]
    fn test_dispatch_dialog_confirm() {
        let effect = dispatch(TuiAction::DialogConfirm);
        assert_eq!(effect, RunnerEffect::DialogResult(DialogEffect::Confirmed));
    }

    #[test]
    fn test_dispatch_dialog_cancel() {
        let effect = dispatch(TuiAction::DialogCancel);
        assert_eq!(effect, RunnerEffect::DialogResult(DialogEffect::Cancelled));
    }

    #[test]
    fn test_dispatch_dialog_select() {
        let effect = dispatch(TuiAction::DialogSelect(2));
        assert_eq!(
            effect,
            RunnerEffect::DialogResult(DialogEffect::Selected(2))
        );
    }

    #[test]
    fn test_dispatch_dialog_dismiss() {
        let effect = dispatch(TuiAction::DialogDismiss);
        assert_eq!(effect, RunnerEffect::DialogResult(DialogEffect::Dismissed));
    }
}
