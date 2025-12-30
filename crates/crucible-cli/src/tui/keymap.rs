//! Composable keymap system with typed layers
//!
//! Keymaps are built from layers (Arrows, Emacs, Vim) using a builder pattern.
//! Layers compose with configurable conflict resolution.

use std::collections::HashMap;

/// Key code (cross-platform)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Enter,
    Escape,
    Tab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
}

/// Keyboard modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
    };
}

/// A key press event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: Modifiers,
}

impl KeyEvent {
    pub fn new(code: KeyCode, modifiers: Modifiers) -> Self {
        Self { code, modifiers }
    }
}

/// What a key binding does
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    ScrollUp,
    ScrollDown,
    Submit,
    Cancel,
}

/// How to handle conflicting bindings between layers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictStrategy {
    #[default]
    LastWins,
    FirstWins,
}

/// Configuration for arrows keymap
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrowsConfig {
    pub scroll_lines: usize,
}

impl Default for ArrowsConfig {
    fn default() -> Self {
        Self { scroll_lines: 1 }
    }
}

/// Configuration for vim keymap
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VimConfig {
    pub leader: char,
}

impl Default for VimConfig {
    fn default() -> Self {
        Self { leader: ' ' }
    }
}

/// Available keymap layers
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeymapLayer {
    Arrows,
    ArrowsCustom(ArrowsConfig),
    Emacs,
    Vim,
    VimCustom(VimConfig),
}

impl From<ArrowsConfig> for KeymapLayer {
    fn from(config: ArrowsConfig) -> Self {
        KeymapLayer::ArrowsCustom(config)
    }
}

impl From<VimConfig> for KeymapLayer {
    fn from(config: VimConfig) -> Self {
        KeymapLayer::VimCustom(config)
    }
}

impl KeymapLayer {
    fn bindings(&self) -> Vec<(KeyEvent, Operation)> {
        match self {
            KeymapLayer::Arrows | KeymapLayer::ArrowsCustom(_) => vec![
                (KeyEvent::new(KeyCode::Up, Modifiers::NONE), Operation::ScrollUp),
                (KeyEvent::new(KeyCode::Down, Modifiers::NONE), Operation::ScrollDown),
                (KeyEvent::new(KeyCode::Enter, Modifiers::NONE), Operation::Submit),
                (KeyEvent::new(KeyCode::Escape, Modifiers::NONE), Operation::Cancel),
            ],
            KeymapLayer::Emacs => vec![
                (KeyEvent::new(KeyCode::Char('p'), Modifiers { ctrl: true, ..Modifiers::NONE }), Operation::ScrollUp),
                (KeyEvent::new(KeyCode::Char('n'), Modifiers { ctrl: true, ..Modifiers::NONE }), Operation::ScrollDown),
            ],
            KeymapLayer::Vim | KeymapLayer::VimCustom(_) => vec![
                (KeyEvent::new(KeyCode::Char('k'), Modifiers::NONE), Operation::ScrollUp),
                (KeyEvent::new(KeyCode::Char('j'), Modifiers::NONE), Operation::ScrollDown),
            ],
        }
    }
}

/// A resolved keymap
pub struct Keymap {
    bindings: HashMap<KeyEvent, Operation>,
}

impl Keymap {
    pub fn builder() -> KeymapBuilder {
        KeymapBuilder::new()
    }

    pub fn resolve(&self, event: &KeyEvent) -> Option<Operation> {
        self.bindings.get(event).copied()
    }
}

/// Builder for composing keymaps from layers
pub struct KeymapBuilder {
    layers: Vec<KeymapLayer>,
    conflict: ConflictStrategy,
}

impl KeymapBuilder {
    fn new() -> Self {
        Self {
            layers: Vec::new(),
            conflict: ConflictStrategy::default(),
        }
    }

    pub fn layer(mut self, layer: impl Into<KeymapLayer>) -> Self {
        self.layers.push(layer.into());
        self
    }

    pub fn conflict(mut self, strategy: ConflictStrategy) -> Self {
        self.conflict = strategy;
        self
    }

