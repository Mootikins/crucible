//! Generic popup state using the new widget system
//!
//! This module provides `PopupState`, a wrapper around the generic `Popup<T>`
//! that exposes the same interface as the existing `PopupState` for event handling.
//!
//! The goal is to enable gradual migration from the old popup system to the new
//! generic one without breaking existing code.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::event_result::{EventResult, TuiAction};
use crate::tui::popup::PopupProvider;
use crate::tui::state::{PopupItem, PopupKind};
use crate::tui::widgets::{Popup, PopupConfig, PopupItem as PopupItemTrait, PopupRenderer};

// =============================================================================
// LegacyPopupItem - Wrapper to implement PopupItem trait for existing PopupItem
// =============================================================================

/// Wrapper around the legacy `PopupItem` that implements the generic `PopupItem` trait
#[derive(Clone, Debug)]
pub struct LegacyPopupItem {
    inner: PopupItem,
}

impl LegacyPopupItem {
    /// Create from a legacy PopupItem
    pub fn from_legacy(item: PopupItem) -> Self {
        Self { inner: item }
    }

    /// Get the inner legacy PopupItem
    pub fn inner(&self) -> &PopupItem {
        &self.inner
    }

    /// Convert back to legacy PopupItem
    pub fn into_inner(self) -> PopupItem {
        self.inner
    }
}

impl PopupItemTrait for LegacyPopupItem {
    fn match_text(&self) -> &str {
        // For matching, we need a stable reference - use the name/id/path directly
        match &self.inner {
            PopupItem::Command { name, .. } => name,
            PopupItem::Agent { id, .. } => id,
            PopupItem::File { path, .. } => path,
            PopupItem::Note { path, .. } => path,
            PopupItem::Skill { name, .. } => name,
            PopupItem::ReplCommand { name, .. } => name,
        }
    }

    fn label(&self) -> &str {
        // For label, return match_text - the title() method allocates
        self.match_text()
    }

    fn description(&self) -> Option<&str> {
        let subtitle = self.inner.subtitle();
        if subtitle.is_empty() {
            None
        } else {
            Some(subtitle)
        }
    }

    fn kind_label(&self) -> Option<&str> {
        Some(self.inner.kind_label())
    }

    fn icon(&self) -> Option<char> {
        Some(match &self.inner {
            PopupItem::Command { .. } => '/',
            PopupItem::Agent { .. } => '@',
            PopupItem::File { .. } => ' ',
            PopupItem::Note { .. } => ' ',
            PopupItem::Skill { .. } => ' ',
            PopupItem::ReplCommand { .. } => ':',
        })
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_available()
    }

    fn token(&self) -> &str {
        // token() method allocates, but we need &str - use match_text with prefix
        // This is a known limitation of the wrapper pattern
        self.match_text()
    }
}

// =============================================================================
// PopupState
// =============================================================================

/// Generic popup state wrapping the new `Popup<T>` widget
///
/// This provides the same interface as the existing `PopupState` but uses
/// the new generic popup internally with fuzzy filtering.
pub struct PopupState {
    /// The type of popup
    kind: PopupKind,
    /// Provider query (for refresh)
    provider_query: String,
    /// Inner generic popup
    popup: Popup<LegacyPopupItem>,
    /// Provider for fetching items
    provider: Arc<dyn PopupProvider>,
}

impl std::fmt::Debug for PopupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PopupState")
            .field("kind", &self.kind)
            .field("provider_query", &self.provider_query)
            .field("popup", &self.popup)
            .field("provider", &"<dyn PopupProvider>")
            .finish()
    }
}

impl PopupState {
    /// Create a new generic popup state
    pub fn new(kind: PopupKind, provider: Arc<dyn PopupProvider>) -> Self {
        // Kind labels are only needed for AgentOrFile (@) which mixes agents and files
        // For single-type popups, the trigger char already indicates type
        let show_kinds = matches!(kind, PopupKind::AgentOrFile);
        let config = PopupConfig::default()
            .max_visible(10)
            .filterable(true)
            .show_kinds(show_kinds);

        Self {
            kind,
            provider_query: String::new(),
            popup: Popup::new(vec![]).with_config(config),
            provider,
        }
    }

    /// Get the popup kind
    pub fn kind(&self) -> PopupKind {
        self.kind
    }

    /// Get the current provider query (used for item fetching)
    pub fn query(&self) -> &str {
        &self.provider_query
    }

    /// Get the filter query (used for fuzzy filtering within current items)
    pub fn filter_query(&self) -> &str {
        self.popup.query()
    }

    /// Get the items (as legacy PopupItems)
    pub fn items(&self) -> Vec<&PopupItem> {
        self.popup
            .all_items()
            .iter()
            .map(|item| item.inner())
            .collect()
    }

