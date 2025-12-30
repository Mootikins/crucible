//! Popup state management with event handling
//!
//! This module provides `PopupState`, a self-contained popup component that:
//! - Manages a list of items with fuzzy matching
//! - Handles navigation (Up/Down, Ctrl+P/N)
//! - Handles selection (Tab/Enter) and dismissal (Escape)
//! - Returns `EventResult` from the new event system

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::event_result::{EventResult, TuiAction};
use crate::tui::state::{PopupItem, PopupItemKind, PopupKind};

/// Provider abstraction for popup items
///
/// This trait allows different sources to feed items into the popup,
/// such as commands, agents, files, or notes.
pub trait PopupItemProvider: Send + Sync {
    /// Get items matching the query for the given popup kind
    fn provide(&self, kind: PopupKind, query: &str) -> Vec<PopupItem>;
}

/// Popup state with event handling
pub struct PopupState {
    /// The type of popup (Command or AgentOrFile)
    kind: PopupKind,
    /// Current query string for filtering
    query: String,
    /// Filtered items to display
    items: Vec<PopupItem>,
    /// Currently selected index
    selected: usize,
    /// Viewport offset for scrolling (for rendering)
    viewport_offset: usize,
    /// Provider for fetching items
    provider: Arc<dyn PopupItemProvider>,
}

impl std::fmt::Debug for PopupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PopupState")
            .field("kind", &self.kind)
            .field("query", &self.query)
            .field("items", &self.items)
            .field("selected", &self.selected)
            .field("viewport_offset", &self.viewport_offset)
            .field("provider", &"<dyn PopupItemProvider>")
            .finish()
    }
}

impl PopupState {
    /// Create a new popup state with the given kind and provider
    pub fn new(kind: PopupKind, provider: Arc<dyn PopupItemProvider>) -> Self {
        Self {
            kind,
            query: String::new(),
            items: Vec::new(),
            selected: 0,
            viewport_offset: 0,
            provider,
        }
    }

    /// Get the popup kind
    pub fn kind(&self) -> PopupKind {
        self.kind
    }

    /// Get the current query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get the current items
    pub fn items(&self) -> &[PopupItem] {
        &self.items
    }

    /// Get the currently selected index
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Get the currently selected item, if any
    pub fn selected_item(&self) -> Option<&PopupItem> {
        self.items.get(self.selected)
    }

    /// Get the viewport offset for rendering
    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    /// Update the query and refresh items from the provider
    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.items = self.provider.provide(self.kind, query);
        // Reset selection when query changes
        self.selected = 0;
        self.viewport_offset = 0;
    }

    /// Move selection up by one
    fn move_up(&mut self) -> EventResult {
        if self.selected > 0 {
            self.selected -= 1;
            self.adjust_viewport();
            EventResult::NeedsRender
        } else {
            EventResult::Handled
        }
    }

    /// Move selection down by one
    fn move_down(&mut self) -> EventResult {
        if !self.items.is_empty() && self.selected < self.items.len() - 1 {
            self.selected += 1;
            self.adjust_viewport();
            EventResult::NeedsRender
        } else {
            EventResult::Handled
        }
    }

    /// Adjust viewport to keep selection visible
    fn adjust_viewport(&mut self) {
        // Simple viewport adjustment - keep selection in view
        // Assuming a visible window of ~10 items (can be made configurable)
        const VISIBLE_ITEMS: usize = 10;

        if self.selected < self.viewport_offset {
            self.viewport_offset = self.selected;
        } else if self.selected >= self.viewport_offset + VISIBLE_ITEMS {
            self.viewport_offset = self.selected - VISIBLE_ITEMS + 1;
        }
    }

    /// Confirm the current selection
    fn confirm(&self) -> EventResult {
        if let Some(item) = self.selected_item() {
            EventResult::Action(TuiAction::PopupConfirm(item.clone()))
        } else {
            // No items to confirm, treat as close
            EventResult::Action(TuiAction::PopupClose)
        }
    }

    /// Handle a key event
    ///
    /// Returns an `EventResult` indicating what happened:
    /// - `Action(PopupConfirm)` when Tab/Enter is pressed with a selection
    /// - `Action(PopupClose)` when Escape is pressed
    /// - `NeedsRender` for navigation that changed selection
    /// - `Handled` for navigation that didn't change (at boundary)
    /// - `Ignored` for unhandled keys (like character input)
    pub fn handle_key(&mut self, key: &KeyEvent) -> EventResult {
        match (key.code, key.modifiers) {
            // Navigation: Up/Down arrows or Ctrl+P/N
            (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.move_up()
            }
            (KeyCode::Down, KeyModifiers::NONE) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.move_down()
            }

            // Selection: Tab or Enter confirms
            (KeyCode::Tab, KeyModifiers::NONE) | (KeyCode::Enter, KeyModifiers::NONE) => {
                self.confirm()
            }

            // Escape closes the popup
            (KeyCode::Esc, KeyModifiers::NONE) => EventResult::Action(TuiAction::PopupClose),

            // Character keys are ignored (passed through to input)
            (KeyCode::Char(_), KeyModifiers::NONE | KeyModifiers::SHIFT) => EventResult::Ignored,

            // Other keys are handled but don't do anything
            _ => EventResult::Handled,
        }
    }
}

