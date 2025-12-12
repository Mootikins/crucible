//! Slash command registry implementing the generic Registry trait
//!
//! This provides a builder-based registry for slash commands that follows
//! the generic Registry pattern from crucible-core.
//!
//! ## Architecture
//!
//! - **SlashCommandRegistry**: Immutable registry implementing `Registry<Key=String, Value=SlashCommand>`
//! - **SlashCommandRegistryBuilder**: Builder for constructing registries
//! - **SlashCommand**: Command wrapper containing handler and descriptor
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_cli::chat::slash_registry::{SlashCommandRegistryBuilder, SlashCommand};
//! use crucible_core::traits::registry::RegistryBuilder;
//!
//! let registry = SlashCommandRegistryBuilder::default()
//!     .command("exit", handler, "Exit the chat session", None)
//!     .command_with_hint("search", handler, "Search knowledge base", Some("query"))
//!     .build();
//!
//! // Query the registry
//! if let Some(cmd) = registry.get("exit") {
//!     // Execute command
//! }
//! ```

use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::Arc;

use crucible_core::traits::chat::{CommandDescriptor, CommandHandler};
use crucible_core::traits::registry::{Registry, RegistryBuilder};

/// A slash command entry containing handler and metadata
#[derive(Clone)]
pub struct SlashCommand {
    /// The command handler
    pub handler: Arc<dyn CommandHandler>,
    /// Command descriptor (name, description, input_hint)
    pub descriptor: CommandDescriptor,
}

impl SlashCommand {
    /// Create a new slash command
    pub fn new(
        handler: Arc<dyn CommandHandler>,
        name: impl Into<String>,
        description: impl Into<String>,
        input_hint: Option<String>,
    ) -> Self {
        Self {
            handler,
            descriptor: CommandDescriptor {
                name: name.into(),
                description: description.into(),
                input_hint,
            },
        }
    }
}

/// Immutable registry of slash commands
///
/// Implements the generic Registry trait for command lookups.
/// Built using SlashCommandRegistryBuilder.
#[derive(Clone)]
pub struct SlashCommandRegistry {
    /// Command storage (name -> command)
    commands: HashMap<String, SlashCommand>,
    /// Cached list of static command names
    static_names: Vec<String>,
    /// Cached list of dynamic command descriptors
    dynamic_descriptors: Vec<CommandDescriptor>,
}

impl SlashCommandRegistry {
    /// Get a command handler by name
    pub fn get_handler(&self, name: &str) -> Option<&Arc<dyn CommandHandler>> {
        self.commands.get(name).map(|cmd| &cmd.handler)
    }

    /// Get a command descriptor by name
    pub fn get_descriptor(&self, name: &str) -> Option<&CommandDescriptor> {
        self.commands.get(name).map(|cmd| &cmd.descriptor)
    }

    /// Check if a command is static (built-in)
    ///
    /// Static commands are registered at build time and don't change.
    pub fn is_static(&self, name: &str) -> bool {
        self.static_names.contains(&name.to_string())
    }

    /// Check if a command is dynamic (agent-provided)
    ///
    /// Dynamic commands are added at runtime via with_dynamic().
    pub fn is_dynamic(&self, name: &str) -> bool {
        self.dynamic_descriptors
            .iter()
            .any(|desc| desc.name == name)
    }

    /// Create a new registry with dynamic commands added
    ///
    /// Returns a new registry with the same static commands but updated dynamic commands.
    /// This is used when an agent publishes new commands at runtime.
    pub fn with_dynamic(&self, dynamic: Vec<CommandDescriptor>) -> Self {
        Self {
            commands: self.commands.clone(),
            static_names: self.static_names.clone(),
            dynamic_descriptors: dynamic,
        }
    }

    /// List all command descriptors (static + dynamic)
    pub fn list_all(&self) -> Vec<CommandDescriptor> {
        let mut all = Vec::new();

        // Add static commands
        for name in &self.static_names {
            if let Some(cmd) = self.commands.get(name) {
                all.push(cmd.descriptor.clone());
            }
        }

        // Add dynamic commands
        all.extend(self.dynamic_descriptors.clone());

        all
    }
}

impl Registry for SlashCommandRegistry {
    type Key = String;
    type Value = SlashCommand;

    fn get<Q>(&self, key: &Q) -> Option<&Self::Value>
    where
        Self::Key: Borrow<Q>,
        Q: ?Sized + Eq + std::hash::Hash,
    {
        self.commands.get(key)
    }

    fn iter(&self) -> impl Iterator<Item = (&Self::Key, &Self::Value)> {
        self.commands.iter()
    }

    fn len(&self) -> usize {
        self.commands.len()
    }
}