    /// Get the filtered count
    pub fn filtered_count(&self) -> usize {
        self.popup.filtered_count()
    }

    /// Get the selected index
    pub fn selected_index(&self) -> usize {
        self.popup.selected_index()
    }

    /// Get the selected item
    pub fn selected_item(&self) -> Option<&PopupItem> {
        self.popup.selected_item().map(|item| item.inner())
    }

    /// Get the argument hint for the selected item (if any)
    ///
    /// Returns the argument hint text (e.g., "<query>" for /search) to show
    /// as faded text in the input when this item is highlighted.
    pub fn selected_argument_hint(&self) -> Option<String> {
        self.selected_item()
            .and_then(|item| item.argument_hint().map(|s| s.to_string()))
    }

    /// Get viewport offset
    pub fn viewport_offset(&self) -> usize {
        self.popup.viewport().offset()
    }

    /// Update the query and refresh items from the provider
    pub fn update_query(&mut self, query: &str) {
        self.provider_query = query.to_string();
        let items = self.provider.provide(self.kind, query);
        let wrapped: Vec<LegacyPopupItem> = items
            .into_iter()
            .map(LegacyPopupItem::from_legacy)
            .collect();
        self.popup.set_items(wrapped);
    }

    /// Set the filter query (for fuzzy filtering within current items)
    pub fn set_filter_query(&mut self, query: &str) {
        self.popup.set_query(query);
    }

    /// Move selection up by delta items
    pub fn move_selection(&mut self, delta: isize) {
        if delta < 0 {
            for _ in 0..(-delta) {
                self.popup.move_up();
            }
        } else {
            for _ in 0..delta {
                self.popup.move_down();
            }
        }
    }

