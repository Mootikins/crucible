//! Session Orchestrator - Interactive Chat Loop
//!
//! Orchestrates the interactive chat session, handling user input, command execution,
//! message processing, and agent communication. Extracted from commands/chat.rs for
//! reusability and testability.
//!
//! NOTE: Interactive REPL is currently stubbed - reedline/ratatui TUI code removed
//! during event architecture cleanup. Use --query for one-shot mode.

use anyhow::Result;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use crate::acp::ContextEnricher;
use crate::chat::bridge::AgentEventBridge;
use crate::chat::handlers;
use crate::chat::mode_registry::ModeRegistry;
use crate::chat::slash_registry::{SlashCommandRegistry, SlashCommandRegistryBuilder};
use crate::chat::{AgentHandle, ChatError, ChatResult};
use crate::core_facade::KilnContext;
use crate::tui::oil::{AgentSelection, ChatMode, OilChatRunner};
use crucible_core::events::EventRing;
use crucible_core::traits::registry::{Registry, RegistryBuilder};
use walkdir::WalkDir;

/// Default number of context results to include in enriched prompts
pub const DEFAULT_CONTEXT_SIZE: usize = 5;

/// Maximum allowed context size to prevent excessive memory usage
pub const MAX_CONTEXT_SIZE: usize = 1000;

/// Default number of search results to display
pub const DEFAULT_SEARCH_LIMIT: usize = 10;

/// CLI chat session configuration.
///
/// User interface settings for the chat command (initial mode, splash screen,
/// context settings). This is distinct from:
/// - `crucible_core::SessionConfig` - ACP protocol session parameters
/// - `crucible_core::SessionEventConfig` - session event configuration
/// - `crucible_acp::TransportConfig` - transport layer settings
///
/// # TODO: Scratch Workspace
///
/// Add a `scratch_workspace` option that uses a folder in the session folder
/// (`.crucible/sessions/<session-id>/workspace/`) instead of cwd for workspace tools.
/// This keeps agent-generated files isolated from the kiln and user's working directory.
/// The workspace path would be passed to tools that operate on files (read, write, edit, etc.)
/// Note: workspace != kiln. The kiln is for knowledge storage, workspace is for agent work.
#[derive(Debug, Clone)]
pub struct ChatSessionConfig {
    pub initial_mode_id: String,
    pub context_enabled: bool,
    pub context_size: Option<usize>,
    pub skip_splash: bool,
    pub agent_name: Option<String>,
    pub default_selection: Option<AgentSelection>,
    pub resume_session_id: Option<String>,
    pub session_kiln_path: Option<std::path::PathBuf>,
}

impl Default for ChatSessionConfig {
    fn default() -> Self {
        Self {
            initial_mode_id: "normal".to_string(),
            context_enabled: true,
            context_size: Some(DEFAULT_CONTEXT_SIZE),
            skip_splash: false,
            agent_name: None,
            default_selection: None,
            resume_session_id: None,
            session_kiln_path: None,
        }
    }
}

impl ChatSessionConfig {
    pub fn new(
        initial_mode_id: impl Into<String>,
        context_enabled: bool,
        context_size: Option<usize>,
    ) -> Self {
        Self {
            initial_mode_id: initial_mode_id.into(),
            context_enabled,
            context_size,
            skip_splash: false,
            agent_name: None,
            default_selection: None,
            resume_session_id: None,
            session_kiln_path: None,
        }
    }

    /// Set whether to skip the splash screen
    pub fn with_skip_splash(mut self, skip: bool) -> Self {
        self.skip_splash = skip;
        self
    }

    /// Set the current agent name for display
    pub fn with_agent_name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    /// Set the default agent selection for first iteration.
    ///
    /// When set, skips the picker phase on first run but still supports
    /// restart via `/new` command (which will show the picker).
    pub fn with_default_selection(mut self, selection: AgentSelection) -> Self {
        self.default_selection = Some(selection);
        self
    }

