//! Dialog state management with event handling
//!
//! This module provides `DialogState`, a self-contained dialog component that:
//! - Supports three dialog types: Confirm, Select, Info
//! - Handles key events and returns results
//! - Manages selection state for Select dialogs

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Result of a dialog interaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult {
    /// User confirmed the dialog (y/Enter on Confirm)
    Confirmed,
    /// User cancelled the dialog (n/Escape)
    Cancelled,
    /// User selected an option (Select dialog)
    Selected(usize),
    /// User dismissed the dialog (Info dialog)
    Dismissed,
}

/// Type of dialog
#[derive(Debug, Clone, PartialEq, Eq)]
enum DialogKind {
    /// Yes/No confirmation dialog
    Confirm,
    /// Selection from a list of options
    Select,
    /// Information display (dismiss only)
    Info,
}

/// Dialog state with event handling
#[derive(Debug, Clone)]
pub struct DialogState {
    /// Dialog title
    title: String,
    /// Dialog message or content
    message: String,
    /// Type of dialog
    kind: DialogKind,
    /// Options for Select dialogs
    options: Vec<String>,
    /// Currently selected index for Select dialogs
    selected: usize,
}

impl DialogState {
    /// Create a confirmation dialog (Yes/No)
    ///
    /// Responds to:
    /// - `y` or `Enter` → Confirmed
    /// - `n` or `Escape` → Cancelled
    pub fn confirm(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            kind: DialogKind::Confirm,
            options: Vec::new(),
            selected: 0,
        }
    }

    /// Create a selection dialog with options
    ///
    /// Responds to:
    /// - `Up`/`Down` or `k`/`j` → Navigate
    /// - `Enter` → Selected(index)
    /// - `Escape` → Cancelled
    pub fn select(title: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            title: title.into(),
            message: String::new(),
            kind: DialogKind::Select,
            options,
            selected: 0,
        }
    }

    /// Create an information dialog (dismiss only)
    ///
    /// Responds to:
    /// - `Enter`, `Escape`, or `Space` → Dismissed
    pub fn info(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            kind: DialogKind::Info,
            options: Vec::new(),
            selected: 0,
        }
    }

    /// Get the dialog title
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Get the dialog message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the options (for Select dialogs)
    pub fn options(&self) -> &[String] {
        &self.options
    }

    /// Get the currently selected index (for Select dialogs)
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Check if this is a confirm dialog
    pub fn is_confirm(&self) -> bool {
        self.kind == DialogKind::Confirm
    }

    /// Check if this is a select dialog
    pub fn is_select(&self) -> bool {
        self.kind == DialogKind::Select
    }

    /// Check if this is an info dialog
    pub fn is_info(&self) -> bool {
        self.kind == DialogKind::Info
    }

    /// Handle a key event
    ///
    /// Returns `Some(DialogResult)` if the dialog should close,
    /// or `None` if the event was handled but dialog stays open.
    pub fn handle_key(&mut self, key: &KeyEvent) -> Option<DialogResult> {
        match self.kind {
            DialogKind::Confirm => self.handle_confirm_key(key),
            DialogKind::Select => self.handle_select_key(key),
            DialogKind::Info => self.handle_info_key(key),
        }
    }

    fn handle_confirm_key(&mut self, key: &KeyEvent) -> Option<DialogResult> {
        match (key.code, key.modifiers) {
            // Confirm: y or Enter
            (KeyCode::Char('y'), KeyModifiers::NONE)
            | (KeyCode::Char('Y'), KeyModifiers::SHIFT)
            | (KeyCode::Enter, KeyModifiers::NONE) => Some(DialogResult::Confirmed),

            // Cancel: n or Escape
            (KeyCode::Char('n'), KeyModifiers::NONE)
            | (KeyCode::Char('N'), KeyModifiers::SHIFT)
            | (KeyCode::Esc, KeyModifiers::NONE) => Some(DialogResult::Cancelled),

            // Other keys ignored
            _ => None,
        }
    }

    fn handle_select_key(&mut self, key: &KeyEvent) -> Option<DialogResult> {
        match (key.code, key.modifiers) {
            // Navigation: Up/Down or k/j
            (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                None
            }
            (KeyCode::Down, KeyModifiers::NONE) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                if !self.options.is_empty() && self.selected < self.options.len() - 1 {
                    self.selected += 1;
                }
                None
            }

            // Select: Enter
            (KeyCode::Enter, KeyModifiers::NONE) => Some(DialogResult::Selected(self.selected)),

            // Cancel: Escape
            (KeyCode::Esc, KeyModifiers::NONE) => Some(DialogResult::Cancelled),

            // Other keys ignored
            _ => None,
        }
    }

    fn handle_info_key(&mut self, key: &KeyEvent) -> Option<DialogResult> {
        match (key.code, key.modifiers) {
            // Dismiss: Enter, Escape, or Space
            (KeyCode::Enter, KeyModifiers::NONE)
            | (KeyCode::Esc, KeyModifiers::NONE)
            | (KeyCode::Char(' '), KeyModifiers::NONE) => Some(DialogResult::Dismissed),

            // Other keys ignored
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // Confirm dialog tests
    // ==========================================================================

    #[test]
    fn test_confirm_dialog_y_confirms() {
        let mut dialog = DialogState::confirm("Title", "Are you sure?");

        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let result = dialog.handle_key(&y);

        assert!(matches!(result, Some(DialogResult::Confirmed)));
    }

    #[test]
    fn test_confirm_dialog_uppercase_y_confirms() {
        let mut dialog = DialogState::confirm("Title", "Are you sure?");

        let y = KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT);
        let result = dialog.handle_key(&y);

        assert!(matches!(result, Some(DialogResult::Confirmed)));
    }

    #[test]
    fn test_confirm_dialog_n_cancels() {
        let mut dialog = DialogState::confirm("Title", "Are you sure?");

        let n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let result = dialog.handle_key(&n);

        assert!(matches!(result, Some(DialogResult::Cancelled)));
    }

    #[test]
    fn test_confirm_dialog_uppercase_n_cancels() {
        let mut dialog = DialogState::confirm("Title", "Are you sure?");

        let n = KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT);
        let result = dialog.handle_key(&n);

        assert!(matches!(result, Some(DialogResult::Cancelled)));
    }

    #[test]
    fn test_confirm_dialog_esc_cancels() {
        let mut dialog = DialogState::confirm("Title", "Are you sure?");

        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = dialog.handle_key(&esc);

        assert!(matches!(result, Some(DialogResult::Cancelled)));
    }

    #[test]
    fn test_confirm_dialog_enter_confirms() {
        let mut dialog = DialogState::confirm("Title", "Are you sure?");

        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = dialog.handle_key(&enter);

        assert!(matches!(result, Some(DialogResult::Confirmed)));
    }

    #[test]
    fn test_confirm_dialog_other_keys_ignored() {
        let mut dialog = DialogState::confirm("Title", "Message");

        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(dialog.handle_key(&a).is_none());
    }

    // ==========================================================================
    // Select dialog tests
    // ==========================================================================

    #[test]
    fn test_select_dialog_navigation() {
        let mut dialog = DialogState::select(
            "Choose",
            vec!["Option A".into(), "Option B".into(), "Option C".into()],
        );

        assert_eq!(dialog.selected_index(), 0);

        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        dialog.handle_key(&down);
        assert_eq!(dialog.selected_index(), 1);

        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        dialog.handle_key(&up);
        assert_eq!(dialog.selected_index(), 0);
    }

    #[test]
    fn test_select_dialog_vim_navigation() {
        let mut dialog = DialogState::select("Choose", vec!["A".into(), "B".into(), "C".into()]);

        let j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        dialog.handle_key(&j);
        assert_eq!(dialog.selected_index(), 1);

        let k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        dialog.handle_key(&k);
        assert_eq!(dialog.selected_index(), 0);
    }

    #[test]
    fn test_select_dialog_navigation_bounds() {
        let mut dialog = DialogState::select("Choose", vec!["A".into(), "B".into()]);

        // At start, can't go up
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        dialog.handle_key(&up);
        assert_eq!(dialog.selected_index(), 0);

        // Move to end
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        dialog.handle_key(&down);
        assert_eq!(dialog.selected_index(), 1);

        // At end, can't go down
        dialog.handle_key(&down);
        assert_eq!(dialog.selected_index(), 1);
    }

    #[test]
    fn test_select_dialog_enter_selects() {
        let mut dialog = DialogState::select("Choose", vec!["A".into(), "B".into()]);
        dialog.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = dialog.handle_key(&enter);

        assert!(matches!(result, Some(DialogResult::Selected(1))));
    }

    #[test]
    fn test_select_dialog_esc_cancels() {
        let mut dialog = DialogState::select("Choose", vec!["A".into()]);

        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = dialog.handle_key(&esc);

        assert!(matches!(result, Some(DialogResult::Cancelled)));
    }

    #[test]
    fn test_select_dialog_empty_options() {
        let mut dialog = DialogState::select("Choose", vec![]);

        // Navigation on empty should not crash
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        dialog.handle_key(&down);
        assert_eq!(dialog.selected_index(), 0);

        // Enter still returns Selected(0)
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = dialog.handle_key(&enter);
        assert!(matches!(result, Some(DialogResult::Selected(0))));
    }

    // ==========================================================================
    // Info dialog tests
    // ==========================================================================

    #[test]
    fn test_info_dialog_enter_dismisses() {
        let mut dialog = DialogState::info("Info", "Some information");

        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = dialog.handle_key(&enter);

        assert!(matches!(result, Some(DialogResult::Dismissed)));
    }

    #[test]
    fn test_info_dialog_esc_dismisses() {
        let mut dialog = DialogState::info("Info", "Some information");

        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = dialog.handle_key(&esc);

        assert!(matches!(result, Some(DialogResult::Dismissed)));
    }

    #[test]
    fn test_info_dialog_space_dismisses() {
        let mut dialog = DialogState::info("Info", "Some information");

        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let result = dialog.handle_key(&space);

        assert!(matches!(result, Some(DialogResult::Dismissed)));
    }

    #[test]
    fn test_info_dialog_other_keys_ignored() {
        let mut dialog = DialogState::info("Info", "Some information");

        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(dialog.handle_key(&a).is_none());
    }

    // ==========================================================================
    // Accessor tests
    // ==========================================================================

    #[test]
    fn test_dialog_accessors() {
        let confirm = DialogState::confirm("Confirm Title", "Confirm message");
        assert_eq!(confirm.title(), "Confirm Title");
        assert_eq!(confirm.message(), "Confirm message");
        assert!(confirm.is_confirm());
        assert!(!confirm.is_select());
        assert!(!confirm.is_info());

        let select = DialogState::select("Select Title", vec!["A".into(), "B".into()]);
        assert_eq!(select.title(), "Select Title");
        assert_eq!(select.options().len(), 2);
        assert!(select.is_select());

        let info = DialogState::info("Info Title", "Info message");
        assert!(info.is_info());
    }
}
