//! Popup item registries
//!
//! Provides separate, type-safe registries for different popup item types.
//! Each registry implements `PopupProvider` for uniform access.

mod command;
mod context;
mod repl;

pub use command::CommandRegistry;
pub use context::ContextRegistry;
pub use repl::ReplCommandRegistry;

use crate::tui::popup::PopupProvider;
use crate::tui::state::{PopupItem, PopupKind};

/// Composite registry that delegates to specialized registries
///
/// This provides the same interface as `StaticPopupProvider` but
/// with cleaner separation of concerns.
pub struct CompositeRegistry {
    pub commands: CommandRegistry,
    pub context: ContextRegistry,
    pub repl: ReplCommandRegistry,
}

impl Default for CompositeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CompositeRegistry {
    pub fn new() -> Self {
        Self {
            commands: CommandRegistry::new(),
            context: ContextRegistry::new(),
            repl: ReplCommandRegistry::new(),
        }
    }
}

impl PopupProvider for CompositeRegistry {
    fn provide(&self, kind: PopupKind, query: &str) -> Vec<PopupItem> {
        match kind {
            PopupKind::Command => self.commands.provide(kind, query),
            PopupKind::AgentOrFile => self.context.provide(kind, query),
            PopupKind::ReplCommand => self.repl.provide(kind, query),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::traits::chat::CommandDescriptor;

    #[test]
    fn test_composite_registry_delegates_commands() {
        let mut registry = CompositeRegistry::new();
        registry.commands.add_command(CommandDescriptor {
            name: "test".into(),
            description: "Test command".into(),
            input_hint: None,
            secondary_options: vec![],
        });

        let items = registry.provide(PopupKind::Command, "test");
        assert_eq!(items.len(), 1);
        assert!(items[0].is_command());
    }

    #[test]
    fn test_composite_registry_delegates_agents() {
        let mut registry = CompositeRegistry::new();
        registry.context.add_agent("dev", "Developer agent");

        let items = registry.provide(PopupKind::AgentOrFile, "dev");
        assert_eq!(items.len(), 1);
        assert!(items[0].is_agent());
    }

    #[test]
    fn test_composite_registry_delegates_repl() {
        let mut registry = CompositeRegistry::new();
        registry.repl.add_command("quit", "Exit the application");

        let items = registry.provide(PopupKind::ReplCommand, "quit");
        assert_eq!(items.len(), 1);
        assert!(items[0].is_repl_command());
    }
}
