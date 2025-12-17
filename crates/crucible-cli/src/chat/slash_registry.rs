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
use crucible_core::types::acp::schema::AvailableCommand;

// ============================================================================
// Reserved Commands (Phase 3)
// ============================================================================

/// Reserved command names that cannot be overridden by agents.
///
/// These are core client commands that must always be available and cannot
/// be shadowed by agent-provided commands. Commands like exit, quit, help
/// are essential for session control.
pub const RESERVED_COMMANDS: &[&str] = &[
    "exit",    // Exit the session
    "quit",    // Alias for exit
    "help",    // Show help
    "search",  // Search knowledge base
    "context", // Context management
    "clear",   // Clear screen/context
    "mode",    // Mode switching
];

/// Unshadowable commands that can NEVER be overridden by agents.
///
/// These are a subset of reserved commands that are absolutely essential
/// for session control and cannot be namespaced away.
pub const UNSHADOWABLE_COMMANDS: &[&str] = &[
    "exit", // Must always be able to exit
    "quit", // Alias for exit
    "help", // Must always be able to get help
];

/// Check if a command name is reserved.
///
/// Reserved commands cannot be overridden by agents and are always
/// handled by the client.
///
/// # Arguments
///
/// * `name` - The command name to check
///
/// # Returns
///
/// `true` if the command is reserved, `false` otherwise
pub fn is_reserved(name: &str) -> bool {
    RESERVED_COMMANDS.contains(&name)
}

/// Check if a command name is unshadowable.
///
/// Unshadowable commands can NEVER be overridden, even with namespacing.
/// These are essential session control commands.
pub fn is_unshadowable(name: &str) -> bool {
    UNSHADOWABLE_COMMANDS.contains(&name)
}

/// The source of a resolved command
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSource {
    /// Command is handled by the client (built-in)
    Client,
    /// Command is provided by an agent
    Agent,
}

