//! Keybinding configuration for the chat TUI
//!
//! Provides layered keybinding resolution: global → mode-specific.
//! Supports future vim modes by using HashMap<InputMode, ...>.
//!
//! ## Architecture
//!
//! Resolution order:
//! 1. Check global bindings first (Ctrl+C, Ctrl+D)
//! 2. Check mode-specific bindings (Enter in Normal = Submit, Enter in Completion = Confirm)
//!
//! ## Extensibility
//!
//! The layered HashMap design supports future vim modes:
//! - VimNormal → motion commands, i for insert
//! - VimInsert → text editing, Esc for normal
//! - VimVisual → selection commands

use std::collections::HashMap;

use crucible_core::traits::{InputMode, KeyAction, KeyCode, KeyPattern, Modifiers};

/// Layered keybinding configuration
///
/// Resolution order:
/// 1. Check global bindings first (always active)
/// 2. Check mode-specific bindings (depends on current InputMode)
#[derive(Debug, Clone)]
pub struct KeyBindings {
    /// Global bindings (active in all modes)
    global: HashMap<KeyPattern, KeyAction>,
    /// Mode-specific bindings
    modes: HashMap<InputMode, HashMap<KeyPattern, KeyAction>>,
}

impl KeyBindings {
    /// Create empty keybindings
    pub fn new() -> Self {
        Self {
            global: HashMap::new(),
            modes: HashMap::new(),
        }
    }