// =============================================================================
// Mock provider for testing
// =============================================================================

/// A mock popup provider for testing
#[derive(Debug, Clone, Default)]
pub struct MockPopupProvider {
    items: Vec<PopupItem>,
}

impl MockPopupProvider {
    /// Create an empty mock provider
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a mock provider with predefined items
    pub fn with_items(items: Vec<PopupItem>) -> Self {
        Self { items }
    }
}

impl PopupItemProvider for MockPopupProvider {
    fn provide(&self, _kind: PopupKind, _query: &str) -> Vec<PopupItem> {
        // For testing, always return all items regardless of query
        self.items.clone()
    }
}

#[cfg(test)]
mod tests {
    //! Tests for PopupState

    /// Helper to create a command PopupItem for testing
    fn make_command(name: &str, description: &str) -> PopupItem {
        PopupItem {
            kind: PopupItemKind::Command,
            title: format!("/{}", name),
            subtitle: description.to_string(),
            token: format!("/{} ", name),
            score: 100,
            available: true,
        }
    }
    use super::*;

    // ==========================================================================
    // Creation and query tests
    // ==========================================================================

    #[test]
    fn test_popup_state_new() {
        let provider = Arc::new(MockPopupProvider::new());
        let state = PopupState::new(PopupKind::Command, provider);

        assert_eq!(state.items().len(), 0);
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn test_popup_state_update_query() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![
            make_command("help", "Show help"),
            make_command("hello", "Say hello"),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);

        state.update_query("hel");
        assert_eq!(state.items().len(), 2); // Mock returns all items
        assert_eq!(state.selected_index(), 0); // Reset on query change
    }

    #[test]
    fn test_popup_state_query_resets_selection() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![
            make_command("a", ""),
            make_command("b", ""),
            make_command("c", ""),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        // Move selection down
        state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.selected_index(), 2);

