//! Built-in slash command handlers
//!
//! This module provides the core CLI command handlers:
//! - `ExitHandler`: Exit the chat session
//! - `ModeHandler`: Switch to a specific mode (plan/act/auto)
//! - `ModeCycleHandler`: Cycle to the next mode
//! - `SearchHandler`: Perform semantic search on the knowledge base
//! - `HelpHandler`: Display available commands
//! - `CommitHandler`: Smart git commit workflow
//!
//! ## Architecture
//!
//! Each handler implements the `CommandHandler` trait from crucible-core.
//! Handlers are stateless and receive a `ChatContext` for accessing state.

use async_trait::async_trait;
use colored::Colorize;
use std::process::Command;
use std::sync::Arc;

use crucible_core::traits::chat::{
    cycle_mode_id, ChatContext, ChatError, ChatResult, CommandHandler, mode_display_name,
};

/// Exit handler - terminates the chat session
///
/// Sets an exit flag in the context to signal the session should end.
pub struct ExitHandler {
    /// Reference to the exit flag
    exit_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl ExitHandler {
    /// Create a new exit handler
    pub fn new(exit_flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { exit_flag }
    }
}

#[async_trait]
impl CommandHandler for ExitHandler {
    async fn execute(&self, _args: &str, _ctx: &mut dyn ChatContext) -> ChatResult<()> {
        self.exit_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        println!("{}", "Exiting chat session...".bright_cyan());
        Ok(())
    }
}

/// Mode handler - switches to a specific mode
///
/// Accepts mode name as argument: plan, act, or auto
pub struct ModeHandler;

impl ModeHandler {
    /// Validate mode ID
    fn validate_mode(args: &str) -> ChatResult<&'static str> {
        let mode_str = args.trim().to_lowercase();
        match mode_str.as_str() {
            "plan" => Ok("plan"),
            "act" => Ok("act"),
            "auto" | "autoapprove" => Ok("auto"),
            "" => Err(ChatError::InvalidInput(
                "Mode required. Usage: /mode <plan|act|auto>".to_string(),
            )),
            _ => Err(ChatError::InvalidInput(format!(
                "Invalid mode '{}'. Valid modes: plan, act, auto",
                mode_str
            ))),
        }
    }
}

#[async_trait]
impl CommandHandler for ModeHandler {
    async fn execute(&self, args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()> {
        let new_mode_id = Self::validate_mode(args)?;
        let current_mode_id = ctx.get_mode_id();

        if new_mode_id == current_mode_id {
            ctx.display_info(&format!("Already in {} mode", mode_display_name(new_mode_id)));
        } else {
            // Actually perform the mode change via context
            // This will also display the mode change notification
            ctx.set_mode_str(new_mode_id).await?;
        }

        Ok(())
    }
}

/// Mode cycle handler - cycles to the next mode
///
/// Cycles through: Plan -> Act -> AutoApprove -> Plan
pub struct ModeCycleHandler;

#[async_trait]
impl CommandHandler for ModeCycleHandler {
    async fn execute(&self, _args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()> {
        let current_mode_id = ctx.get_mode_id();
        let new_mode_id = cycle_mode_id(current_mode_id);

        // Actually perform the mode change via context
        // This will also display the mode change notification
        ctx.set_mode_str(new_mode_id).await?;

        Ok(())
    }
}

/// Search handler - performs semantic search on the knowledge base
///
/// Accepts search query as argument, displays results
pub struct SearchHandler;

impl SearchHandler {
    /// Default number of search results
    const DEFAULT_LIMIT: usize = 10;

