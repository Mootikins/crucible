//! Unified ChatView - single source of truth for TUI state
//!
//! This module provides `ChatView`, which:
//! - Composes all UI state (input, popup, dialog)
//! - Routes events by priority (Dialog > Popup > Input)
//! - Handles global shortcuts (Ctrl+C, Ctrl+D, Escape)
//! - Auto-triggers popups on / and @

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::tui::components::dialog_state::{DialogResult, DialogState};
use crate::tui::components::generic_popup::PopupState;
use crate::tui::components::input_state::InputState;
use crate::tui::event_result::{EventResult, TuiAction};
use crate::tui::popup::PopupProvider;
use crate::tui::state::PopupKind;

/// Time window for double Ctrl+C to trigger exit
const DOUBLE_CTRL_C_WINDOW: Duration = Duration::from_millis(500);

/// Unified view state for the chat TUI
pub struct ChatView<'a> {
    /// Current mode ID (plan, act, auto)
    pub mode_id: String,

    /// Input state (text editor with history)
    pub input: InputState<'a>,

    /// Active popup, if any
    popup: Option<PopupState>,

    /// Active dialog, if any
    pub dialog: Option<DialogState>,

    /// Provider for popup items
    popup_provider: Option<Arc<dyn PopupProvider>>,

    /// Last Ctrl+C timestamp for double-tap detection
    last_ctrl_c: Option<Instant>,

    /// Error message to display in status
    pub status_error: Option<String>,
}

impl<'a> ChatView<'a> {
    /// Create a new ChatView with the given mode
    pub fn new(mode_id: impl Into<String>) -> Self {
        Self {
            mode_id: mode_id.into(),
            input: InputState::new(),
            popup: None,
            dialog: None,
            popup_provider: None,
            last_ctrl_c: None,
            status_error: None,
        }
    }

    /// Set the popup provider for auto-triggering popups
    pub fn with_popup_provider(mut self, provider: Arc<dyn PopupProvider>) -> Self {
        self.popup_provider = Some(provider);
        self
    }

    /// Set the popup provider
    pub fn set_popup_provider(&mut self, provider: Arc<dyn PopupProvider>) {
        self.popup_provider = Some(provider);
    }

    /// Handle an event with priority-based routing
    ///
    /// Priority order:
    /// 1. Global shortcuts (Ctrl+D always exits)
    /// 2. Dialog (if present, captures all events)
    /// 3. Popup (if present, captures navigation, passes chars to input)
    /// 4. Input (default handler)
    pub fn handle_event(&mut self, event: &Event) -> EventResult {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };

        // 1. Global shortcuts (highest priority)
        if let Some(result) = self.handle_global_shortcut(key) {
            return result;
        }

        // 2. Dialog captures all events
        if self.dialog.is_some() {
            return self.handle_dialog_event(key);
        }

        // 3. Popup captures navigation, passes chars through
        if self.popup.is_some() {
            let popup_result = self.handle_popup_event(key);
            if popup_result != EventResult::Ignored {
                return popup_result;
            }
            // Fall through to input for ignored keys (like chars)
        }