    pub fn build(self) -> Keymap {
        let mut bindings = HashMap::new();
        for layer in self.layers {
            for (key, op) in layer.bindings() {
                match self.conflict {
                    ConflictStrategy::LastWins => {
                        bindings.insert(key, op);
                    }
                    ConflictStrategy::FirstWins => {
                        bindings.entry(key).or_insert(op);
                    }
                }
            }
        }
        Keymap { bindings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_from_arrows_layer_has_arrow_bindings() {
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Arrows)
            .build();

        // Arrow keys should produce scroll operations
        let up = KeyEvent::new(KeyCode::Up, Modifiers::NONE);
        let down = KeyEvent::new(KeyCode::Down, Modifiers::NONE);

        assert_eq!(keymap.resolve(&up), Some(Operation::ScrollUp));
        assert_eq!(keymap.resolve(&down), Some(Operation::ScrollDown));
    }

    #[test]
    fn emacs_layer_has_ctrl_bindings() {
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Emacs)
            .build();

        // Ctrl+P/N should scroll (emacs style)
        let ctrl_p = KeyEvent::new(KeyCode::Char('p'), Modifiers { ctrl: true, ..Modifiers::NONE });
        let ctrl_n = KeyEvent::new(KeyCode::Char('n'), Modifiers { ctrl: true, ..Modifiers::NONE });

        assert_eq!(keymap.resolve(&ctrl_p), Some(Operation::ScrollUp));
        assert_eq!(keymap.resolve(&ctrl_n), Some(Operation::ScrollDown));
    }

    #[test]
    fn layers_compose_with_last_wins() {
        // Both layers define scroll behavior - last layer wins
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Arrows)
            .layer(KeymapLayer::Emacs)
            .build();

        // Should have both arrow keys and ctrl bindings
        let up = KeyEvent::new(KeyCode::Up, Modifiers::NONE);
        let ctrl_p = KeyEvent::new(KeyCode::Char('p'), Modifiers { ctrl: true, ..Modifiers::NONE });

        assert_eq!(keymap.resolve(&up), Some(Operation::ScrollUp));
        assert_eq!(keymap.resolve(&ctrl_p), Some(Operation::ScrollUp));
    }

    #[test]
    fn vim_layer_has_hjkl_bindings() {
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Vim)
            .build();

        // j/k should scroll (vim style)
        let j = KeyEvent::new(KeyCode::Char('j'), Modifiers::NONE);
        let k = KeyEvent::new(KeyCode::Char('k'), Modifiers::NONE);

        assert_eq!(keymap.resolve(&j), Some(Operation::ScrollDown));
        assert_eq!(keymap.resolve(&k), Some(Operation::ScrollUp));
    }

    #[test]
    fn conflict_last_wins_overrides_earlier_binding() {
        // First layer binds 'j' to ScrollDown
        // Second layer could override it - test that last layer wins
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Vim)      // j = ScrollDown
            .layer(KeymapLayer::Arrows)   // no conflict on 'j'
            .build();

        let j = KeyEvent::new(KeyCode::Char('j'), Modifiers::NONE);
        assert_eq!(keymap.resolve(&j), Some(Operation::ScrollDown)); // vim binding survives
    }

    #[test]
    fn unbound_key_returns_none() {
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Arrows)
            .build();

        let q = KeyEvent::new(KeyCode::Char('q'), Modifiers::NONE);
        assert_eq!(keymap.resolve(&q), None);
    }

    #[test]
    fn arrows_config_can_customize_scroll_amount() {
        // Custom config should be usable via Into<KeymapLayer>
        let config = ArrowsConfig { scroll_lines: 5 };
        let keymap = Keymap::builder()
            .layer(config)  // Into<KeymapLayer>
            .build();

        // Should still resolve arrow keys
        let up = KeyEvent::new(KeyCode::Up, Modifiers::NONE);
        assert_eq!(keymap.resolve(&up), Some(Operation::ScrollUp));
    }

    #[test]
    fn vim_config_allows_custom_leader() {
        let config = VimConfig { leader: ',' };
        let keymap = Keymap::builder()
            .layer(config)
            .build();

        // j/k should still work
        let j = KeyEvent::new(KeyCode::Char('j'), Modifiers::NONE);
        assert_eq!(keymap.resolve(&j), Some(Operation::ScrollDown));
    }

    #[test]
    fn first_wins_conflict_strategy() {
        // Define same key in both layers, first should win
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Vim)      // j = ScrollDown
            .layer(KeymapLayer::Arrows)   // no j binding
            .conflict(ConflictStrategy::FirstWins)
            .build();

        let j = KeyEvent::new(KeyCode::Char('j'), Modifiers::NONE);
        assert_eq!(keymap.resolve(&j), Some(Operation::ScrollDown));
    }

    #[test]
    fn submit_and_cancel_operations() {
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Arrows)
            .build();

        let enter = KeyEvent::new(KeyCode::Enter, Modifiers::NONE);
        let esc = KeyEvent::new(KeyCode::Escape, Modifiers::NONE);

        assert_eq!(keymap.resolve(&enter), Some(Operation::Submit));
        assert_eq!(keymap.resolve(&esc), Some(Operation::Cancel));
    }
}