/// Builder for SlashCommandRegistry
///
/// Accumulates command registrations, then builds an immutable registry.
///
/// ## Example
///
/// ```rust,ignore
/// use crucible_cli::chat::slash_registry::SlashCommandRegistryBuilder;
/// use crucible_core::traits::registry::RegistryBuilder;
///
/// let registry = SlashCommandRegistryBuilder::default()
///     .command("exit", handler, "Exit session", None)
///     .build();
/// ```
#[derive(Default)]
pub struct SlashCommandRegistryBuilder {
    commands: HashMap<String, SlashCommand>,
}

impl SlashCommandRegistryBuilder {
    /// Register a command without input hint
    pub fn command(
        self,
        name: impl Into<String>,
        handler: Arc<dyn CommandHandler>,
        description: impl Into<String>,
    ) -> Self {
        self.command_with_hint(name, handler, description, None)
    }

    /// Register a command with input hint
    pub fn command_with_hint(
        mut self,
        name: impl Into<String>,
        handler: Arc<dyn CommandHandler>,
        description: impl Into<String>,
        input_hint: Option<String>,
    ) -> Self {
        let name_str = name.into();
        let cmd = SlashCommand::new(handler, name_str.clone(), description, input_hint);
        self.commands.insert(name_str, cmd);
        self
    }
}

impl RegistryBuilder for SlashCommandRegistryBuilder {
    type Registry = SlashCommandRegistry;
    type Key = String;
    type Value = SlashCommand;

    fn register(mut self, key: Self::Key, value: Self::Value) -> Self {
        self.commands.insert(key, value);
        self
    }

    fn build(self) -> Self::Registry {
        let static_names: Vec<String> = self.commands.keys().cloned().collect();

        SlashCommandRegistry {
            commands: self.commands,
            static_names,
            dynamic_descriptors: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::traits::chat::{ChatContext, ChatResult};

    // Mock handler for testing
    struct MockHandler;

    #[async_trait]
    impl CommandHandler for MockHandler {
        async fn execute(&self, _args: &str, _ctx: &mut dyn ChatContext) -> ChatResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_builder_creates_empty_registry() {
        let registry = SlashCommandRegistryBuilder::default().build();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_builder_registers_commands() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("exit", handler.clone(), "Exit the session")
            .command_with_hint(
                "search",
                handler.clone(),
                "Search knowledge base",
                Some("query".to_string()),
            )
            .build();

        assert_eq!(registry.len(), 2);
        assert!(registry.contains("exit"));
        assert!(registry.contains("search"));
    }

    #[test]
    fn test_registry_get_handler() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("exit", handler.clone(), "Exit")
            .build();

        assert!(registry.get_handler("exit").is_some());
        assert!(registry.get_handler("nonexistent").is_none());
    }

    #[test]
    fn test_registry_get_descriptor() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command_with_hint("search", handler, "Search KB", Some("query".to_string()))
            .build();

        let desc = registry.get_descriptor("search").unwrap();
        assert_eq!(desc.name, "search");
        assert_eq!(desc.description, "Search KB");
        assert_eq!(desc.input_hint, Some("query".to_string()));
    }

    #[test]
    fn test_static_vs_dynamic_commands() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("exit", handler, "Exit")
            .build();

        // Static command
        assert!(registry.is_static("exit"));
        assert!(!registry.is_dynamic("exit"));

        // Add dynamic commands
        let registry = registry.with_dynamic(vec![CommandDescriptor {
            name: "web".to_string(),
            description: "Search web".to_string(),
            input_hint: None,
        }]);

        assert!(registry.is_dynamic("web"));
        assert!(!registry.is_static("web"));
    }

    #[test]
    fn test_list_all_includes_static_and_dynamic() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("exit", handler, "Exit")
            .build();

        let registry = registry.with_dynamic(vec![CommandDescriptor {
            name: "web".to_string(),
            description: "Search web".to_string(),
            input_hint: None,
        }]);

        let all = registry.list_all();
        assert_eq!(all.len(), 2);

        let names: Vec<_> = all.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"exit"));
        assert!(names.contains(&"web"));
    }

    #[test]
    fn test_with_dynamic_preserves_static() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("exit", handler, "Exit")
            .build();

        let registry2 = registry.with_dynamic(vec![CommandDescriptor {
            name: "web".to_string(),
            description: "Search web".to_string(),
            input_hint: None,
        }]);

        // Original static command still accessible
        assert!(registry2.get_handler("exit").is_some());
        assert!(registry2.is_static("exit"));
    }

    #[test]
    fn test_registry_trait_implementation() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("exit", handler.clone(), "Exit")
            .command("help", handler, "Show help")
            .build();

        // Test Registry trait methods
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
        assert!(registry.contains("exit"));

        // Test iteration
        let names: Vec<_> = registry.iter().map(|(k, _)| k.as_str()).collect();
        assert!(names.contains(&"exit"));
        assert!(names.contains(&"help"));
    }
}