        // 4. Input handles the rest
        self.handle_input_event(key)
    }

    /// Handle global shortcuts that work regardless of focus
    fn handle_global_shortcut(&mut self, key: &KeyEvent) -> Option<EventResult> {
        match (key.code, key.modifiers) {
            // Ctrl+D always exits
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                Some(EventResult::Action(TuiAction::Exit))
            }

            // Ctrl+C: first cancels, double-tap exits
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                let now = Instant::now();
                if let Some(last) = self.last_ctrl_c {
                    if now.duration_since(last) < DOUBLE_CTRL_C_WINDOW {
                        self.last_ctrl_c = None;
                        return Some(EventResult::Action(TuiAction::Exit));
                    }
                }
                self.last_ctrl_c = Some(now);
                Some(EventResult::Action(TuiAction::Cancel))
            }

            // Escape: close popup if present, otherwise cancel
            (KeyCode::Esc, KeyModifiers::NONE) => {
                if self.popup.is_some() {
                    self.popup = None;
                    Some(EventResult::NeedsRender)
                } else if self.dialog.is_none() {
                    // Only cancel if no dialog (dialog handles its own Esc)
                    Some(EventResult::Action(TuiAction::Cancel))
                } else {
                    None // Let dialog handle it
                }
            }

            _ => None,
        }
    }

    /// Handle events when dialog is active
    fn handle_dialog_event(&mut self, key: &KeyEvent) -> EventResult {
        let dialog = self.dialog.as_mut().unwrap();

        if let Some(result) = dialog.handle_key(key) {
            // Dialog produced a result, close it and return action
            self.dialog = None;

            match result {
                DialogResult::Confirmed => EventResult::Action(TuiAction::DialogConfirm),
                DialogResult::Cancelled => EventResult::Action(TuiAction::DialogCancel),
                DialogResult::Selected(idx) => EventResult::Action(TuiAction::DialogSelect(idx)),
                DialogResult::Dismissed => EventResult::Action(TuiAction::DialogDismiss),
            }
        } else {
            // Dialog handled but didn't close
            EventResult::Handled
        }
    }

    /// Handle events when popup is active
    fn handle_popup_event(&mut self, key: &KeyEvent) -> EventResult {
        let popup = self.popup.as_mut().unwrap();
        let result = popup.handle_key(key);

        match &result {
            EventResult::Action(TuiAction::PopupClose) => {
                self.popup = None;
                EventResult::NeedsRender
            }
            EventResult::Action(TuiAction::PopupConfirm(_)) => {
                // Keep popup for now, let runner handle the action
                result
            }
            _ => result,
        }
    }

    /// Handle events for input
    fn handle_input_event(&mut self, key: &KeyEvent) -> EventResult {
        // Check for popup triggers before passing to input
        if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
            if let KeyCode::Char(c) = key.code {
                // First, let input handle the character
                let input_result = self.input.handle_key(key);

                // Then check if we should trigger a popup
                self.check_popup_trigger(c);

                // Update existing popup based on new input content
                // (closes popup if query contains whitespace)
                self.update_popup_from_input();

                return input_result;
            }
        }

        let result = self.input.handle_key(key);

        // Update popup after any input change (backspace, etc.)
        self.update_popup_from_input();

        result
    }

    /// Check if a character should trigger a popup
    fn check_popup_trigger(&mut self, c: char) {
        let Some(provider) = &self.popup_provider else {
            return;
        };

        let content = self.input.content();

        // Check for / at start of input or after whitespace
        if c == '/' && (content == "/" || content.ends_with(" /")) {
            let mut popup = PopupState::new(PopupKind::Command, Arc::clone(provider));
            popup.update_query("");
            self.popup = Some(popup);
        }
        // Check for @ at start of input or after whitespace
        else if c == '@' && (content == "@" || content.ends_with(" @")) {
            let mut popup = PopupState::new(PopupKind::AgentOrFile, Arc::clone(provider));
            popup.update_query("");
            self.popup = Some(popup);
        }
    }

    /// Update popup state based on current input content
    ///
    /// Call this after modifying input directly (not via handle_event)
    pub fn update_popup_from_input(&mut self) {
        if self.popup.is_none() {
            return;
        }

        let content = self.input.content();

        // Find the trigger and extract query
        let popup_kind = self.popup.as_ref().unwrap().kind();

        let (trigger, query) = match popup_kind {
            PopupKind::Command => {
                if let Some(pos) = content.rfind('/') {
                    ('/', &content[pos + 1..])
                } else {
                    // No trigger found, close popup
                    self.popup = None;
                    return;
                }
            }
            PopupKind::AgentOrFile => {
                if let Some(pos) = content.rfind('@') {
                    ('@', &content[pos + 1..])
                } else {
                    // No trigger found, close popup
                    self.popup = None;
                    return;
                }
            }
            PopupKind::ReplCommand => {
                if let Some(pos) = content.rfind(':') {
                    (':', &content[pos + 1..])
                } else {
                    // No trigger found, close popup
                    self.popup = None;
                    return;
                }
            }
        };

        // Verify trigger is at start or after whitespace
        let trigger_pos = content.rfind(trigger).unwrap();
        if trigger_pos > 0 {
            let before = content.chars().nth(trigger_pos - 1);
            if before != Some(' ') && before != Some('\n') {
                self.popup = None;
                return;
            }
        }

        // Update query first to get filtered results
        if let Some(popup) = &mut self.popup {
            // Extract the part before any trailing space
            let query_trimmed = query.trim_end();
            popup.update_query(query_trimmed);

            // Close popup if:
            // 1. Query ends with space (user finished typing the token)
            // 2. AND either no matches OR there's an exact match
            if query.ends_with(' ') || query.ends_with('\n') {
                // Token is complete - close popup
                self.popup = None;
            }
        }
    }

    /// Close the current popup
    pub fn close_popup(&mut self) {
        self.popup = None;
    }

    /// Show a dialog
    pub fn show_dialog(&mut self, dialog: DialogState) {
        self.dialog = Some(dialog);
    }

    /// Close the current dialog
    pub fn close_dialog(&mut self) {
        self.dialog = None;
    }

    /// Check if a popup is active
    pub fn has_popup(&self) -> bool {
        self.popup.is_some()
    }

    /// Set the popup
    pub fn set_popup(&mut self, popup: Option<PopupState>) {
        self.popup = popup;
    }

    /// Get a reference to the popup
    pub fn popup(&self) -> Option<&PopupState> {
        self.popup.as_ref()
    }

    /// Get a mutable reference to the popup
    pub fn popup_mut(&mut self) -> Option<&mut PopupState> {
        self.popup.as_mut()
    }

    /// Check if a dialog is active
    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some()
    }

    /// Clear the status error
    pub fn clear_error(&mut self) {
        self.status_error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::PopupItem;

    /// Mock popup provider for tests
    #[derive(Debug, Clone, Default)]
    struct MockProvider {
        items: Vec<PopupItem>,
    }

    impl MockProvider {
        fn with_items(items: Vec<PopupItem>) -> Self {
            Self { items }
        }
    }

    impl PopupProvider for MockProvider {
        fn provide(&self, _kind: PopupKind, _query: &str) -> Vec<PopupItem> {
            self.items.clone()
        }
    }

    fn mock_provider() -> Arc<dyn PopupProvider> {
        Arc::new(MockProvider::with_items(vec![
            PopupItem::cmd("help").desc("Show help").with_score(100),
            PopupItem::cmd("exit").desc("Exit").with_score(100),
        ]))
    }

    // ==========================================================================
    // Event priority - Dialog blocks everything
    // ==========================================================================

    #[test]
    fn test_dialog_captures_all_events() {
        let mut view = ChatView::new("plan");
        view.dialog = Some(DialogState::confirm("Test", "Message"));

        // Regular keys go to dialog, not input
        let a = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        view.handle_event(&a);

        assert!(view.input.is_empty()); // Input didn't receive it
    }

    #[test]
    fn test_dialog_y_closes_and_returns_action() {
        let mut view = ChatView::new("plan");
        view.dialog = Some(DialogState::confirm("Test", "Message"));

        let y = Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        let result = view.handle_event(&y);

        assert!(matches!(
            result,
            EventResult::Action(TuiAction::DialogConfirm)
        ));
        assert!(view.dialog.is_none()); // Dialog closed
    }

    #[test]
    fn test_dialog_n_closes_and_returns_cancel() {
        let mut view = ChatView::new("plan");
        view.dialog = Some(DialogState::confirm("Test", "Message"));

        let n = Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        let result = view.handle_event(&n);

        assert!(matches!(
            result,
            EventResult::Action(TuiAction::DialogCancel)
        ));
        assert!(view.dialog.is_none());
    }

    #[test]
    fn test_dialog_select_returns_index() {
        let mut view = ChatView::new("plan");
        view.dialog = Some(DialogState::select("Choose", vec!["A".into(), "B".into()]));

        // Move down and select
        view.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));
        let result = view.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert!(matches!(
            result,
            EventResult::Action(TuiAction::DialogSelect(1))
        ));
    }

    // ==========================================================================
    // Event priority - Popup captures navigation
    // ==========================================================================

    #[test]
    fn test_popup_captures_navigation_keys() {
        let mut view = ChatView::new("plan");
        view.popup = Some(PopupState::new(PopupKind::Command, mock_provider()));
        view.popup.as_mut().unwrap().update_query("");

        let down = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let result = view.handle_event(&down);

        assert_eq!(result, EventResult::NeedsRender);
        // Popup received it, not input history
    }

    #[test]
    fn test_popup_passes_char_keys_to_input() {
        let mut view = ChatView::new("plan").with_popup_provider(mock_provider());
        view.popup = Some(PopupState::new(PopupKind::Command, mock_provider()));

        let a = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        view.handle_event(&a);

        // Input should have received the character
        assert!(view.input.content().contains('a'));
    }

    #[test]
    fn test_popup_tab_confirms() {
        let mut view = ChatView::new("plan");
        view.popup = Some(PopupState::new(PopupKind::Command, mock_provider()));
        view.popup.as_mut().unwrap().update_query("");

        let tab = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let result = view.handle_event(&tab);

        assert!(matches!(
            result,
            EventResult::Action(TuiAction::PopupConfirm(_))
        ));
    }

    // ==========================================================================
    // Global shortcuts
    // ==========================================================================

    #[test]
    fn test_ctrl_d_always_exits() {
        let mut view = ChatView::new("plan");

        let ctrl_d = Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        let result = view.handle_event(&ctrl_d);

        assert!(matches!(result, EventResult::Action(TuiAction::Exit)));
    }

    #[test]
    fn test_ctrl_d_exits_even_with_dialog() {
        let mut view = ChatView::new("plan");
        view.dialog = Some(DialogState::confirm("Test", "Message"));

        let ctrl_d = Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        let result = view.handle_event(&ctrl_d);

        assert!(matches!(result, EventResult::Action(TuiAction::Exit)));
    }

    #[test]
    fn test_single_ctrl_c_cancels() {
        let mut view = ChatView::new("plan");

        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let result = view.handle_event(&ctrl_c);

        assert!(matches!(result, EventResult::Action(TuiAction::Cancel)));
    }

    #[test]
    fn test_double_ctrl_c_exits() {
        let mut view = ChatView::new("plan");

        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        // First Ctrl+C cancels
        let result1 = view.handle_event(&ctrl_c);
        assert!(matches!(result1, EventResult::Action(TuiAction::Cancel)));

        // Second Ctrl+C (within window) exits
        let result2 = view.handle_event(&ctrl_c);
        assert!(matches!(result2, EventResult::Action(TuiAction::Exit)));
    }

    #[test]
    fn test_escape_closes_popup() {
        let mut view = ChatView::new("plan");
        view.popup = Some(PopupState::new(PopupKind::Command, mock_provider()));

        let esc = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let result = view.handle_event(&esc);

        assert_eq!(result, EventResult::NeedsRender);
        assert!(view.popup.is_none());
    }

    #[test]
    fn test_escape_without_popup_cancels() {
        let mut view = ChatView::new("plan");

        let esc = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let result = view.handle_event(&esc);

        assert!(matches!(result, EventResult::Action(TuiAction::Cancel)));
    }

    // ==========================================================================
    // Popup auto-trigger from input
    // ==========================================================================

    #[test]
    fn test_slash_triggers_command_popup() {
        let mut view = ChatView::new("plan").with_popup_provider(mock_provider());

        let slash = Event::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        view.handle_event(&slash);

        assert!(view.popup.is_some());
        assert_eq!(view.popup.as_ref().unwrap().kind(), PopupKind::Command);
    }

    #[test]
    fn test_at_triggers_agent_popup() {
        let mut view = ChatView::new("plan").with_popup_provider(mock_provider());

        let at = Event::Key(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE));
        view.handle_event(&at);

        assert!(view.popup.is_some());
        assert_eq!(view.popup.as_ref().unwrap().kind(), PopupKind::AgentOrFile);
    }

    #[test]
    fn test_slash_after_space_triggers_popup() {
        let mut view = ChatView::new("plan").with_popup_provider(mock_provider());

        // Type "run "
        for c in "run ".chars() {
            view.handle_event(&Event::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE,
            )));
        }
        assert!(view.popup.is_none());

        // Type "/" after space
        view.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Char('/'),
            KeyModifiers::NONE,
        )));
        assert!(view.popup.is_some());
    }

    #[test]
    fn test_slash_mid_word_no_popup() {
        let mut view = ChatView::new("plan").with_popup_provider(mock_provider());

        // Type "path" then "/" (no space before)
        for c in "path".chars() {
            view.handle_event(&Event::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE,
            )));
        }
        view.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Char('/'),
            KeyModifiers::NONE,
        )));

        // Should NOT trigger popup (/ is mid-word)
        assert!(view.popup.is_none());
    }

    #[test]
    fn test_clearing_trigger_closes_popup() {
        let mut view = ChatView::new("plan").with_popup_provider(mock_provider());

        view.input.set_content("/help");
        view.popup = Some(PopupState::new(PopupKind::Command, mock_provider()));

        // Clear input
        view.input.clear();
        view.update_popup_from_input();

        assert!(view.popup.is_none());
    }

    #[test]
    fn test_space_after_command_closes_popup() {
        let mut view = ChatView::new("plan").with_popup_provider(mock_provider());

        // Type "/" to open popup
        view.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Char('/'),
            KeyModifiers::NONE,
        )));
        assert!(view.popup.is_some(), "Popup should open on /");

        // Type "help"
        for c in "help".chars() {
            view.handle_event(&Event::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE,
            )));
        }
        assert!(view.popup.is_some(), "Popup should stay open while typing");

        // Type space - should close popup (command is complete)
        view.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        assert!(
            view.popup.is_none(),
            "Popup should close when space typed after complete token"
        );
    }

    #[test]
    fn test_no_provider_no_popup() {
        let mut view = ChatView::new("plan");
        // No provider set

        let slash = Event::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        view.handle_event(&slash);

        assert!(view.popup.is_none());
    }

    // ==========================================================================
    // Input passthrough
    // ==========================================================================

    #[test]
    fn test_typing_without_popup() {
        let mut view = ChatView::new("plan");

        view.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        )));
        view.handle_event(&Event::Key(KeyEvent::new(
            KeyCode::Char('i'),
            KeyModifiers::NONE,
        )));

        assert_eq!(view.input.content(), "hi");
    }

    #[test]
    fn test_enter_sends_message() {
        let mut view = ChatView::new("plan");
        view.input.set_content("hello");

        let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let result = view.handle_event(&enter);

        assert!(matches!(
            result,
            EventResult::Action(TuiAction::SendMessage(s)) if s == "hello"
        ));
    }

    // ==========================================================================
    // Helper methods
    // ==========================================================================

    #[test]
    fn test_has_popup_and_dialog() {
        let mut view = ChatView::new("plan");

        assert!(!view.has_popup());
        assert!(!view.has_dialog());

        view.popup = Some(PopupState::new(PopupKind::Command, mock_provider()));
        assert!(view.has_popup());

        view.show_dialog(DialogState::info("Test", "Message"));
        assert!(view.has_dialog());

        view.close_popup();
        assert!(!view.has_popup());

        view.close_dialog();
        assert!(!view.has_dialog());
    }

    #[test]
    fn test_clear_error() {
        let mut view = ChatView::new("plan");
        view.status_error = Some("Error".into());

        view.clear_error();
        assert!(view.status_error.is_none());
    }

    // ==========================================================================
    // PopupState integration tests
    // ==========================================================================

    #[test]
    fn test_generic_popup_can_be_used() {
        use crate::tui::components::PopupState;

        let mut view = ChatView::new("plan");

        // Should be able to create a PopupState from the same provider
        let generic = PopupState::new(PopupKind::Command, mock_provider());

        // Should be able to set it as the popup
        view.set_popup(Some(generic));

        assert!(view.has_popup());
    }

    #[test]
    fn test_generic_popup_fuzzy_filtering() {
        use crate::tui::components::PopupState;

        let mut view = ChatView::new("plan");
        let mut generic = PopupState::new(PopupKind::Command, mock_provider());
        generic.update_query("");

        // Set filter query for fuzzy filtering within items
        generic.set_filter_query("hel");

        view.set_popup(Some(generic));

        // The generic popup should have filtered results
        let popup = view.popup().unwrap();
        assert!(popup.filtered_count() <= 2);
    }

    #[test]
    fn test_generic_popup_navigation() {
        use crate::tui::components::PopupState;

        let mut view = ChatView::new("plan");
        let mut generic = PopupState::new(PopupKind::Command, mock_provider());
        generic.update_query("");

        view.set_popup(Some(generic));

        // Navigation should work through handle_event
        let down = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let result = view.handle_event(&down);

        assert_eq!(result, EventResult::NeedsRender);
        assert_eq!(view.popup().unwrap().selected_index(), 1);
    }

    #[test]
    fn test_generic_popup_confirm_returns_action() {
        use crate::tui::components::PopupState;

        let mut view = ChatView::new("plan");
        let mut generic = PopupState::new(PopupKind::Command, mock_provider());
        generic.update_query("");

        view.set_popup(Some(generic));

        let tab = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let result = view.handle_event(&tab);

        assert!(matches!(
            result,
            EventResult::Action(TuiAction::PopupConfirm(_))
        ));
    }
}