    /// Create default keybindings matching current behavior
    pub fn defaults() -> Self {
        let mut bindings = Self::new();

        // === Global bindings ===
        bindings.bind_global(KeyPattern::ctrl('d'), KeyAction::Quit);
        bindings.bind_global(KeyPattern::ctrl('c'), KeyAction::Cancel);

        // === Normal mode ===
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::key(KeyCode::Enter),
            KeyAction::Submit,
        );
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::new(KeyCode::Enter, Modifiers::CONTROL),
            KeyAction::InsertNewline,
        );
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::new(KeyCode::Enter, Modifiers::SHIFT),
            KeyAction::InsertNewline,
        );

        // === Completion mode ===
        // Navigation
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Up),
            KeyAction::CompletionPrev,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Down),
            KeyAction::CompletionNext,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::ctrl('k'),
            KeyAction::CompletionPrev,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::ctrl('j'),
            KeyAction::CompletionNext,
        );

        // Selection
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Enter),
            KeyAction::CompletionConfirm,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Tab),
            KeyAction::CompletionConfirm,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Escape),
            KeyAction::CompletionCancel,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Space),
            KeyAction::ToggleSelection,
        );

        bindings
    }

    /// Resolve a key pattern to an action
    ///
    /// Checks global bindings first, then mode-specific.
    pub fn resolve(&self, key: KeyPattern, mode: InputMode) -> Option<KeyAction> {
        // 1. Check global first
        if let Some(action) = self.global.get(&key) {
            return Some(*action);
        }
        // 2. Check mode-specific
        self.modes.get(&mode)?.get(&key).copied()
    }

    /// Bind a key globally (all modes)
    pub fn bind_global(&mut self, key: KeyPattern, action: KeyAction) {
        self.global.insert(key, action);
    }

    /// Bind a key for a specific mode
    pub fn bind_mode(&mut self, mode: InputMode, key: KeyPattern, action: KeyAction) {
        self.modes.entry(mode).or_default().insert(key, action);
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Basic resolution tests ===

    #[test]
    fn test_empty_bindings_returns_none() {
        let bindings = KeyBindings::new();
        let result = bindings.resolve(KeyPattern::char('a'), InputMode::Normal);
        assert!(result.is_none());
    }

    #[test]
    fn test_global_binding() {
        let mut bindings = KeyBindings::new();
        bindings.bind_global(KeyPattern::ctrl('c'), KeyAction::Cancel);

        // Should work in any mode
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Normal),
            Some(KeyAction::Cancel)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Completion),
            Some(KeyAction::Cancel)
        );
    }

    #[test]
    fn test_mode_specific_binding() {
        let mut bindings = KeyBindings::new();
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::key(KeyCode::Enter),
            KeyAction::Submit,
        );

        // Should work in Normal mode
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Enter), InputMode::Normal),
            Some(KeyAction::Submit)
        );
        // Should NOT work in Completion mode
        assert!(bindings
            .resolve(KeyPattern::key(KeyCode::Enter), InputMode::Completion)
            .is_none());
    }

    #[test]
    fn test_global_takes_precedence() {
        let mut bindings = KeyBindings::new();

        // Bind same key globally and for mode
        bindings.bind_global(KeyPattern::ctrl('c'), KeyAction::Quit);
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::ctrl('c'),
            KeyAction::Cancel,
        );

        // Global should win
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Normal),
            Some(KeyAction::Quit)
        );
    }

    // === Default keybinding tests ===

    #[test]
    fn test_defaults_global_ctrl_d_quits() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('d'), InputMode::Normal),
            Some(KeyAction::Quit)
        );
    }

    #[test]
    fn test_defaults_ctrl_c_cancels() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Normal),
            Some(KeyAction::Cancel)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Completion),
            Some(KeyAction::Cancel)
        );
    }

    #[test]
    fn test_defaults_enter_submits_in_normal() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Enter), InputMode::Normal),
            Some(KeyAction::Submit)
        );
    }

    #[test]
    fn test_defaults_ctrl_enter_newline() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.resolve(
                KeyPattern::new(KeyCode::Enter, Modifiers::CONTROL),
                InputMode::Normal
            ),
            Some(KeyAction::InsertNewline)
        );
    }

    #[test]
    fn test_defaults_shift_enter_newline() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.resolve(
                KeyPattern::new(KeyCode::Enter, Modifiers::SHIFT),
                InputMode::Normal
            ),
            Some(KeyAction::InsertNewline)
        );
    }

    #[test]
    fn test_defaults_completion_navigation() {
        let bindings = KeyBindings::defaults();

        // Arrow keys
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Up), InputMode::Completion),
            Some(KeyAction::CompletionPrev)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Down), InputMode::Completion),
            Some(KeyAction::CompletionNext)
        );

        // Ctrl+J/K
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('j'), InputMode::Completion),
            Some(KeyAction::CompletionNext)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('k'), InputMode::Completion),
            Some(KeyAction::CompletionPrev)
        );
    }

    #[test]
    fn test_defaults_completion_confirm() {
        let bindings = KeyBindings::defaults();

        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Enter), InputMode::Completion),
            Some(KeyAction::CompletionConfirm)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Tab), InputMode::Completion),
            Some(KeyAction::CompletionConfirm)
        );
    }

    #[test]
    fn test_defaults_completion_cancel() {
        let bindings = KeyBindings::defaults();

        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Escape), InputMode::Completion),
            Some(KeyAction::CompletionCancel)
        );
    }

    #[test]
    fn test_defaults_toggle_selection() {
        let bindings = KeyBindings::defaults();

        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Space), InputMode::Completion),
            Some(KeyAction::ToggleSelection)
        );
    }

    // === Mode isolation tests ===

    #[test]
    fn test_normal_mode_bindings_not_in_completion() {
        let bindings = KeyBindings::defaults();

        // Enter in Normal = Submit
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Enter), InputMode::Normal),
            Some(KeyAction::Submit)
        );

        // Enter in Completion = Confirm (different action)
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Enter), InputMode::Completion),
            Some(KeyAction::CompletionConfirm)
        );
    }

    #[test]
    fn test_completion_bindings_not_in_normal() {
        let bindings = KeyBindings::defaults();

        // Space in Completion = ToggleSelection
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Space), InputMode::Completion),
            Some(KeyAction::ToggleSelection)
        );

        // Space in Normal = not bound (passthrough to input)
        assert!(bindings
            .resolve(KeyPattern::key(KeyCode::Space), InputMode::Normal)
            .is_none());
    }
}