    /// Format search results for display
    fn format_results(results: Vec<crucible_core::traits::chat::SearchResult>) -> String {
        if results.is_empty() {
            return "No results found.".bright_yellow().to_string();
        }

        let mut output = String::new();
        output.push_str(&format!("{}
", "Search Results:".bright_cyan().bold()));

        for (i, result) in results.iter().enumerate() {
            output.push_str(&format!(
                "
{}. {} {}
",
                i + 1,
                result.title.bright_white().bold(),
                format!("(similarity: {:.2})", result.similarity).dimmed()
            ));
            output.push_str(&format!("   {}
", result.snippet.dimmed()));
        }

        output
    }
}

#[async_trait]
impl CommandHandler for SearchHandler {
    async fn execute(&self, args: &str, ctx: &mut dyn ChatContext) -> ChatResult<()> {
        let query = args.trim();

        if query.is_empty() {
            return Err(ChatError::InvalidInput(
                "Search query required. Usage: /search <query>".to_string(),
            ));
        }

        println!("{}", format!("Searching for: {}", query).bright_cyan());

        let results = ctx.semantic_search(query, Self::DEFAULT_LIMIT).await?;
        println!("{}", Self::format_results(results));

        Ok(())
    }
}

/// Help handler - displays available commands
///
/// Shows all static and dynamic commands with descriptions
pub struct HelpHandler;

impl HelpHandler {
    // Note: This method is reserved for future integration with CommandRegistry
    // Currently using inline display in execute()
    #[allow(dead_code)]
    fn format_commands(commands: Vec<crucible_core::traits::chat::CommandDescriptor>) -> String {
        if commands.is_empty() {
            return "No commands available.".bright_yellow().to_string();
        }

        let mut output = String::new();
        output.push_str(&format!("{}
", "Available Commands:".bright_cyan().bold()));

        for cmd in commands {
            let hint = cmd
                .input_hint
                .map(|h| format!(" <{}>", h))
                .unwrap_or_default();

            output.push_str(&format!(
                "
  /{}{}
",
                cmd.name.bright_white().bold(),
                hint.dimmed()
            ));
            output.push_str(&format!("    {}
", cmd.description.dimmed()));
        }

        output
    }
}

#[async_trait]
impl CommandHandler for HelpHandler {
    async fn execute(&self, _args: &str, _ctx: &mut dyn ChatContext) -> ChatResult<()> {
        // This requires CommandRegistry integration to list commands
        // For now, we'll just print a placeholder
        println!("{}", "Help: Available commands".bright_cyan().bold());
        println!("  {}", "/exit - Exit the chat session".dimmed());
        println!(
            "  {}",
            "/mode <plan|act|auto> - Switch to a specific mode".dimmed()
        );
        println!("  {}", "/cycle - Cycle to the next mode".dimmed());
        println!(
            "  {}",
            "/search <query> - Search the knowledge base".dimmed()
        );
        println!(
            "  {}",
            "/commit [mode] [message] - Smart git commit workflow".dimmed()
        );
        println!(
            "    {}",
            "  smart (default) - Analyze staged changes and suggest message".dimmed()
        );
        println!(
            "    {}",
            "  quick <message> - Fast commit with message".dimmed()
        );
        println!(
            "    {}",
            "  review - Show changes summary and suggestions".dimmed()
        );
        println!(
            "    {}",
            "  wip - Work in progress commit with auto-staging".dimmed()
        );
        println!("  {}", "/help - Show this help message".dimmed());

        Ok(())
    }
}

/// Commit handler - smart git commit workflow
///
/// Supports multiple modes:
/// - Smart (default): Analyze staged changes and suggest commit message
/// - Quick: Fast commit with provided message
/// - Review: Show changes summary and suggest message options
/// - WIP: Create work-in-progress commit with brief summary
pub struct CommitHandler;

impl CommitHandler {
    /// Execute a git command and return output
    fn run_git_command(args: &[&str]) -> ChatResult<String> {
        let output = Command::new("git")
            .args(args)
            .output()
            .map_err(|e| ChatError::Internal(format!("Failed to execute git command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ChatError::CommandFailed(format!(
                "Git command failed: {}",
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Check if we're in a git repository
    fn is_git_repo() -> bool {
        Command::new("git")
            .args(&["rev-parse", "--git-dir"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Get staged files
    fn get_staged_files() -> ChatResult<Vec<String>> {
        let output = Self::run_git_command(&["diff", "--cached", "--name-only"])?;
        Ok(output
            .lines()
            .filter(|l| !l.is_empty())
            .map(|s| s.to_string())
            .collect())
    }

    /// Get diff of staged changes
    fn get_staged_diff() -> ChatResult<String> {
        Self::run_git_command(&["diff", "--cached"])
    }

    /// Get short status
    fn get_status() -> ChatResult<String> {
        Self::run_git_command(&["status", "--short"])
    }

    /// Analyze staged changes and suggest commit type
    fn analyze_changes(files: &[String], diff: &str) -> (String, String) {
        let mut commit_type = "chore";
        let description;

        // Analyze file patterns
        let _has_rust = files.iter().any(|f| f.ends_with(".rs"));
        let has_tests = files
            .iter()
            .any(|f| f.contains("test") || f.contains("spec"));
        let has_docs = files
            .iter()
            .any(|f| f.ends_with(".md") || f.ends_with(".txt") || f.contains("doc"));
        let has_config = files.iter().any(|f| {
            f.contains("Cargo.toml")
                || f.contains("justfile")
                || f.contains(".config")
                || f.contains("config")
        });

        // Analyze diff content
        let diff_lower = diff.to_lowercase();
        let has_fix = diff_lower.contains("fix")
            || diff_lower.contains("bug")
            || diff_lower.contains("error");
        let has_feat = diff_lower.contains("add")
            || diff_lower.contains("implement")
            || diff_lower.contains("new");
        let has_refactor = diff_lower.contains("refactor")
            || diff_lower.contains("cleanup")
            || diff_lower.contains("simplify");

        // Determine commit type
        if has_fix {
            commit_type = "fix";
        } else if has_feat && !has_tests {
            commit_type = "feat";
        } else if has_tests {
            commit_type = "test";
        } else if has_docs {
            commit_type = "docs";
        } else if has_refactor {
            commit_type = "refactor";
        } else if has_config {
            commit_type = "chore";
        }

        // Generate description from file names
        if files.len() == 1 {
            let file = &files[0];
            let name = file.split('/').last().unwrap_or(file);
            description = format!("update {}", name);
        } else if files.len() <= 3 {
            description = format!("update {} files", files.len());
        } else {
            description = format!("update {} files", files.len());
        }

        (commit_type.to_string(), description)
    }

    /// Suggest commit message based on changes
    fn suggest_commit_message(files: &[String], diff: &str) -> Vec<String> {
        let (commit_type, description) = Self::analyze_changes(files, diff);
        let mut suggestions = vec![];

        // Primary suggestion
        suggestions.push(format!("{}: {}", commit_type, description));

        // Additional suggestions based on file patterns
        if files.iter().any(|f| f.contains("test")) {
            suggestions.push(format!("test: add tests for {}", description));
        }
        if files
            .iter()
            .any(|f| f.ends_with(".rs") && !f.contains("test"))
        {
            suggestions.push(format!("{}: improve {}", commit_type, description));
        }

        suggestions
    }

    /// Smart commit mode - analyze and suggest
    async fn smart_mode(&self, message: Option<&str>) -> ChatResult<()> {
        if !Self::is_git_repo() {
            return Err(ChatError::InvalidInput(
                "Not in a git repository".to_string(),
            ));
        }

        let staged_files = Self::get_staged_files()?;

        if staged_files.is_empty() {
            println!("{}", "No staged changes found.".bright_yellow());
            println!("{}", "Staging all modified files...".bright_cyan());
            Self::run_git_command(&["add", "-u"])?;
            let staged_files = Self::get_staged_files()?;
            if staged_files.is_empty() {
                return Err(ChatError::InvalidInput("No changes to commit".to_string()));
            }
        }

        let diff = Self::get_staged_diff().unwrap_or_default();
        let suggestions = Self::suggest_commit_message(&staged_files, &diff);

        println!("{}", "
📋 Staged Changes:".bright_cyan().bold());
        for file in &staged_files {
            println!("  {}", file.dimmed());
        }

        println!("
{}", "💡 Suggested Commit Messages:".bright_cyan().bold());
        for (i, suggestion) in suggestions.iter().enumerate() {
            println!("  {}. {}", i + 1, suggestion.bright_white());
        }

        let commit_msg = if let Some(msg) = message {
            msg.to_string()
        } else {
            // Use first suggestion
            suggestions[0].clone()
        };

        println!("
{}", format!("Committing: {}", commit_msg).bright_green());
        Self::run_git_command(&["commit", "-m", &commit_msg])?;
        println!("{}", "✅ Commit created successfully!".bright_green());

        Ok(())
    }

    /// Quick commit mode
    async fn quick_mode(&self, message: &str) -> ChatResult<()> {
        if !Self::is_git_repo() {
            return Err(ChatError::InvalidInput(
                "Not in a git repository".to_string(),
            ));
        }

        if message.trim().is_empty() {
            return Err(ChatError::InvalidInput(
                "Commit message required for quick mode".to_string(),
            ));
        }

        // Check for staged changes
        let staged_files = Self::get_staged_files()?;
        if staged_files.is_empty() {
            println!(
                "{}",
                "No staged changes. Staging all modified files...".bright_cyan()
            );
            Self::run_git_command(&["add", "-u"])?;
        }

        // Basic validation
        if message.len() < 3 {
            return Err(ChatError::InvalidInput(
                "Commit message too short".to_string(),
            ));
        }

        println!("{}", format!("Committing: {}", message).bright_green());
        Self::run_git_command(&["commit", "-m", message])?;
        println!("{}", "✅ Commit created successfully!".bright_green());

        Ok(())
    }

    /// Review mode - show changes and suggest options
    async fn review_mode(&self) -> ChatResult<()> {
        if !Self::is_git_repo() {
            return Err(ChatError::InvalidInput(
                "Not in a git repository".to_string(),
            ));
        }

        let status = Self::get_status()?;
        if status.trim().is_empty() {
            println!("{}", "No changes detected.".bright_yellow());
            return Ok(());
        }

        println!("{}", "
📊 Repository Status:".bright_cyan().bold());
        println!("{}", status);

        let staged_files = Self::get_staged_files()?;
        if staged_files.is_empty() {
            println!("
{}", "No staged changes.".bright_yellow());
            println!(
                "{}",
                "Use 'git add' to stage files, or run '/commit wip' to auto-stage.".dimmed()
            );
            return Ok(());
        }

        let diff = Self::get_staged_diff().unwrap_or_default();
        let suggestions = Self::suggest_commit_message(&staged_files, &diff);

        println!("
{}", "📋 Staged Files:".bright_cyan().bold());
        for file in &staged_files {
            println!("  {}", file.dimmed());
        }

        println!("
{}", "💡 Suggested Commit Messages:".bright_cyan().bold());
        for (i, suggestion) in suggestions.iter().enumerate() {
            println!("  {}. {}", i + 1, suggestion.bright_white());
        }

        println!(
            "
{}",
            "Run '/commit quick \"message\"' to commit with a specific message.".dimmed()
        );
        println!(
            "{}",
            "Or run '/commit' to use the first suggestion.".dimmed()
        );

        Ok(())
    }

    /// WIP mode - work in progress commit
    async fn wip_mode(&self) -> ChatResult<()> {
        if !Self::is_git_repo() {
            return Err(ChatError::InvalidInput(
                "Not in a git repository".to_string(),
            ));
        }

        // Auto-stage modified files
        println!("{}", "Staging modified files...".bright_cyan());
        Self::run_git_command(&["add", "-u"])?;

        let staged_files = Self::get_staged_files()?;
        if staged_files.is_empty() {
            return Err(ChatError::InvalidInput("No changes to commit".to_string()));
        }

        // Create brief summary
        let summary = if staged_files.len() == 1 {
            format!(
                "wip: {}",
                staged_files[0]
                    .split('/')
                    .last()
                    .unwrap_or(&staged_files[0])
            )
        } else {
            format!("wip: {} files", staged_files.len())
        };

        println!("{}", format!("Committing: {}", summary).bright_green());
        Self::run_git_command(&["commit", "-m", &summary])?;
        println!("{}", "✅ WIP commit created!".bright_green());

        Ok(())
    }
}

#[async_trait]
impl CommandHandler for CommitHandler {
    async fn execute(&self, args: &str, _ctx: &mut dyn ChatContext) -> ChatResult<()> {
        let args = args.trim();
        let parts: Vec<&str> = args.splitn(2, ' ').collect();

        match parts[0] {
            "quick" => {
                let message = parts.get(1).unwrap_or(&"").trim();
                if message.is_empty() {
                    return Err(ChatError::InvalidInput(
                        "Usage: /commit quick <message>".to_string(),
                    ));
                }
                self.quick_mode(message).await
            }
            "review" => self.review_mode().await,
            "wip" => self.wip_mode().await,
            "" => {
                // Smart mode (default) - can have optional message
                let message = parts.get(1).copied();
                self.smart_mode(message).await
            }
            _ => {
                // Treat as smart mode with message
                self.smart_mode(Some(args)).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::traits::chat::SearchResult;
    use std::sync::atomic::AtomicBool;

    // Mock ChatContext for testing
    struct MockContext {
        mode_id: String,
        search_results: Vec<SearchResult>,
    }

    #[async_trait]
    impl ChatContext for MockContext {
        fn get_mode_id(&self) -> &str {
            &self.mode_id
        }

        async fn semantic_search(
            &self,
            _query: &str,
            _limit: usize,
        ) -> ChatResult<Vec<SearchResult>> {
            Ok(self.search_results.clone())
        }

        async fn send_command_to_agent(&mut self, _name: &str, _args: &str) -> ChatResult<()> {
            Ok(())
        }

        fn request_exit(&mut self) {}
        fn exit_requested(&self) -> bool {
            false
        }
        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }
        fn display_search_results(&self, _query: &str, _results: &[SearchResult]) {}
        fn display_help(&self) {}
        fn display_error(&self, _message: &str) {}
        fn display_info(&self, _message: &str) {}
    }

    #[tokio::test]
    async fn test_exit_handler_sets_flag() {
        let exit_flag = Arc::new(AtomicBool::new(false));
        let handler = ExitHandler::new(exit_flag.clone());
        let mut ctx = MockContext {
            mode_id: "plan".to_string(),
            search_results: vec![],
        };

        let result = handler.execute("", &mut ctx).await;
        assert!(result.is_ok());
        assert!(exit_flag.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_mode_handler_validates_modes() {
        assert_eq!(ModeHandler::validate_mode("plan").unwrap(), "plan");
        assert_eq!(ModeHandler::validate_mode("act").unwrap(), "act");
        assert_eq!(ModeHandler::validate_mode("auto").unwrap(), "auto");
        assert_eq!(ModeHandler::validate_mode("autoapprove").unwrap(), "auto");
    }

    #[tokio::test]
    async fn test_mode_handler_rejects_invalid_mode() {
        assert!(ModeHandler::validate_mode("invalid").is_err());
        assert!(ModeHandler::validate_mode("").is_err());
    }

    #[tokio::test]
    async fn test_mode_handler_executes() {
        let handler = ModeHandler;
        let mut ctx = MockContext {
            mode_id: "plan".to_string(),
            search_results: vec![],
        };

        let result = handler.execute("act", &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mode_cycle_handler_executes() {
        let handler = ModeCycleHandler;
        let mut ctx = MockContext {
            mode_id: "plan".to_string(),
            search_results: vec![],
        };

        let result = handler.execute("", &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_handler_requires_query() {
        let handler = SearchHandler;
        let mut ctx = MockContext {
            mode_id: "plan".to_string(),
            search_results: vec![],
        };

        let result = handler.execute("", &mut ctx).await;
        assert!(matches!(result, Err(ChatError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_search_handler_executes_with_query() {
        let handler = SearchHandler;
        let mut ctx = MockContext {
            mode_id: "plan".to_string(),
            search_results: vec![SearchResult {
                title: "Test Note".to_string(),
                snippet: "Test content".to_string(),
                similarity: 0.95,
            }],
        };

        let result = handler.execute("test query", &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_help_handler_executes() {
        let handler = HelpHandler;
        let mut ctx = MockContext {
            mode_id: "plan".to_string(),
            search_results: vec![],
        };

        let result = handler.execute("", &mut ctx).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_search_format_empty_results() {
        let output = SearchHandler::format_results(vec![]);
        assert!(output.contains("No results found"));
    }

    #[test]
    fn test_search_format_with_results() {
        let results = vec![
            SearchResult {
                title: "Note 1".to_string(),
                snippet: "Content 1".to_string(),
                similarity: 0.95,
            },
            SearchResult {
                title: "Note 2".to_string(),
                snippet: "Content 2".to_string(),
                similarity: 0.85,
            },
        ];

        let output = SearchHandler::format_results(results);
        assert!(output.contains("Note 1"));
        assert!(output.contains("Note 2"));
        assert!(output.contains("Content 1"));
        assert!(output.contains("Content 2"));
    }
}
