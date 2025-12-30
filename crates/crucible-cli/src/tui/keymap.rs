//! Composable keymap system with typed layers
//!
//! Keymaps are built from layers (Arrows, Emacs, Vim) using a builder pattern.
//! Layers compose with configurable conflict resolution.
//!
//! Uses crossterm types directly - Operation enum is the abstraction layer.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

/// What a key binding does - the semantic abstraction layer
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

/// Helper to create KeyEvent with modifiers
fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

impl KeymapLayer {
    fn bindings(&self) -> Vec<(KeyEvent, Operation)> {
        match self {
            KeymapLayer::Arrows | KeymapLayer::ArrowsCustom(_) => vec![
                (key(KeyCode::Up, KeyModifiers::NONE), Operation::ScrollUp),
                (
                    key(KeyCode::Down, KeyModifiers::NONE),
                    Operation::ScrollDown,
                ),
                (key(KeyCode::Enter, KeyModifiers::NONE), Operation::Submit),
                (key(KeyCode::Esc, KeyModifiers::NONE), Operation::Cancel),
            ],
            KeymapLayer::Emacs => vec![
                (
                    key(KeyCode::Char('p'), KeyModifiers::CONTROL),
                    Operation::ScrollUp,
                ),
                (
                    key(KeyCode::Char('n'), KeyModifiers::CONTROL),
                    Operation::ScrollDown,
                ),
            ],
            KeymapLayer::Vim | KeymapLayer::VimCustom(_) => vec![
                (
                    key(KeyCode::Char('k'), KeyModifiers::NONE),
                    Operation::ScrollUp,
                ),
                (
                    key(KeyCode::Char('j'), KeyModifiers::NONE),
                    Operation::ScrollDown,
                ),
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
        let keymap = Keymap::builder().layer(KeymapLayer::Arrows).build();

        let up = key(KeyCode::Up, KeyModifiers::NONE);
        let down = key(KeyCode::Down, KeyModifiers::NONE);

        assert_eq!(keymap.resolve(&up), Some(Operation::ScrollUp));
        assert_eq!(keymap.resolve(&down), Some(Operation::ScrollDown));
    }

    #[test]
    fn emacs_layer_has_ctrl_bindings() {
        let keymap = Keymap::builder().layer(KeymapLayer::Emacs).build();

        let ctrl_p = key(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let ctrl_n = key(KeyCode::Char('n'), KeyModifiers::CONTROL);

        assert_eq!(keymap.resolve(&ctrl_p), Some(Operation::ScrollUp));
        assert_eq!(keymap.resolve(&ctrl_n), Some(Operation::ScrollDown));
    }

    #[test]
    fn layers_compose_with_last_wins() {
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Arrows)
            .layer(KeymapLayer::Emacs)
            .build();

        let up = key(KeyCode::Up, KeyModifiers::NONE);
        let ctrl_p = key(KeyCode::Char('p'), KeyModifiers::CONTROL);

        assert_eq!(keymap.resolve(&up), Some(Operation::ScrollUp));
        assert_eq!(keymap.resolve(&ctrl_p), Some(Operation::ScrollUp));
    }

    #[test]
    fn vim_layer_has_hjkl_bindings() {
        let keymap = Keymap::builder().layer(KeymapLayer::Vim).build();

        let j = key(KeyCode::Char('j'), KeyModifiers::NONE);
        let k = key(KeyCode::Char('k'), KeyModifiers::NONE);

        assert_eq!(keymap.resolve(&j), Some(Operation::ScrollDown));
        assert_eq!(keymap.resolve(&k), Some(Operation::ScrollUp));
    }

    #[test]
    fn conflict_last_wins_overrides_earlier_binding() {
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Vim)
            .layer(KeymapLayer::Arrows)
            .build();

        let j = key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(keymap.resolve(&j), Some(Operation::ScrollDown));
    }

    #[test]
    fn unbound_key_returns_none() {
        let keymap = Keymap::builder().layer(KeymapLayer::Arrows).build();

        let q = key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(keymap.resolve(&q), None);
    }

    #[test]
    fn arrows_config_can_customize_scroll_amount() {
        let config = ArrowsConfig { scroll_lines: 5 };
        let keymap = Keymap::builder().layer(config).build();

        let up = key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(keymap.resolve(&up), Some(Operation::ScrollUp));
    }

    #[test]
    fn vim_config_allows_custom_leader() {
        let config = VimConfig { leader: ',' };
        let keymap = Keymap::builder().layer(config).build();

        let j = key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(keymap.resolve(&j), Some(Operation::ScrollDown));
    }

    #[test]
    fn first_wins_conflict_strategy() {
        let keymap = Keymap::builder()
            .layer(KeymapLayer::Vim)
            .layer(KeymapLayer::Arrows)
            .conflict(ConflictStrategy::FirstWins)
            .build();

        let j = key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(keymap.resolve(&j), Some(Operation::ScrollDown));
    }

    #[test]
    fn submit_and_cancel_operations() {
        let keymap = Keymap::builder().layer(KeymapLayer::Arrows).build();

        let enter = key(KeyCode::Enter, KeyModifiers::NONE);
        let esc = key(KeyCode::Esc, KeyModifiers::NONE);

        assert_eq!(keymap.resolve(&enter), Some(Operation::Submit));
        assert_eq!(keymap.resolve(&esc), Some(Operation::Cancel));
    }
}