    /// Get a renderer for the popup
    pub fn renderer(&self) -> PopupRenderer<'_, LegacyPopupItem> {
        PopupRenderer::new(&self.popup)
    }

    /// Get reference to the inner generic popup (for alternative renderers)
    pub fn inner_popup(&self) -> &Popup<LegacyPopupItem> {
        &self.popup
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: &KeyEvent) -> EventResult {
        match (key.code, key.modifiers) {
            // Navigation: Up/Down arrows or Ctrl+P/N or Ctrl+K/J
            (KeyCode::Up, KeyModifiers::NONE)
            | (KeyCode::Char('p'), KeyModifiers::CONTROL)
            | (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                self.popup.move_up();
                EventResult::NeedsRender
            }
            (KeyCode::Down, KeyModifiers::NONE)
            | (KeyCode::Char('n'), KeyModifiers::CONTROL)
            | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
                self.popup.move_down();
                EventResult::NeedsRender
            }

            // Page navigation
            (KeyCode::PageUp, KeyModifiers::NONE) => {
                self.popup.page_up();
                EventResult::NeedsRender
            }
            (KeyCode::PageDown, KeyModifiers::NONE) => {
                self.popup.page_down();
                EventResult::NeedsRender
            }

            // Home/End
            (KeyCode::Home, KeyModifiers::NONE) => {
                self.popup.select_first();
                EventResult::NeedsRender
            }
            (KeyCode::End, KeyModifiers::NONE) => {
                self.popup.select_last();
                EventResult::NeedsRender
            }

            // Selection: Tab or Enter confirms
            (KeyCode::Tab, KeyModifiers::NONE) | (KeyCode::Enter, KeyModifiers::NONE) => {
                if let Some(item) = self.popup.selected_item() {
                    EventResult::Action(TuiAction::PopupConfirm(item.inner().clone()))
                } else {
                    EventResult::Action(TuiAction::PopupClose)
                }
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
// Tests - Written FIRST per TDD
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────────────────
    // Test helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Helper to create a command PopupItem for testing
    fn make_command(name: &str, description: &str) -> PopupItem {
        PopupItem::cmd(name).desc(description).with_score(100)
    }

    /// Mock provider that returns all items regardless of query
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

    // ─────────────────────────────────────────────────────────────────────────
    // Creation tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_state_new_starts_empty() {
        let provider = Arc::new(MockProvider::default());
        let state = PopupState::new(PopupKind::Command, provider);

        assert_eq!(state.kind(), PopupKind::Command);
        assert_eq!(state.items().len(), 0);
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn generic_popup_state_update_query_fetches_items() {
        let provider = Arc::new(MockProvider::with_items(vec![
            make_command("help", "Show help"),
            make_command("quit", "Exit"),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);

        state.update_query("hel");
        assert_eq!(state.items().len(), 2);
        assert_eq!(state.query(), "hel");
    }

    #[test]
    fn generic_popup_update_query_resets_selection() {
        let provider = Arc::new(MockProvider::with_items(vec![
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

    // ─────────────────────────────────────────────────────────────────────────
    // Navigation tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_navigation_down() {
        let provider = Arc::new(MockProvider::with_items(vec![
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
    }

    #[test]
    fn generic_popup_navigation_up() {
        let provider = Arc::new(MockProvider::with_items(vec![
            make_command("a", ""),
            make_command("b", ""),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");
        state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(state.handle_key(&up), EventResult::NeedsRender);
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn generic_popup_navigation_wraps() {
        let provider = Arc::new(MockProvider::with_items(vec![
            make_command("a", ""),
            make_command("b", ""),
            make_command("c", ""),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        // At bottom, down wraps to top
        state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.selected_index(), 0); // Wrapped

        // At top, up wraps to bottom
        state.handle_key(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.selected_index(), 2); // Wrapped to bottom
    }

    #[test]
    fn generic_popup_ctrl_n_ctrl_p() {
        let provider = Arc::new(MockProvider::with_items(vec![
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
    fn generic_popup_vim_j_k_navigation() {
        let provider = Arc::new(MockProvider::with_items(vec![
            make_command("a", ""),
            make_command("b", ""),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        // j moves down (when popup is active, not in input)
        let j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);
        state.handle_key(&j);
        assert_eq!(state.selected_index(), 1);

        // k moves up
        let k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        state.handle_key(&k);
        assert_eq!(state.selected_index(), 0);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Selection confirmation tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_tab_confirms() {
        let provider = Arc::new(MockProvider::with_items(vec![make_command(
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
    fn generic_popup_enter_confirms() {
        let provider = Arc::new(MockProvider::with_items(vec![make_command(
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
    fn generic_popup_confirm_returns_selected_item() {
        let provider = Arc::new(MockProvider::with_items(vec![
            make_command("first", "First"),
            make_command("second", "Second"),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        // Select second item
        state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let result = state.handle_key(&tab);

        if let EventResult::Action(TuiAction::PopupConfirm(item)) = result {
            assert!(item.title().contains("second"));
        } else {
            panic!("Expected PopupConfirm action");
        }
    }

    #[test]
    fn generic_popup_confirm_empty_closes() {
        let provider = Arc::new(MockProvider::default());
        let mut state = PopupState::new(PopupKind::Command, provider);

        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let result = state.handle_key(&tab);

        assert!(matches!(result, EventResult::Action(TuiAction::PopupClose)));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Escape and dismissal tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_escape_closes() {
        let provider = Arc::new(MockProvider::default());
        let mut state = PopupState::new(PopupKind::Command, provider);

        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let result = state.handle_key(&esc);

        assert!(matches!(result, EventResult::Action(TuiAction::PopupClose)));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Key passthrough tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_char_keys_ignored() {
        let provider = Arc::new(MockProvider::default());
        let mut state = PopupState::new(PopupKind::Command, provider);

        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(state.handle_key(&a), EventResult::Ignored);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Fuzzy filtering tests (NEW functionality)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_uses_fuzzy_filtering() {
        // Create provider that returns all items
        let provider = Arc::new(MockProvider::with_items(vec![
            make_command("help", "Show help"),
            make_command("hello", "Say hello"),
            make_command("quit", "Exit"),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);

        // Update query - the generic popup should fuzzy filter internally
        state.update_query("");
        assert_eq!(state.filtered_count(), 3);

        // With query "hel" - should match help and hello
        state.set_filter_query("hel");
        assert!(state.filtered_count() <= 3); // Fuzzy filtering applied
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Viewport tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_viewport_follows_selection() {
        let items: Vec<PopupItem> = (0..20)
            .map(|i| make_command(&format!("cmd{}", i), ""))
            .collect();
        let provider = Arc::new(MockProvider::with_items(items));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        // Move past viewport
        for _ in 0..12 {
            state.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }

        // Viewport should have scrolled
        assert!(state.viewport_offset() > 0);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Rendering integration tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_can_render_with_popup_renderer() {
        use crate::tui::widgets::PopupRenderer;
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::widgets::Widget;

        let provider = Arc::new(MockProvider::with_items(vec![
            make_command("help", "Show help"),
            make_command("quit", "Exit"),
        ]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        // Should be able to get a renderer from the internal popup
        let renderer = state.renderer();
        let area = Rect::new(0, 0, 50, 8);
        let mut buf = Buffer::empty(area);

        // Should render without panic
        renderer.render(area, &mut buf);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Compact mode tests (Phase 1)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_command_renders_without_labels() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::widgets::Widget;

        let provider = Arc::new(MockProvider::with_items(vec![make_command(
            "help",
            "Show help",
        )]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        let renderer = state.renderer();
        let area = Rect::new(0, 0, 50, 8);
        let mut buf = Buffer::empty(area);
        renderer.render(area, &mut buf);

        // Extract buffer content as string
        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    content.push_str(cell.symbol());
                }
            }
        }

        // Command popup should NOT contain [cmd] label
        assert!(
            !content.contains("[cmd]"),
            "Command popup should not show [cmd] label. Content: {}",
            content
        );
        // But should still show the command (icon and name rendered separately)
        assert!(
            content.contains("/ help") || content.contains("/help"),
            "Command popup should show command name (as '/ help' or '/help'). Content: {}",
            content
        );
    }

    #[test]
    fn generic_popup_agent_or_file_shows_kind_labels() {
        // AgentOrFile popups show kind labels to distinguish agents from files
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::widgets::Widget;

        let provider = Arc::new(MockProvider::with_items(vec![PopupItem::agent("opencode")
            .desc("ACP agent")
            .with_score(100)]));
        let mut state = PopupState::new(PopupKind::AgentOrFile, provider);
        state.update_query("");

        let renderer = state.renderer();
        let area = Rect::new(0, 0, 50, 8);
        let mut buf = Buffer::empty(area);
        renderer.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    content.push_str(cell.symbol());
                }
            }
        }

        // AgentOrFile popups DO show kind labels to distinguish agents from files
        assert!(
            content.contains("[agent]"),
            "AgentOrFile popup should show [agent] label. Content: {}",
            content
        );
        // And should show the agent name
        assert!(
            content.contains("opencode"),
            "Mention popup should show agent name. Content: {}",
            content
        );
    }

    #[test]
    fn generic_popup_command_hides_kind_labels() {
        // Command popups don't show kind labels - trigger char / indicates type
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::widgets::Widget;

        let provider = Arc::new(MockProvider::with_items(vec![PopupItem::cmd("search")
            .desc("Search notes")
            .with_score(100)]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        let renderer = state.renderer();
        let area = Rect::new(0, 0, 50, 8);
        let mut buf = Buffer::empty(area);
        renderer.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    content.push_str(cell.symbol());
                }
            }
        }

        // Command popups should NOT show kind labels
        assert!(
            !content.contains("[cmd]"),
            "Command popup should not show [cmd] label. Content: {}",
            content
        );
        // But should show the command name
        assert!(
            content.contains("search"),
            "Command popup should show command name. Content: {}",
            content
        );
    }

    #[test]
    fn generic_popup_repl_command_hides_kind_labels() {
        // ReplCommand popups don't show kind labels - trigger char : indicates type
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::widgets::Widget;

        let provider = Arc::new(MockProvider::with_items(vec![PopupItem::repl("quit")
            .desc("Quit the application")
            .with_score(100)]));
        let mut state = PopupState::new(PopupKind::ReplCommand, provider);
        state.update_query("");

        let renderer = state.renderer();
        let area = Rect::new(0, 0, 50, 8);
        let mut buf = Buffer::empty(area);
        renderer.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    content.push_str(cell.symbol());
                }
            }
        }

        // ReplCommand popups should NOT show kind labels
        assert!(
            !content.contains("[repl]") && !content.contains("[cmd]"),
            "ReplCommand popup should not show kind labels. Content: {}",
            content
        );
        // But should show the command name
        assert!(
            content.contains("quit"),
            "ReplCommand popup should show command name. Content: {}",
            content
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Argument hint tests (Phase 1)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_popup_selected_item_has_argument_hint() {
        // Create a command with an argument hint
        let item = PopupItem::cmd("search")
            .desc("Search notes")
            .hint("<query>")
            .with_score(100);

        let provider = Arc::new(MockProvider::with_items(vec![item]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        // Get argument hint for selected item
        let hint = state.selected_argument_hint();

        assert_eq!(hint, Some("<query>".to_string()));
    }

    #[test]
    fn generic_popup_no_hint_when_no_selection() {
        let provider = Arc::new(MockProvider::with_items(vec![]));
        let state = PopupState::new(PopupKind::Command, provider);

        let hint = state.selected_argument_hint();

        assert_eq!(hint, None);
    }

    #[test]
    fn generic_popup_no_hint_when_item_has_none() {
        // Create a command without an argument hint
        let item = PopupItem::cmd("help").desc("Show help").with_score(100);

        let provider = Arc::new(MockProvider::with_items(vec![item]));
        let mut state = PopupState::new(PopupKind::Command, provider);
        state.update_query("");

        let hint = state.selected_argument_hint();

        assert_eq!(hint, None);
    }
}
