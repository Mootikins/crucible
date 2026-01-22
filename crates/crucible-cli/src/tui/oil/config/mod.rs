//! Configuration system for vim-style `:set` commands.
//!
//! This module provides types for managing configuration options
//! in the TUI, inspired by Vim's option system.
//!
//! ## Submodules
//!
//! - [`overlay`]: Runtime configuration overlay with audit stack
//! - [`presets`]: Thinking budget presets for LLM reasoning control
//! - [`shortcuts`]: Option name shortcuts mapping to config paths
//! - [`stack`]: Audit stack for config modifications
//! - [`value`]: Dynamic configuration value types

mod overlay;
pub mod presets;
pub mod shortcuts;
pub mod stack;
mod value;

pub use overlay::{RuntimeConfig, SetError};
pub use presets::{ThinkingPreset, THINKING_PRESETS};
pub use shortcuts::{
    CompletionSource, ConfigShortcut, ShortcutRegistry, ShortcutTarget, SHORTCUTS,
};
pub use stack::{ConfigMod, ConfigStack, ModSource};
pub use value::ConfigValue;