        // Update query resets selection
        state.update_query("new");
        assert_eq!(state.selected_index(), 0);
    }

    // ==========================================================================
    // Navigation tests
    // ==========================================================================

    #[test]
    fn test_popup_navigation_down() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![
            make_command("a", ""),
            make_command("b", ""),
            make_command("c", ""),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        assert_eq!(state.selected_index(), 0);

        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(state.handle_key(&down), EventResult::NeedsRender);
        assert_eq!(state.selected_index(), 1);

        state.handle_key(&down);
        assert_eq!(state.selected_index(), 2);

        // At end, stays at 2
        assert_eq!(state.handle_key(&down), EventResult::Handled);
        assert_eq!(state.selected_index(), 2);
    }

    #[test]
    fn test_popup_navigation_up() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![
            make_command("a", ""),
            make_command("b", ""),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");
        state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(state.handle_key(&up), EventResult::NeedsRender);
        assert_eq!(state.selected_index(), 0);

        // At start, stays at 0
        assert_eq!(state.handle_key(&up), EventResult::Handled);
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn test_popup_ctrl_n_ctrl_p() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![
            make_command("a", ""),
            make_command("b", ""),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        let ctrl_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
        let ctrl_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);

        state.handle_key(&ctrl_n);
        assert_eq!(state.selected_index(), 1);

        state.handle_key(&ctrl_p);
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn test_popup_navigation_empty_list() {
        let provider = Arc::new(MockPopupProvider::new());
        let mut state = PopupState::new(PopupKind::Command, provider);

        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);

        // Navigation on empty list should be handled but not change anything
        assert_eq!(state.handle_key(&down), EventResult::Handled);
        assert_eq!(state.handle_key(&up), EventResult::Handled);
        assert_eq!(state.selected_index(), 0);
    }

    // ==========================================================================
    // Selection confirmation tests
    // ==========================================================================

    #[test]
    fn test_popup_tab_confirms() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![make_command(
            "help",
            "Show help",
        )]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let result = state.handle_key(&tab);

        assert!(matches!(
            result,
            EventResult::Action(TuiAction::PopupConfirm(_))
        ));
    }

    #[test]
    fn test_popup_enter_confirms() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![make_command(
            "help",
            "Show help",
        )]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = state.handle_key(&enter);

        assert!(matches!(
            result,
            EventResult::Action(TuiAction::PopupConfirm(_))
        ));
    }

    #[test]
    fn test_popup_confirm_returns_selected_item() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![
            make_command("first", "First command"),
            make_command("second", "Second command"),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        // Select second item
        state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let result = state.handle_key(&tab);

        if let EventResult::Action(TuiAction::PopupConfirm(item)) = result {
            assert!(item.title.contains("second"));
        } else {
            panic!("Expected PopupConfirm action");
        }
    }

    #[test]
    fn test_popup_confirm_empty_closes() {
        let provider = Arc::new(MockPopupProvider::new());
        let mut state = PopupState::new(PopupKind::Command, provider);

        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let result = state.handle_key(&tab);

        assert!(matches!(result, EventResult::Action(TuiAction::PopupClose)));
    }

    // ==========================================================================
    // Escape and dismissal tests
    // ==========================================================================

    #[test]
    fn test_popup_escape_closes() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![]));
        let mut state = PopupState::new(PopupKind::Command, provider);

        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = state.handle_key(&esc);

        assert!(matches!(result, EventResult::Action(TuiAction::PopupClose)));
    }

    // ==========================================================================
    // Key passthrough tests
    // ==========================================================================

    #[test]
    fn test_popup_char_keys_ignored() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![]));
        let mut state = PopupState::new(PopupKind::Command, provider);

        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(state.handle_key(&a), EventResult::Ignored);
    }

    #[test]
    fn test_popup_shift_char_keys_ignored() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![]));
        let mut state = PopupState::new(PopupKind::Command, provider);

        let shift_a = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
        assert_eq!(state.handle_key(&shift_a), EventResult::Ignored);
    }

    #[test]
    fn test_popup_backspace_handled() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![]));
        let mut state = PopupState::new(PopupKind::Command, provider);

        let backspace = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        // Backspace is handled (consumed) but doesn't change popup state
        // The input component will handle the actual text deletion
        assert_eq!(state.handle_key(&backspace), EventResult::Handled);
    }

    // ==========================================================================
    // Accessor tests
    // ==========================================================================

    #[test]
    fn test_popup_accessors() {
        let provider = Arc::new(MockPopupProvider::with_items(vec![make_command(
            "test", "desc",
        )]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("query");

        assert_eq!(state.kind(), PopupKind::Command);
        assert_eq!(state.query(), "query");
        assert_eq!(state.viewport_offset(), 0);
        assert!(state.selected_item().is_some());
    }
}