/// Result of command resolution
#[derive(Debug, Clone)]
pub struct CommandResolution {
    /// The resolved command name (may be different from input if namespaced)
    pub name: String,
    /// Where the command comes from
    pub source: CommandSource,
    /// The descriptor for the command
    pub descriptor: CommandDescriptor,
}

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
    /// Agent-provided commands (from ACP)
    agent_commands: Vec<AvailableCommand>,
    /// Mapping of conflicting client commands to namespaced versions
    /// e.g., "search" -> "crucible:search" when agent provides "search"
    namespaced_client_commands: HashMap<String, String>,
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
    /// Static commands are registered at build time and do not change.
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

    /// Check if a command is provided by an agent
    pub fn is_agent_command(&self, name: &str) -> bool {
        self.agent_commands.iter().any(|cmd| cmd.name == name)
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
            agent_commands: self.agent_commands.clone(),
            namespaced_client_commands: self.namespaced_client_commands.clone(),
        }
    }

    /// Create a new registry with agent commands added
    ///
    /// When an agent provides commands that conflict with client commands,
    /// the client commands are namespaced with "crucible:" prefix.
    /// Unshadowable commands (exit, quit, help) are never overridden.
    ///
    /// # Arguments
    ///
    /// * `commands` - Agent-provided commands from ACP
    ///
    /// # Returns
    ///
    /// A new registry with agent commands integrated
    pub fn with_agent_commands(&self, commands: Vec<AvailableCommand>) -> Self {
        let mut namespaced = HashMap::new();

        // Check for conflicts
        for agent_cmd in &commands {
            // If agent provides a command that matches a client command
            if self.static_names.contains(&agent_cmd.name) {
                // Unshadowable commands are never namespaced - agent loses
                if !is_unshadowable(&agent_cmd.name) {
                    // Namespace the client command
                    let namespaced_name = format!("crucible:{}", agent_cmd.name);
                    namespaced.insert(agent_cmd.name.clone(), namespaced_name);
                }
            }
        }

        Self {
            commands: self.commands.clone(),
            static_names: self.static_names.clone(),
            dynamic_descriptors: self.dynamic_descriptors.clone(),
            agent_commands: commands,
            namespaced_client_commands: namespaced,
        }
    }

    /// Resolve a command name to its handler and source
    ///
    /// Resolution order:
    /// 1. Unshadowable commands always resolve to client
    /// 2. Namespaced commands (e.g., "crucible:search") resolve to client
    /// 3. Agent commands take priority over client commands
    /// 4. Client commands are the fallback
    ///
    /// # Arguments
    ///
    /// * `name` - The command name to resolve (e.g., "search" or "crucible:search")
    ///
    /// # Returns
    ///
    /// `Some(CommandResolution)` if the command exists, `None` otherwise
    pub fn resolve(&self, name: &str) -> Option<CommandResolution> {
        // 1. Unshadowable commands always resolve to client
        if is_unshadowable(name) {
            if let Some(cmd) = self.commands.get(name) {
                return Some(CommandResolution {
                    name: name.to_string(),
                    source: CommandSource::Client,
                    descriptor: cmd.descriptor.clone(),
                });
            }
        }

        // 2. Check for namespaced client command (e.g., "crucible:search")
        if let Some(stripped) = name.strip_prefix("crucible:") {
            if let Some(cmd) = self.commands.get(stripped) {
                return Some(CommandResolution {
                    name: stripped.to_string(),
                    source: CommandSource::Client,
                    descriptor: cmd.descriptor.clone(),
                });
            }
        }

        // 3. Check for agent command
        if let Some(agent_cmd) = self.agent_commands.iter().find(|c| c.name == name) {
            let input_hint = agent_cmd.input.as_ref().map(|input| {
                match input {
                    crucible_core::types::acp::schema::AvailableCommandInput::Unstructured { hint } => hint.clone(),
                }
            });
            return Some(CommandResolution {
                name: name.to_string(),
                source: CommandSource::Agent,
                descriptor: CommandDescriptor {
                    name: agent_cmd.name.clone(),
                    description: agent_cmd.description.clone(),
                    input_hint,
                },
            });
        }

        // 4. Check for client command (not shadowed by agent)
        if let Some(cmd) = self.commands.get(name) {
            // If this command is shadowed by an agent command, it should not resolve
            // (user should use crucible:name to access it)
            if !self.namespaced_client_commands.contains_key(name) {
                return Some(CommandResolution {
                    name: name.to_string(),
                    source: CommandSource::Client,
                    descriptor: cmd.descriptor.clone(),
                });
            }
        }

        None
    }

    /// Get the namespaced name for a client command if it was shadowed
    pub fn get_namespaced_name(&self, name: &str) -> Option<&String> {
        self.namespaced_client_commands.get(name)
    }

    /// List all command descriptors (static + dynamic + agent)
    pub fn list_all(&self) -> Vec<CommandDescriptor> {
        let mut all = Vec::new();

        // Add static commands (with namespaced names if shadowed)
        for name in &self.static_names {
            if let Some(cmd) = self.commands.get(name) {
                let mut desc = cmd.descriptor.clone();
                // If this command is namespaced, show the namespaced name
                if let Some(namespaced) = self.namespaced_client_commands.get(name) {
                    desc.name = namespaced.clone();
                }
                all.push(desc);
            }
        }

        // Add dynamic commands
        all.extend(self.dynamic_descriptors.clone());

        // Add agent commands
        for agent_cmd in &self.agent_commands {
            let input_hint = agent_cmd.input.as_ref().map(|input| {
                match input {
                    crucible_core::types::acp::schema::AvailableCommandInput::Unstructured { hint } => hint.clone(),
                }
            });
            all.push(CommandDescriptor {
                name: agent_cmd.name.clone(),
                description: agent_cmd.description.clone(),
                input_hint,
            });
        }

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
            agent_commands: Vec::new(),
            namespaced_client_commands: HashMap::new(),
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

    // ========================================================================
    // Phase 3: Command Registry Namespacing Tests
    // ========================================================================

    // Task 3.1.1: Reserved command tests
    #[test]
    fn test_is_reserved_returns_true_for_reserved_commands() {
        assert!(is_reserved("exit"));
        assert!(is_reserved("quit"));
        assert!(is_reserved("help"));
        assert!(is_reserved("search"));
        assert!(is_reserved("context"));
        assert!(is_reserved("clear"));
        assert!(is_reserved("mode"));
    }

    #[test]
    fn test_is_reserved_returns_false_for_non_reserved() {
        assert!(!is_reserved("foo"));
        assert!(!is_reserved("bar"));
        assert!(!is_reserved("custom_command"));
        assert!(!is_reserved(""));
    }

    // Task 3.2.1: Conflict handling tests
    #[test]
    fn test_agent_registers_search_client_becomes_namespaced() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("search", handler.clone(), "Client search")
            .build();

        // Agent registers "search" command
        let agent_commands = vec![AvailableCommand {
            name: "search".to_string(),
            description: "Agent search".to_string(),
            input: None,
            meta: None,
        }];

        let registry = registry.with_agent_commands(agent_commands);

        // Client command should be namespaced
        assert_eq!(
            registry.get_namespaced_name("search"),
            Some(&"crucible:search".to_string())
        );
    }

    #[test]
    fn test_agent_registers_foo_no_conflict() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("search", handler.clone(), "Client search")
            .build();

        // Agent registers "foo" command (no conflict)
        let agent_commands = vec![AvailableCommand {
            name: "foo".to_string(),
            description: "Agent foo".to_string(),
            input: None,
            meta: None,
        }];

        let registry = registry.with_agent_commands(agent_commands);

        // No namespacing needed
        assert!(registry.namespaced_client_commands.is_empty());
        assert!(registry.is_agent_command("foo"));
    }

    #[test]
    fn test_unshadowable_commands_always_client_handled() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("exit", handler.clone(), "Client exit")
            .command("quit", handler.clone(), "Client quit")
            .command("help", handler.clone(), "Client help")
            .build();

        // Agent tries to register unshadowable commands
        let agent_commands = vec![
            AvailableCommand {
                name: "exit".to_string(),
                description: "Agent exit".to_string(),
                input: None,
                meta: None,
            },
            AvailableCommand {
                name: "quit".to_string(),
                description: "Agent quit".to_string(),
                input: None,
                meta: None,
            },
            AvailableCommand {
                name: "help".to_string(),
                description: "Agent help".to_string(),
                input: None,
                meta: None,
            },
        ];

        let registry = registry.with_agent_commands(agent_commands);

        // Unshadowable commands should NOT be namespaced
        assert!(registry.get_namespaced_name("exit").is_none());
        assert!(registry.get_namespaced_name("quit").is_none());
        assert!(registry.get_namespaced_name("help").is_none());
    }

    // Task 3.3.1: Command resolution tests
    #[test]
    fn test_search_resolves_to_agent_if_registered() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("search", handler.clone(), "Client search")
            .build();

        // Agent registers "search"
        let agent_commands = vec![AvailableCommand {
            name: "search".to_string(),
            description: "Agent search".to_string(),
            input: None,
            meta: None,
        }];

        let registry = registry.with_agent_commands(agent_commands);

        // "/search" should resolve to agent
        let resolution = registry.resolve("search").unwrap();
        assert_eq!(resolution.source, CommandSource::Agent);
        assert_eq!(resolution.descriptor.description, "Agent search");
    }

    #[test]
    fn test_crucible_search_resolves_to_client() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("search", handler.clone(), "Client search")
            .build();

        // Agent registers "search"
        let agent_commands = vec![AvailableCommand {
            name: "search".to_string(),
            description: "Agent search".to_string(),
            input: None,
            meta: None,
        }];

        let registry = registry.with_agent_commands(agent_commands);

        // "/crucible:search" should resolve to client
        let resolution = registry.resolve("crucible:search").unwrap();
        assert_eq!(resolution.source, CommandSource::Client);
        assert_eq!(resolution.descriptor.description, "Client search");
    }

    #[test]
    fn test_exit_always_resolves_to_client() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("exit", handler.clone(), "Client exit")
            .build();

        // Agent registers "exit"
        let agent_commands = vec![AvailableCommand {
            name: "exit".to_string(),
            description: "Agent exit".to_string(),
            input: None,
            meta: None,
        }];

        let registry = registry.with_agent_commands(agent_commands);

        // "/exit" should ALWAYS resolve to client (unshadowable)
        let resolution = registry.resolve("exit").unwrap();
        assert_eq!(resolution.source, CommandSource::Client);
        assert_eq!(resolution.descriptor.description, "Client exit");
    }

    #[test]
    fn test_resolve_nonexistent_command_returns_none() {
        let handler = Arc::new(MockHandler);
        let registry = SlashCommandRegistryBuilder::default()
            .command("exit", handler.clone(), "Exit")
            .build();

        assert!(registry.resolve("nonexistent").is_none());
    }

    #[test]
    fn test_resolve_agent_only_command() {
        let registry = SlashCommandRegistryBuilder::default().build();

        // Agent provides a command with no client equivalent
        let agent_commands = vec![AvailableCommand {
            name: "agent_only".to_string(),
            description: "Agent only command".to_string(),
            input: None,
            meta: None,
        }];

        let registry = registry.with_agent_commands(agent_commands);

        let resolution = registry.resolve("agent_only").unwrap();
        assert_eq!(resolution.source, CommandSource::Agent);
        assert_eq!(resolution.name, "agent_only");
    }
}
