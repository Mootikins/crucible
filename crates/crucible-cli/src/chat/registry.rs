//! Dynamic command registry for static and agent-provided commands
//!
//! Manages two types of slash commands:
//! - **Static commands**: CLI-defined, always available (/plan, /act, /search, etc.)
//! - **Dynamic commands**: Agent-provided, can change during session (/web, /test, etc.)
//!
//! ## Architecture
//!
//! Static commands are registered at startup and handled directly by the CLI.
//! Dynamic commands are forwarded to the agent for execution.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crucible_core::traits::chat::{
    ChatContext, ChatResult, ChatError, CommandDescriptor, CommandHandler, CommandRegistry,
};

/// CLI implementation of command registry
///
/// Maintains separate registries for static (CLI-owned) and dynamic (agent-provided) commands.
pub struct CliCommandRegistry {
    /// Static commands (always available, handled by CLI)
    static_commands: HashMap<String, Arc<dyn CommandHandler>>,

    /// Dynamic commands (agent-provided, forwarded to agent)
    dynamic_commands: Vec<CommandDescriptor>,
}

impl CliCommandRegistry {
    /// Create a new empty command registry
    pub fn new() -> Self {
        Self {
            static_commands: HashMap::new(),
            dynamic_commands: Vec::new(),
        }
    }

    /// Check if a command is static (CLI-defined)
    ///
    /// Returns true if the command is registered as a static command.
    pub fn is_static(&self, name: &str) -> bool {
        self.static_commands.contains_key(name)
    }

    /// Check if a command is dynamic (agent-provided)
    ///
    /// Returns true if the command is in the dynamic commands list.
    pub fn is_dynamic(&self, name: &str) -> bool {
        self.dynamic_commands.iter().any(|c| c.name == name)
    }

    /// Get a static command handler by name
    pub fn get_static(&self, name: &str) -> Option<Arc<dyn CommandHandler>> {
        self.static_commands.get(name).cloned()
    }

    /// Get dynamic command descriptor by name
    pub fn get_dynamic(&self, name: &str) -> Option<&CommandDescriptor> {
        self.dynamic_commands.iter().find(|c| c.name == name)
    }
}

impl Default for CliCommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandRegistry for CliCommandRegistry {
    fn register_static(&mut self, name: &str, handler: Box<dyn CommandHandler>) {
        self.static_commands
            .insert(name.to_string(), Arc::from(handler));
    }

    fn update_dynamic(&mut self, commands: Vec<CommandDescriptor>) {
        self.dynamic_commands = commands;
    }

    async fn execute(
        &self,
        name: &str,
        args: &str,
        ctx: &mut dyn ChatContext,
    ) -> ChatResult<()> {
        if let Some(handler) = self.static_commands.get(name) {
            // Execute static command via handler
            handler.execute(args, ctx).await
        } else if self.dynamic_commands.iter().any(|c| c.name == name) {
            // Forward dynamic command to agent
            ctx.send_command_to_agent(name, args).await
        } else {
            Err(ChatError::UnknownCommand(name.to_string()))
        }
    }

    fn list_commands(&self) -> Vec<CommandDescriptor> {
        // Combine static and dynamic commands
        let mut commands = Vec::new();

        // Add static commands (with placeholder descriptions)
        for name in self.static_commands.keys() {
            commands.push(CommandDescriptor {
                name: name.clone(),
                description: format!("Static command: {}", name),
                input_hint: None,
            });
        }

        // Add dynamic commands
        commands.extend(self.dynamic_commands.clone());

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock command handler for testing
    struct MockCommand {
        name: String,
    }

    #[async_trait]
    impl CommandHandler for MockCommand {
        async fn execute(&self, _args: &str, _ctx: &mut dyn ChatContext) -> ChatResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_new_registry_is_empty() {
        let registry = CliCommandRegistry::new();
        assert_eq!(registry.static_commands.len(), 0);
        assert_eq!(registry.dynamic_commands.len(), 0);
    }

    #[test]
    fn test_register_static_command() {
        let mut registry = CliCommandRegistry::new();
        let handler = Box::new(MockCommand {
            name: "test".to_string(),
        });

        registry.register_static("test", handler);

        assert!(registry.is_static("test"));
        assert!(!registry.is_static("nonexistent"));
    }

    #[test]
    fn test_update_dynamic_commands() {
        let mut registry = CliCommandRegistry::new();

        let commands = vec![
            CommandDescriptor {
                name: "web".to_string(),
                description: "Search the web".to_string(),
                input_hint: Some("query".to_string()),
            },
            CommandDescriptor {
                name: "test".to_string(),
                description: "Run tests".to_string(),
                input_hint: None,
            },
        ];

        registry.update_dynamic(commands);

        assert!(registry.is_dynamic("web"));
        assert!(registry.is_dynamic("test"));
        assert!(!registry.is_dynamic("nonexistent"));
    }

    #[test]
    fn test_list_commands_includes_both_static_and_dynamic() {
        let mut registry = CliCommandRegistry::new();

        // Register static command
        registry.register_static(
            "plan",
            Box::new(MockCommand {
                name: "plan".to_string(),
            }),
        );

        // Add dynamic commands
        registry.update_dynamic(vec![CommandDescriptor {
            name: "web".to_string(),
            description: "Search the web".to_string(),
            input_hint: Some("query".to_string()),
        }]);

        let commands = registry.list_commands();
        assert_eq!(commands.len(), 2);

        let names: Vec<_> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"plan"));
        assert!(names.contains(&"web"));
    }

    #[test]
    fn test_get_static_returns_handler() {
        let mut registry = CliCommandRegistry::new();
        registry.register_static(
            "test",
            Box::new(MockCommand {
                name: "test".to_string(),
            }),
        );

        assert!(registry.get_static("test").is_some());
        assert!(registry.get_static("nonexistent").is_none());
    }

    #[test]
    fn test_get_dynamic_returns_descriptor() {
        let mut registry = CliCommandRegistry::new();
        registry.update_dynamic(vec![CommandDescriptor {
            name: "web".to_string(),
            description: "Search the web".to_string(),
            input_hint: Some("query".to_string()),
        }]);

        assert!(registry.get_dynamic("web").is_some());
        assert!(registry.get_dynamic("nonexistent").is_none());
    }
}