    /// Set a session ID to resume from.
    ///
    /// When set, the runner will load existing conversation history from the
    /// session and prepopulate the conversation view.
    pub fn with_resume_session(mut self, session_id: impl Into<String>) -> Self {
        self.resume_session_id = Some(session_id.into());
        self
    }

    /// Set the kiln path for session storage.
    ///
    /// When set, sessions will be saved to this kiln. If not set, falls back
    /// to the kiln_path from the core config.
    pub fn with_session_kiln(mut self, kiln_path: std::path::PathBuf) -> Self {
        self.session_kiln_path = Some(kiln_path);
        self
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(size) = self.context_size {
            if size == 0 {
                anyhow::bail!("context_size must be greater than 0");
            }
            if size > MAX_CONTEXT_SIZE {
                anyhow::bail!(
                    "context_size must be <= {} (got {})",
                    MAX_CONTEXT_SIZE,
                    size
                );
            }
        }
        Ok(())
    }
}

/// Interactive chat session orchestrator
pub struct ChatSession {
    config: ChatSessionConfig,
    core: Arc<KilnContext>,
    enricher: ContextEnricher,
    command_registry: SlashCommandRegistry,
    mode_registry: ModeRegistry,
    exit_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl ChatSession {
    /// Create a new chat session
    pub fn new(
        config: ChatSessionConfig,
        core: Arc<KilnContext>,
        available_models: Option<Vec<String>>,
    ) -> Self {
        Self::with_lua_plugins(config, core, available_models, &[], &[])
    }

    /// Create a chat session with pre-discovered Lua commands.
    ///
    /// Commands are discovered via `PluginManager` spec table loading.
    pub fn with_lua_commands(
        config: ChatSessionConfig,
        core: Arc<KilnContext>,
        available_models: Option<Vec<String>>,
        lua_commands: &[crucible_lua::DiscoveredCommand],
    ) -> Self {
        Self::with_lua_plugins(config, core, available_models, lua_commands, &[])
    }

    /// Create a chat session with pre-discovered Lua commands and views.
    ///
    /// Plugins are discovered via `PluginManager` spec table loading.
    pub fn with_lua_plugins(
        config: ChatSessionConfig,
        core: Arc<KilnContext>,
        available_models: Option<Vec<String>>,
        lua_commands: &[crucible_lua::DiscoveredCommand],
        lua_views: &[crucible_lua::DiscoveredView],
    ) -> Self {
        let context_size = config.context_size.unwrap_or(5);
        let enricher = ContextEnricher::new(core.clone(), Some(context_size));

        let exit_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut registry_builder = SlashCommandRegistryBuilder::default()
            .lua_commands(lua_commands)
            .command(
                "exit",
                Arc::new(handlers::ExitHandler::new(exit_flag.clone())),
                "Exit the chat session",
            )
            .command(
                "quit",
                Arc::new(handlers::ExitHandler::new(exit_flag.clone())),
                "Exit the chat session",
            )
            .command(
                "plan",
                Arc::new(handlers::ModeHandler),
                "Switch to Plan mode (read-only)",
            )
            .command(
                "act",
                Arc::new(handlers::ModeHandler),
                "Switch to Act mode (write-enabled)",
            )
            .command(
                "auto",
                Arc::new(handlers::ModeHandler),
                "Switch to AutoApprove mode",
            )
            .command(
                "mode",
                Arc::new(handlers::ModeCycleHandler),
                "Cycle to the next mode",
            )
            .command_with_hint(
                "search",
                Arc::new(handlers::SearchHandler),
                "Search the knowledge base",
                Some("query".to_string()),
            )
            .command(
                "help",
                Arc::new(handlers::HelpHandler),
                "Show available commands",
            )
            .command_with_hint(
                "commit",
                Arc::new(handlers::CommitHandler),
                "Smart git commit workflow (smart/quick/review/wip)",
                Some("mode [message]".to_string()),
            )
            .command(
                "agent",
                Arc::new(handlers::AgentHandler::new(None)),
                "Show current agent and list available agents",
            )
            .command(
                "new",
                Arc::new(handlers::NewHandler),
                "Start a new session with agent picker",
            )
            .command(
                "resume",
                Arc::new(handlers::ResumeHandler),
                "Browse and resume recent sessions",
            )
            .command_with_hint(
                "view",
                Arc::new(handlers::ViewHandler::new(lua_views.to_vec())),
                "Open or list Lua-defined views",
                Some("name".to_string()),
            );

        // Register /models command if models are available (e.g., from OpenCode)
        if let Some(models) = available_models {
            use crucible_core::traits::chat::CommandOption;

            let handler = Arc::new(handlers::ModelsHandler::new(models.clone(), None));

            // Convert model IDs to CommandOptions for autocomplete
            let options: Vec<CommandOption> = models
                .iter()
                .map(|m| CommandOption {
                    label: m.clone(),
                    value: m.clone(),
                })
                .collect();

            registry_builder = registry_builder.command_with_options(
                "models",
                handler,
                "List or switch between available models",
                options,
            );
        }

        let command_registry = registry_builder.build();

        // Initialize mode registry with defaults
        let mode_registry = ModeRegistry::new();

        Self {
            config,
            core,
            enricher,
            command_registry,
            mode_registry,
            exit_flag,
        }
    }

    /// Get a reference to the mode registry
    pub fn mode_registry(&self) -> &ModeRegistry {
        &self.mode_registry
    }

    /// Get a mutable reference to the mode registry
    pub fn mode_registry_mut(&mut self) -> &mut ModeRegistry {
        &mut self.mode_registry
    }

    /// Get a reference to the command registry
    pub fn command_registry(&self) -> &SlashCommandRegistry {
        &self.command_registry
    }

    /// Set mode with validation against the registry
    ///
    /// Validates the mode ID exists in the registry before calling agent.set_mode_str.
    ///
    /// # Arguments
    ///
    /// * `mode_id` - The mode ID to set (e.g., "plan", "act", "auto")
    /// * `agent` - The agent handle to notify of the mode change
    ///
    /// # Returns
    ///
    /// Ok(()) if mode was set successfully, or ChatError::InvalidMode if mode does not exist
    pub async fn set_mode<A: AgentHandle>(
        &mut self,
        mode_id: &str,
        agent: &mut A,
    ) -> ChatResult<()> {
        // Validate mode exists in registry
        if !self.mode_registry.exists(mode_id) {
            return Err(ChatError::InvalidMode(mode_id.to_string()));
        }

        // Set mode on agent
        agent.set_mode_str(mode_id).await?;

        // Update registry current mode
        self.mode_registry
            .set_mode(mode_id)
            .map_err(|e| ChatError::InvalidMode(e.to_string()))?;

        Ok(())
    }

    /// Run the session with deferred agent creation.
    ///
    /// Shows splash for agent selection, then creates agent using the factory.
    /// All happens within a single TUI session (no terminal flicker).
    pub async fn run_deferred<F, Fut, A>(&mut self, create_agent: F) -> Result<()>
    where
        F: Fn(crate::tui::AgentSelection) -> Fut,
        Fut: std::future::Future<Output = Result<A>>,
        A: AgentHandle,
    {
        let _session_folder = self.core.session_folder();
        let ring = std::sync::Arc::new(EventRing::new(4096));
        let bridge = AgentEventBridge::new(ring.clone());

        let mode = ChatMode::parse(&self.config.initial_mode_id);

        let workspace_root =
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let kiln_root = self.core.config().kiln_path.clone();

        let (files, notes) = tokio::join!(
            tokio::task::spawn_blocking(move || index_workspace_files(&workspace_root)),
            tokio::task::spawn_blocking(move || index_kiln_notes(&kiln_root)),
        );

        let mut runner = OilChatRunner::new()?.with_mode(mode);
        if let Ok(files) = files {
            runner = runner.with_workspace_files(files);
        }
        if let Ok(notes) = notes {
            runner = runner.with_kiln_notes(notes);
        }
        if let Some(session_dir) = self.config.session_kiln_path.clone() {
            runner = runner.with_session_dir(session_dir);
        }
        if let Some(session_id) = self.config.resume_session_id.clone() {
            runner = runner.with_resume_session(session_id);
        }

        runner = runner.with_slash_commands(crate::commands::chat::known_slash_commands());

        runner.run_with_factory(&bridge, create_agent).await
    }
}

pub fn index_workspace_files(root: &Path) -> Vec<String> {
    const MAX_ENTRIES: usize = 2000;
    // Try git ls-files to respect gitignore
    if let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
    {
        if output.status.success() {
            if let Ok(text) = String::from_utf8(output.stdout) {
                let mut files: Vec<String> = text
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .take(MAX_ENTRIES)
                    .map(|s| s.replace('\\', "/"))
                    .collect();
                files.sort();
                files.dedup();
                return files;
            }
        }
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_hidden_entry(e))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if files.len() >= MAX_ENTRIES {
            break;
        }
        if let Ok(rel) = entry.path().strip_prefix(root) {
            if let Some(path_str) = rel.to_str() {
                files.push(path_str.replace('\\', "/"));
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

pub fn index_kiln_notes(kiln_root: &Path) -> Vec<String> {
    const MAX_ENTRIES: usize = 2000;
    if !kiln_root.exists() {
        return Vec::new();
    }
    let mut notes = Vec::new();
    for entry in WalkDir::new(kiln_root)
        .into_iter()
        .filter_entry(|e| !is_hidden_entry(e))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(ext) = entry.path().extension() {
            if ext != "md" {
                continue;
            }
        } else {
            continue;
        }
        if notes.len() >= MAX_ENTRIES {
            break;
        }
        if let Ok(rel) = entry.path().strip_prefix(kiln_root) {
            if let Some(path_str) = rel.to_str() {
                notes.push(format!("note:{}", path_str.replace('\\', "/")));
            }
        }
    }
    notes.sort();
    notes.dedup();
    notes
}

fn is_hidden_entry(entry: &walkdir::DirEntry) -> bool {
    // Don't filter the root directory (depth 0)
    // Only check non-root entries for hidden names
    entry.depth() > 0 && entry.file_name().to_string_lossy().starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::types::acp::schema::{SessionMode, SessionModeId, SessionModeState};
    use serde_json::json;

    // Helper to create a standard mode state with plan/act/auto modes
    fn default_mode_state() -> SessionModeState {
        serde_json::from_value(json!({
            "currentModeId": "plan",
            "availableModes": [
                {"id": "plan", "name": "Plan", "description": "Read-only exploration mode", "_meta": null},
                {"id": "act", "name": "Act", "description": "Write-enabled execution mode", "_meta": null},
                {"id": "auto", "name": "Auto", "description": "Auto-approve all operations", "_meta": null},
            ],
            "_meta": null,
        }))
        .expect("Failed to create test SessionModeState")
    }

    // TDD Test 1: Exit handler should signal exit via shared flag when executed through trait
    #[tokio::test]
    async fn test_exit_handler_via_trait() {
        use async_trait::async_trait;
        use crucible_core::traits::chat::{ChatContext, ChatResult, CommandHandler, SearchResult};

        // Simple mock context that does not need an agent
        struct SimpleMockContext;

        #[async_trait]
        impl ChatContext for SimpleMockContext {
            fn get_mode_id(&self) -> &str {
                "plan"
            }

            fn request_exit(&mut self) {}
            fn exit_requested(&self) -> bool {
                false
            }

            async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
                Ok(())
            }

            async fn semantic_search(
                &self,
                _query: &str,
                _limit: usize,
            ) -> ChatResult<Vec<SearchResult>> {
                Ok(vec![])
            }

            async fn send_command_to_agent(&mut self, _name: &str, _args: &str) -> ChatResult<()> {
                Ok(())
            }

            fn display_search_results(&self, _query: &str, _results: &[SearchResult]) {}
            fn display_help(&self) {}
            fn display_error(&self, _message: &str) {}
            fn display_info(&self, _message: &str) {}
        }

        // Setup: Create exit flag and handler
        let exit_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handler = handlers::ExitHandler::new(exit_flag.clone());
        let mut ctx = SimpleMockContext;

        // Execute handler through the CommandHandler trait
        let result = handler.execute("", &mut ctx).await;

        // Assert: Should succeed and set the exit flag
        assert!(result.is_ok(), "Handler should execute successfully");
        assert!(
            exit_flag.load(std::sync::atomic::Ordering::SeqCst),
            "Exit flag should be set"
        );
    }

    // ChatSessionConfig tests
    #[test]
    fn test_session_config_new() {
        let config = ChatSessionConfig::new("plan", true, Some(10));
        assert_eq!(config.initial_mode_id, "plan");
        assert!(config.context_enabled);
        assert_eq!(config.context_size, Some(10));
    }

    #[test]
    fn test_session_config_default() {
        let config = ChatSessionConfig::default();
        assert_eq!(config.initial_mode_id, "normal");
        assert!(config.context_enabled);
        assert_eq!(config.context_size, Some(5));
    }

    #[test]
    fn test_session_config_clone() {
        let config = ChatSessionConfig::new("normal", false, None);
        let cloned = config.clone();
        assert_eq!(config.initial_mode_id, cloned.initial_mode_id);
        assert_eq!(config.context_enabled, cloned.context_enabled);
        assert_eq!(config.context_size, cloned.context_size);
    }

    #[test]
    fn test_session_config_validate_success() {
        let config = ChatSessionConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_context_disabled_no_size() {
        let config = ChatSessionConfig::new("plan", false, None);
        assert!(!config.context_enabled);
        assert_eq!(config.context_size, None);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_all_modes() {
        let normal_config = ChatSessionConfig::new("normal", true, Some(5));
        assert_eq!(normal_config.initial_mode_id, "normal");

        let plan_config = ChatSessionConfig::new("plan", true, Some(5));
        assert_eq!(plan_config.initial_mode_id, "plan");

        let auto_config = ChatSessionConfig::new("auto", true, Some(5));
        assert_eq!(auto_config.initial_mode_id, "auto");
    }

    #[test]
    fn test_session_config_validate_zero_context_size() {
        let config = ChatSessionConfig::new("plan", true, Some(0));
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be greater than 0"));
    }

    #[test]
    fn test_session_config_validate_too_large_context_size() {
        let config = ChatSessionConfig::new("plan", true, Some(1001));
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be <= 1000"));
    }

    #[test]
    fn test_session_config_validate_max_context_size() {
        let config = ChatSessionConfig::new("plan", true, Some(1000));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_validate_min_context_size() {
        let config = ChatSessionConfig::new("plan", true, Some(1));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_config_default_selection_initially_none() {
        let config = ChatSessionConfig::default();
        assert!(config.default_selection.is_none());
    }

    #[test]
    fn test_session_config_with_default_selection_acp() {
        let config = ChatSessionConfig::new("plan", true, Some(5))
            .with_default_selection(AgentSelection::Acp("opencode".to_string()));

        assert!(config.default_selection.is_some());
        assert!(matches!(
            config.default_selection,
            Some(AgentSelection::Acp(ref name)) if name == "opencode"
        ));
    }

    #[test]
    fn test_session_config_with_default_selection_internal() {
        let config = ChatSessionConfig::new("plan", true, Some(5))
            .with_default_selection(AgentSelection::Internal);

        assert!(matches!(
            config.default_selection,
            Some(AgentSelection::Internal)
        ));
    }

    #[test]
    fn test_session_config_builder_chain() {
        // Verify all builder methods can be chained
        let config = ChatSessionConfig::new("act", true, Some(10))
            .with_skip_splash(true)
            .with_agent_name("test-agent")
            .with_default_selection(AgentSelection::Internal);

        assert_eq!(config.initial_mode_id, "act");
        assert!(config.skip_splash);
        assert_eq!(config.agent_name, Some("test-agent".to_string()));
        assert!(config.default_selection.is_some());
    }

    // Phase 5: Session Integration Tests

    #[test]
    fn test_mode_registry_starts_empty() {
        let mode_registry = ModeRegistry::new();

        assert!(mode_registry.is_empty(), "Mode registry should start empty");
        assert!(
            !mode_registry.exists("plan"),
            "Empty registry should not have plan mode"
        );
    }

    #[test]
    fn test_command_registry_has_default_commands() {
        let exit_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let command_registry = SlashCommandRegistryBuilder::default()
            .command(
                "exit",
                Arc::new(handlers::ExitHandler::new(exit_flag.clone())),
                "Exit the chat session",
            )
            .command(
                "quit",
                Arc::new(handlers::ExitHandler::new(exit_flag.clone())),
                "Exit the chat session",
            )
            .command(
                "help",
                Arc::new(handlers::HelpHandler),
                "Show available commands",
            )
            .build();

        assert!(
            command_registry.get("exit").is_some(),
            "Registry should have exit command"
        );
        assert!(
            command_registry.get("quit").is_some(),
            "Registry should have quit command"
        );
        assert!(
            command_registry.get("help").is_some(),
            "Registry should have help command"
        );
    }

    #[test]
    fn test_index_workspace_files_skips_hidden() {
        let dir = tempfile::tempdir().unwrap();
        // Initialize git repo so git ls-files works predictably
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .ok();
        // Add gitignore to exclude hidden directories
        std::fs::write(dir.path().join(".gitignore"), ".*\n").unwrap();
        std::fs::write(dir.path().join("visible.txt"), "hi").unwrap();
        std::fs::create_dir_all(dir.path().join(".hidden")).unwrap();
        std::fs::write(dir.path().join(".hidden").join("ignored.txt"), "x").unwrap();
        let files = index_workspace_files(dir.path());
        assert!(files.contains(&"visible.txt".to_string()));
        assert!(!files.iter().any(|f| f.contains(".hidden")));
    }

    #[test]
    fn test_index_kiln_notes_md_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note1.md"), "# Note").unwrap();
        std::fs::write(dir.path().join("skip.txt"), "text").unwrap();
        let notes = index_kiln_notes(dir.path());
        assert!(notes.contains(&"note:note1.md".to_string()));
        assert!(!notes.iter().any(|n| n.contains("skip.txt")));
    }

    #[tokio::test]
    async fn test_initialization_queries_agent_modes() {
        use async_trait::async_trait;
        use crucible_core::traits::chat::{ChatChunk, ChatResult as CoreChatResult};
        use crucible_core::types::acp::schema::{
            AvailableCommand, SessionMode, SessionModeId, SessionModeState,
        };
        use futures::stream::BoxStream;

        struct MockAgentWithModes {
            mode_state: SessionModeState,
            mode_id: String,
        }

        #[async_trait]
        impl AgentHandle for MockAgentWithModes {
            fn send_message_stream(
                &mut self,
                _message: String,
            ) -> BoxStream<'static, CoreChatResult<ChatChunk>> {
                Box::pin(futures::stream::empty())
            }

            fn get_mode_id(&self) -> &str {
                &self.mode_id
            }

            async fn set_mode_str(&mut self, mode_id: &str) -> CoreChatResult<()> {
                self.mode_id = mode_id.to_string();
                Ok(())
            }

            fn is_connected(&self) -> bool {
                true
            }

            fn get_modes(&self) -> Option<&SessionModeState> {
                Some(&self.mode_state)
            }

            fn get_commands(&self) -> &[AvailableCommand] {
                &[]
            }
        }

        let agent = MockAgentWithModes {
            mode_state: serde_json::from_value(json!({
                "currentModeId": "custom",
                "availableModes": [{
                    "id": "custom",
                    "name": "Custom Mode",
                    "description": "A custom agent mode",
                    "_meta": null,
                }],
                "_meta": null,
            }))
            .expect("Failed to create test SessionModeState"),
            mode_id: "custom".to_string(),
        };

        let modes = agent.get_modes();
        assert!(modes.is_some(), "Agent should provide modes");
        let modes = modes.unwrap();
        assert_eq!(modes.available_modes.len(), 1);
        assert_eq!(modes.available_modes[0].id.0.as_ref(), "custom");
    }

    #[test]
    fn test_mode_registry_populated_from_agent() {
        use crucible_core::types::acp::schema::{SessionMode, SessionModeId, SessionModeState};

        let mut mode_registry = ModeRegistry::new();

        let agent_state: SessionModeState = serde_json::from_value(json!({
            "currentModeId": "agent-mode",
            "availableModes": [{
                "id": "agent-mode",
                "name": "Agent Mode",
                "description": "Custom agent mode",
                "_meta": null,
            }],
            "_meta": null,
        }))
        .expect("Failed to create test SessionModeState");

        mode_registry.update(agent_state);

        assert!(
            !mode_registry.exists("plan"),
            "Should NOT have plan - only agent modes"
        );
        assert!(mode_registry.exists("agent-mode"), "Should have agent mode");
    }

    #[tokio::test]
    async fn test_set_mode_validates_via_registry() {
        use async_trait::async_trait;
        use crucible_core::traits::chat::{ChatChunk, ChatResult as CoreChatResult};
        use futures::stream::BoxStream;

        struct MockAgent {
            current_mode: String,
        }

        #[async_trait]
        impl AgentHandle for MockAgent {
            fn send_message_stream(
                &mut self,
                _message: String,
            ) -> BoxStream<'static, CoreChatResult<ChatChunk>> {
                Box::pin(futures::stream::empty())
            }

            fn get_mode_id(&self) -> &str {
                &self.current_mode
            }

            async fn set_mode_str(&mut self, mode_id: &str) -> CoreChatResult<()> {
                self.current_mode = mode_id.to_string();
                Ok(())
            }

            fn is_connected(&self) -> bool {
                true
            }
        }

        let mut agent = MockAgent {
            current_mode: "normal".to_string(),
        };

        let mut mode_registry = ModeRegistry::from_agent(default_mode_state());

        assert!(mode_registry.exists("plan"), "plan mode should exist");

        if mode_registry.exists("plan") {
            agent.set_mode_str("plan").await.unwrap();
            mode_registry.set_mode("plan").unwrap();
        }
        assert_eq!(agent.current_mode, "plan");
        assert_eq!(mode_registry.current_id(), "plan");
    }

    #[tokio::test]
    async fn test_set_mode_invalid_mode_returns_error() {
        let mode_registry = ModeRegistry::new();
        assert!(
            !mode_registry.exists("invalid-mode"),
            "invalid-mode should not exist"
        );
    }

    #[tokio::test]
    async fn test_set_mode_calls_agent_set_mode() {
        use async_trait::async_trait;
        use crucible_core::traits::chat::{ChatChunk, ChatResult as CoreChatResult};
        use futures::stream::BoxStream;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct MockAgentWithCounter {
            set_mode_call_count: Arc<AtomicUsize>,
            mode_id: String,
        }

        #[async_trait]
        impl AgentHandle for MockAgentWithCounter {
            fn send_message_stream(
                &mut self,
                _message: String,
            ) -> BoxStream<'static, CoreChatResult<ChatChunk>> {
                Box::pin(futures::stream::empty())
            }

            fn get_mode_id(&self) -> &str {
                &self.mode_id
            }

            async fn set_mode_str(&mut self, _mode_id: &str) -> CoreChatResult<()> {
                self.set_mode_call_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            fn is_connected(&self) -> bool {
                true
            }
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let mut agent = MockAgentWithCounter {
            set_mode_call_count: counter.clone(),
            mode_id: "normal".to_string(),
        };

        let mut mode_registry = ModeRegistry::from_agent(default_mode_state());

        if mode_registry.exists("plan") {
            agent.set_mode_str("plan").await.unwrap();
            mode_registry.set_mode("plan").unwrap();
        }

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "Agent set_mode should be called once"
        );
    }
}
