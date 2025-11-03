// REPL module for Crucible CLI
//
// This module implements the Read-Eval-Print Loop with two operational modes:
//
// 1. INTERACTIVE MODE (default):
//    - Uses reedline for rich terminal features (history, completion, highlighting)
//    - Requires a TTY (terminal device)
//    - Provides user-friendly line editing with Emacs/Vi keybindings
//    - Method: Repl::run()
//
// 2. NON-INTERACTIVE MODE (--non-interactive flag):
//    - Reads commands from stdin line-by-line
//    - Works without a TTY (pipes, scripts, CI/CD, tests)
//    - Flushes output after each command for immediate availability
//    - Method: Repl::run_non_interactive()
//
// Features (both modes):
// - Built-in commands (`:tools`, `:run`, `:help`, `:quit`)
// - SurrealQL query execution
// - Output formatting (tables, JSON, CSV)
// - Command history (persistent in interactive mode)
// - Tool system for executing registered tools
//
// See MODES.md for detailed documentation on usage and testing.

use anyhow::Result;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{oneshot, watch};
use tracing::{debug, error, info};

use crucible_core::CrucibleCore;
use crucible_surrealdb::{SurrealClient, SurrealDbConfig};

pub mod command;
pub mod completer;
pub mod error;
pub mod formatter;
pub mod highlighter;
pub mod history;
pub mod input;
mod tools; // Stub implementation - Rune tools removed from MVP

use crate::config::CliConfig;
use tools::UnifiedToolRegistry;

use command::Command;
use completer::ReplCompleter;
use error::ReplError;
use formatter::{CsvFormatter, JsonFormatter, OutputFormatter, TableFormatter};
use highlighter::SurrealQLHighlighter;
use history::CommandHistory;
use input::Input;

// Re-export key types for external use (tests, integrations)
pub use command::Command as ReplCommand;
pub use input::Input as ReplInput;

/// REPL state and configuration
pub struct Repl {
    /// Core coordinator (owns database client)
    core: Arc<CrucibleCore>,

    /// Line editor with history and completion
    editor: Reedline,

    /// Tool registry for `:run` commands
    tools: Arc<UnifiedToolRegistry>,

    /// Configuration (for UI settings like history file, format)
    config: ReplConfig,

    /// Current output formatter
    formatter: Box<dyn OutputFormatter>,

    /// Command history manager
    history: CommandHistory,

    /// Graceful shutdown signal
    shutdown_tx: watch::Sender<bool>,

    /// Current query cancellation handle
    current_query_cancel: Option<oneshot::Sender<()>>,

    /// Statistics
    stats: ReplStats,
}

impl Repl {
    /// Create a new REPL instance
    pub async fn new(
        core: Arc<CrucibleCore>,
        cli_config: &CliConfig,
    ) -> Result<Self> {
        info!("Initializing REPL with Core");

        // Create simple REPL config (just UI settings, no DB path)
        let config = ReplConfig::from_cli_config(cli_config)?;

        // Create line editor with history and completion
        let history = CommandHistory::new(config.history_file.clone())?;

        // Setup editor with custom highlighter
        let highlighter = SurrealQLHighlighter::new();

        // Initialize unified tool registry
        let tool_dir = config.tool_dir.clone();
        let tools = UnifiedToolRegistry::new(tool_dir).await?;

        info!("Initialized unified tool registry");

        let tools_arc = Arc::new(tools);

        // Create completer with Core and tools
        let completer = ReplCompleter::new(core.clone(), tools_arc.clone());

        let editor = Reedline::create()
            .with_highlighter(Box::new(highlighter))
            .with_completer(Box::new(completer))
            .with_history(history.clone_backend());

        // Default formatter
        let formatter: Box<dyn OutputFormatter> = match config.default_format.as_str() {
            "json" => Box::new(JsonFormatter::new(true)),
            "csv" => Box::new(CsvFormatter::new()),
            _ => Box::new(TableFormatter::new()),
        };

        // Create shutdown channel
        let (shutdown_tx, _) = watch::channel(false);

        Ok(Self {
            core,
            editor,
            tools: tools_arc,
            config,
            formatter,
            history,
            shutdown_tx,
            current_query_cancel: None,
            stats: ReplStats::default(),
        })
    }

    /// Run the REPL loop (blocks until :quit or Ctrl+D)
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting REPL loop");
        self.print_welcome();

        let prompt = DefaultPrompt::new(
            DefaultPromptSegment::Basic("crucible> ".to_string()),
            DefaultPromptSegment::Empty,
        );

        loop {
            let sig = self.editor.read_line(&prompt);

            match sig {
                Ok(Signal::Success(buffer)) => {
                    // Add to history
                    self.history.add(&buffer);

                    // Process input
                    if let Err(e) = self.process_input(&buffer).await {
                        eprintln!("{}", e.display_pretty());
                    }
                }
                Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                    // Check if we're currently running a query
                    if self.current_query_cancel.is_some() {
                        self.cancel_query();
                    } else {
                        println!("\nUse :quit to exit");
                    }
                }
                Err(err) => {
                    error!("REPL error: {:?}", err);
                    break;
                }
            }
        }

        info!("REPL loop exited");
        Ok(())
    }

    /// Run the REPL in non-interactive mode (reads from stdin line by line)
    /// This is useful for testing and scripting scenarios where a TTY is not available
    pub async fn run_non_interactive(&mut self) -> Result<()> {
        use std::io::{self, BufRead, Write};

        info!("Starting REPL in non-interactive mode");
        self.print_welcome();

        // Flush stdout after welcome message
        io::stdout().flush()?;

        let stdin = io::stdin();
        let reader = stdin.lock();

        for line in reader.lines() {
            let line = line?;

            // Add to history
            self.history.add(&line);

            // Process input
            if let Err(e) = self.process_input(&line).await {
                eprintln!("{}", e.display_pretty());
            }

            // Flush stdout after each command to ensure output is available for reading
            io::stdout().flush()?;

            // Check if we should quit
            if line.trim() == ":quit" || line.trim() == ":q" || line.trim() == ":exit" {
                break;
            }
        }

        info!("REPL non-interactive mode exited");
        Ok(())
    }

    /// Process a single input line
    ///
    /// This method is public to enable direct testing of command processing
    /// logic without spawning a REPL session.
    pub async fn process_input(&mut self, input: &str) -> Result<(), ReplError> {
        let start = Instant::now();

        // Parse input
        let parsed = Input::parse(input)?;

        match parsed {
            Input::Empty => {
                // Do nothing for empty lines
                Ok(())
            }
            Input::Command(cmd) => {
                debug!("Executing command: {:?}", cmd);
                let result = self.execute_command(cmd).await;
                self.stats.command_count += 1;
                result
            }
            Input::Query(query) => {
                debug!("Executing query: {}", query);
                let result = self.execute_query(&query).await;
                self.stats.query_count += 1;
                self.stats.total_query_time += start.elapsed();
                result
            }
        }
    }

    /// Execute a built-in command
    ///
    /// This method is public to enable direct testing of individual command handlers.
    pub async fn execute_command(&mut self, cmd: Command) -> Result<(), ReplError> {
        match cmd {
            Command::ListTools => {
                self.list_tools().await;
                Ok(())
            }
            Command::RunTool { tool_name, args } => self.run_tool(&tool_name, args).await,
            Command::ShowStats => {
                self.show_stats().await;
                Ok(())
            }
            Command::ShowConfig => {
                self.show_config().await;
                Ok(())
            }
            Command::SetLogLevel(level) => {
                self.set_log_level(level);
                Ok(())
            }
            Command::SetFormat(format) => {
                self.set_output_format(format);
                Ok(())
            }
            Command::Help(topic) => match topic {
                Some(topic_str) => {
                    // Check if it's a command or a tool
                    // First, try as a command
                    if Command::help_for_command(&topic_str).is_some() {
                        self.show_help(Some(&topic_str));
                        Ok(())
                    } else {
                        // Try as a tool
                        self.show_tool_help(&topic_str).await
                    }
                }
                None => {
                    // Show general help
                    self.show_help(None);
                    Ok(())
                }
            },
            Command::ShowHistory(limit) => {
                self.show_history(limit);
                Ok(())
            }
            Command::ClearScreen => {
                self.clear_screen();
                Ok(())
            }
            Command::Quit => {
                self.quit();
                Ok(())
            }
        }
    }

    /// Execute a SurrealQL query with async cancellation support
    ///
    /// This method is public to enable direct testing of query execution
    /// without spawning a REPL session.
    pub async fn execute_query(&mut self, query: &str) -> Result<(), ReplError> {
        use indicatif::{ProgressBar, ProgressStyle};

        // Create progress indicator
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("Executing query...");

        // Create cancellation channel
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.current_query_cancel = Some(cancel_tx);

        // Spawn query in background
        let core = self.core.clone();
        let query = query.to_string();
        let query_task = tokio::spawn(async move {
            // Use Core façade method
            core.query(&query)
                .await
                .map(|rows| crate::commands::repl::formatter::QueryResult {
                    rows,
                    duration: std::time::Duration::from_millis(0),
                    affected_rows: None,
                    status: crate::commands::repl::formatter::QueryStatus::Success,
                })
        });

        // Tick progress bar in background
        let pb_clone = pb.clone();
        let ticker = tokio::spawn(async move {
            loop {
                pb_clone.tick();
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });

        // Wait for either completion or cancellation
        let result = tokio::select! {
            result = query_task => {
                self.current_query_cancel = None;
                ticker.abort();
                result.map_err(|e| ReplError::Query(format!("Task error: {}", e)))?
            }
            _ = cancel_rx => {
                self.current_query_cancel = None;
                ticker.abort();
                pb.finish_and_clear();
                println!("\n⚠️  Query cancelled by user");
                return Ok(());
            }
        };

        pb.finish_and_clear();

        // Format and display results
        match result {
            Ok(query_result) => {
                let formatted = self
                    .formatter
                    .format(query_result)
                    .await
                    .map_err(|e| ReplError::Formatting(e.to_string()))?;
                println!("{}", formatted);
                Ok(())
            }
            Err(e) => Err(ReplError::Query(e)),
        }
    }

    /// Cancel currently running query
    fn cancel_query(&mut self) {
        if let Some(cancel_tx) = self.current_query_cancel.take() {
            let _ = cancel_tx.send(());
            println!("\n⚠️  Query cancelled");
        }
    }

    /// List available tools by group
    async fn list_tools(&self) {
        use colored::Colorize;

        let grouped_tools = self.tools.list_tools_by_group().await;

        if grouped_tools.is_empty() {
            println!("\n{} No tools found.\n", "ℹ".blue());
            return;
        }

        let total_tools: usize = grouped_tools.values().map(|v| v.len()).sum();
        println!(
            "\n{} Available Tools ({}) - Grouped by Source:\n",
            "📦".green(),
            total_tools
        );

        for (group_name, tools) in grouped_tools {
            let group_color = match group_name.as_str() {
                "system" => colored::Color::Magenta,
                _ => colored::Color::White,
            };

            println!(
                "  {} ({}) [{} tools]:",
                group_name.to_uppercase().color(group_color),
                match group_name.as_str() {
                    "system" => "crucible-tools",
                    _ => "external tools",
                },
                tools.len()
            );

            // Calculate column width for this group based on longest tool name
            let max_name_len = tools.iter().map(|name| name.len()).max().unwrap_or(0);
            let column_width = max_name_len.max(20); // Minimum 20 chars for alignment

            // Fetch schemas for all tools in this group in parallel
            let schema_futures: Vec<_> = tools
                .iter()
                .map(|tool_name| async {
                    let schema = self.tools.get_tool_schema(tool_name).await.ok().flatten();
                    (tool_name.clone(), schema)
                })
                .collect();

            let tool_schemas = futures::future::join_all(schema_futures).await;

            // Display tools with descriptions
            for (tool_name, schema_opt) in tool_schemas {
                let description = schema_opt
                    .as_ref()
                    .map(|schema| {
                        // Truncate description to 60 chars if needed
                        let desc = &schema.description;
                        if desc.len() > 60 {
                            format!("{}...", &desc[..57])
                        } else {
                            desc.to_string()
                        }
                    })
                    .unwrap_or_else(String::new);

                if !description.is_empty() {
                    println!(
                        "    {:<width$} - {}",
                        tool_name.cyan(),
                        description.white(),
                        width = column_width
                    );
                } else {
                    println!("    {}", tool_name.cyan());
                }
            }
            println!();
        }

        println!(
            "{} :run <tool> [args...] | :help <tool> for details",
            "Tip:".yellow()
        );
        println!();
    }

    /// Run a tool by name
    async fn run_tool(&mut self, tool_name: &str, args: Vec<String>) -> Result<(), ReplError> {
        use colored::Colorize;

        info!("Running tool: {} with args: {:?}", tool_name, args);
        debug!(
            "Tool execution context - name: {}, arg_count: {}",
            tool_name,
            args.len()
        );

        // Execute the tool
        let result = self
            .tools
            .execute_tool(tool_name, &args)
            .await
            .map_err(|e| {
                let err_str = e.to_string();

                // Enhanced error reporting with context
                if err_str.contains("Parameter error:") {
                    // Parameter validation error (from unified_registry)
                    eprintln!(
                        "\n{} {}\n\n{} Tool '{}' parameter requirements:",
                        "❌ Parameter Error:".red().bold(),
                        err_str.strip_prefix("Parameter error: ").unwrap_or(&err_str),
                        "💡".cyan(),
                        tool_name.green()
                    );

                    // Provide tool-specific help
                    if tool_name == "create_note" {
                        eprintln!("  Usage: {}",
                            ":run create_note <path> <title> <content> [properties_json] [tags]".yellow());
                        eprintln!("  Example: {}",
                            ":run create_note /tmp/test.md \"My Note\" \"Content here\" '{{\"type\":\"note\"}}' \"tag1,tag2\"".cyan());
                    } else {
                        eprintln!("  Use {} to see tool details", ":tools".green());
                    }
                } else if err_str.contains("not found or execution failed in all registries") {
                    // Tool not found error (from unified_registry)
                    eprintln!(
                        "\n{} Tool '{}' not found\n\n{} Available tools:",
                        "❌".red(),
                        tool_name.yellow(),
                        "💡".cyan()
                    );
                    eprintln!("  Use {} to list all available tools", ":tools".green());
                    eprintln!("  Use {} to see detailed logs", "RUST_LOG=debug".yellow());
                } else if err_str.contains("Parameter conversion failed:") {
                    // Legacy parameter conversion error
                    let clean_msg = err_str
                        .strip_prefix("Parameter conversion failed: ")
                        .unwrap_or(&err_str);
                    eprintln!(
                        "\n{} {}\n\n{} Check parameter format and count",
                        "❌ Tool Execution Failed:".red().bold(),
                        clean_msg,
                        "💡".cyan()
                    );
                } else {
                    // Generic error with context
                    eprintln!("\n{} Tool '{}' execution failed", "❌".red(), tool_name.yellow());
                    eprintln!("{}\n", err_str);
                    eprintln!("{} Debugging tips:", "💡".cyan());
                    eprintln!("  • Run with {} for detailed logs", "RUST_LOG=debug".yellow());
                    eprintln!("  • Use {} to verify tool availability", ":tools".green());
                    eprintln!("  • Check parameter format and requirements");
                }

                ReplError::Tool(e.to_string())
            })?;

        // Display result based on status
        match result.status {
            tools::ToolStatus::Success => {
                println!("\n{}", result.output);
                Ok(())
            }
            tools::ToolStatus::Error(ref error_msg) => {
                eprintln!("\n{} {}", "❌ Tool Error:".red(), error_msg);
                if !result.output.is_empty() {
                    eprintln!("\nPartial output:\n{}", result.output);
                }
                Err(ReplError::Tool(error_msg.clone()))
            }
        }
    }

    /// Show REPL and database statistics
    async fn show_stats(&self) {
        println!("\n📊 Statistics:\n");
        println!("  Commands executed: {}", self.stats.command_count);
        println!("  Queries executed:  {}", self.stats.query_count);
        println!("  Avg query time:    {:?}", self.stats.avg_query_time());
        println!("  History size:      {}", self.history.len());
        println!(
            "  Tools loaded:      {}",
            self.tools.list_tools().await.len()
        );
        println!();
    }

    /// Show current configuration
    async fn show_config(&self) {
        println!("\n⚙️  Configuration:\n");
        println!("  Kiln path:       {}", self.config.kiln_path.display());
        println!("  Database path:    {}", self.config.db_path.display());
        println!("  History file:     {}", self.config.history_file.display());
        println!("  Output format:    {}", self.config.default_format);
        println!("  Query timeout:    {}s", self.config.query_timeout_secs);
        println!("  Max column width: {}", self.config.max_column_width);

        // Show database statistics
        match self.core.get_stats().await {
            Ok(stats) => {
                println!("  Database stats:");
                for (key, value) in stats {
                    println!("    {}: {}", key, value);
                }
            }
            Err(e) => {
                println!("  Database stats:   Error retrieving stats: {}", e);
            }
        }
        println!();
    }

    /// Set log level
    fn set_log_level(&self, level: tracing::level_filters::LevelFilter) {
        info!("Setting log level to: {:?}", level);
        // TODO: Implement dynamic log level updating
        println!("✓ Log level set to: {:?}", level);
    }

    /// Set output format
    fn set_output_format(&mut self, format: OutputFormat) {
        use colored::Colorize;

        self.formatter = match format {
            OutputFormat::Table => Box::new(TableFormatter::new()),
            OutputFormat::Json => Box::new(JsonFormatter::new(true)),
            OutputFormat::Csv => Box::new(CsvFormatter::new()),
        };

        println!("{} Output format set to: {}", "✓".green(), format.as_str());
    }

    /// Show help information
    fn show_help(&self, topic: Option<&str>) {
        if let Some(topic) = topic {
            // Show help for specific command
            if let Some(help) = Command::help_for_command(topic) {
                println!("\n{}", help);
            } else {
                println!("\n❌ Unknown command: {}", topic);
            }
        } else {
            // Show general help
            println!("\n{}", Command::general_help());
        }
    }

    /// Show detailed help for a specific tool
    async fn show_tool_help(&self, tool_name: &str) -> Result<(), ReplError> {
        use colored::Colorize;

        // Fetch the tool schema
        match self.tools.get_tool_schema(tool_name).await {
            Ok(Some(schema)) => {
                // Format and display the schema
                let formatted = format_tool_schema(&schema);
                println!("{}", formatted);
                Ok(())
            }
            Ok(None) => {
                // Tool exists but has no schema
                println!(
                    "\n{} Tool '{}' found, but no schema information is available.",
                    "ℹ".yellow(),
                    tool_name.cyan()
                );
                Ok(())
            }
            Err(e) => {
                // Tool not found or error retrieving schema
                println!(
                    "\n{} Tool '{}' not found or error retrieving schema: {}",
                    "❌".red(),
                    tool_name.cyan(),
                    e
                );
                println!(
                    "{} Use {} to see all available tools.",
                    "💡".cyan(),
                    ":tools".green()
                );
                Err(ReplError::Tool(format!(
                    "Tool '{}' not found or schema unavailable",
                    tool_name
                )))
            }
        }
    }

    /// Show command history
    fn show_history(&self, limit: Option<usize>) {
        let limit = limit.unwrap_or(20);
        let history = self.history.get_last_n(limit);

        println!("\n📜 Command History (last {}):\n", limit);
        for (i, cmd) in history.iter().enumerate() {
            println!("  {:4} | {}", i + 1, cmd);
        }
        println!();
    }

    /// Clear screen
    fn clear_screen(&self) {
        print!("\x1B[2J\x1B[1;1H");
    }

    /// Quit the REPL
    fn quit(&self) {
        println!("\n👋 Goodbye!");
        let _ = self.shutdown_tx.send(true);
        std::process::exit(0);
    }

    /// Print welcome message
    fn print_welcome(&self) {
        use colored::Colorize;

        println!(
            "\n{}",
            "╔═══════════════════════════════════════════════════╗".cyan()
        );
        println!(
            "{}",
            "║        Crucible CLI REPL v0.1.0                   ║".cyan()
        );
        println!(
            "{}",
            "║   Real SurrealDB-Backed Knowledge Management     ║".cyan()
        );
        println!(
            "{}",
            "╚═══════════════════════════════════════════════════╝".cyan()
        );
        println!(
            "\nType {} for available commands or {} to exit.",
            ":help".green(),
            ":quit".red()
        );
        println!("Connected to real SurrealDB - execute actual queries!\n");
    }

    /// Get access to the tool registry (for testing)
    ///
    /// This is available for both unit tests and integration tests.
    pub fn get_tools(&self) -> &Arc<UnifiedToolRegistry> {
        &self.tools
    }

    /// Get access to the Core (for testing)
    ///
    /// This allows tests to verify database state or perform queries directly.
    /// The Core is the primary interface for all UI/UX interactions.
    pub fn get_core(&self) -> &Arc<CrucibleCore> {
        &self.core
    }

    /// Get the current statistics (for testing)
    ///
    /// This allows tests to verify that commands and queries are being counted correctly.
    pub fn get_stats(&self) -> &ReplStats {
        &self.stats
    }

    /// Create a test REPL instance with in-memory database
    ///
    /// This is a convenience constructor for tests that need a fully functional
    /// REPL without file system dependencies.
    ///
    /// Available in both unit tests and integration tests.
    pub async fn new_test() -> Result<Self> {
        let config_dir = std::env::temp_dir().join("crucible_test");
        std::fs::create_dir_all(&config_dir)?;

        // Use the examples/test-kiln directory as the test kiln
        let test_kiln_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/test-kiln");

        // Create storage with in-memory database
        let storage_config = SurrealDbConfig {
            path: ":memory:".to_string(),
            namespace: "crucible".to_string(),
            database: "test".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };
        let storage = SurrealClient::new(storage_config).await
            .map_err(|e| anyhow::anyhow!("Failed to create test storage: {}", e))?;

        // Initialize kiln schema
        crucible_surrealdb::kiln_integration::initialize_kiln_schema(&storage).await
            .map_err(|e| anyhow::anyhow!("Failed to initialize kiln schema: {}", e))?;

        // Populate database with test-kiln data
        Self::populate_test_data(&storage, &test_kiln_path).await?;

        // Build Core with builder pattern
        let core = Arc::new(
            CrucibleCore::builder()
                .with_storage(storage)
                .build()
                .map_err(|e| anyhow::anyhow!(e))?
        );

        // Create minimal REPL state
        let config = ReplConfig {
            kiln_path: test_kiln_path,
            db_path: std::path::PathBuf::new(), // Unused now
            history_file: config_dir.join("history"),
            tool_dir: config_dir.join("tools"),
            default_format: "table".to_string(),
            query_timeout_secs: 30,
            max_column_width: 50,
        };

        let history = CommandHistory::new(config.history_file.clone())?;
        let highlighter = SurrealQLHighlighter::new();
        let tools = Arc::new(UnifiedToolRegistry::new(config.tool_dir.clone()).await?);

        // Create completer with Core and tools
        let completer = ReplCompleter::new(core.clone(), tools.clone());

        let editor = Reedline::create()
            .with_highlighter(Box::new(highlighter))
            .with_completer(Box::new(completer))
            .with_history(history.clone_backend());

        let formatter: Box<dyn OutputFormatter> = Box::new(TableFormatter::new());
        let (shutdown_tx, _) = watch::channel(false);

        Ok(Self {
            core,
            editor,
            tools,
            config,
            formatter,
            history,
            shutdown_tx,
            current_query_cancel: None,
            stats: ReplStats::default(),
        })
    }

    /// Populate test database with data from test-kiln directory
    async fn populate_test_data(storage: &SurrealClient, kiln_path: &std::path::Path) -> Result<()> {
        use crucible_surrealdb::kiln_integration::store_parsed_document;
        use crucible_core::ParsedDocument;

        // Scan for markdown files in test-kiln
        let mut files = Vec::new();
        if kiln_path.exists() {
            for entry in std::fs::read_dir(kiln_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "md") {
                    files.push(path);
                }
            }
        }

        // Process each file
        for file_path in files {
            let content = std::fs::read_to_string(&file_path)?;

            // Create parsed document
            let mut doc = ParsedDocument::new(file_path.clone());
            doc.content.plain_text = content.clone();
            doc.parsed_at = chrono::Utc::now();
            doc.content_hash = format!("hash_{}", file_path.file_name().unwrap().to_str().unwrap());
            doc.file_size = content.len() as u64;

            // Store document in database
            store_parsed_document(storage, &doc, kiln_path).await
                .map_err(|e| anyhow::anyhow!("Failed to store document {:?}: {}", file_path, e))?;
        }

        Ok(())
    }

}


/// REPL configuration
#[derive(Debug, Clone)]
pub struct ReplConfig {
    pub kiln_path: std::path::PathBuf,
    pub db_path: std::path::PathBuf,
    pub history_file: std::path::PathBuf,
    pub tool_dir: std::path::PathBuf,
    pub default_format: String,
    pub query_timeout_secs: u64,
    pub max_column_width: usize,
}

impl ReplConfig {
    /// Create ReplConfig from CLI config
    ///
    /// This creates a minimal config with just UI settings (history, format, etc.).
    /// Database path is no longer needed here - Core owns the database.
    pub(crate) fn from_cli_config(
        cli_config: &crate::config::CliConfig,
    ) -> Result<Self> {
        let kiln_path = cli_config.kiln.path.clone();
        let config_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Failed to get home directory"))?
            .join(".crucible");

        Ok(Self {
            kiln_path,
            db_path: std::path::PathBuf::new(), // Unused - Core owns DB
            history_file: config_dir.join("repl_history"),
            tool_dir: cli_config.tools_path(),
            default_format: "table".to_string(),
            query_timeout_secs: 300,
            max_column_width: 50,
        })
    }
}

/// REPL usage statistics
///
/// This struct tracks execution metrics and is accessible via `get_stats()` for testing.
#[derive(Debug, Default)]
pub struct ReplStats {
    pub command_count: usize,
    pub query_count: usize,
    pub total_query_time: std::time::Duration,
}

impl ReplStats {
    pub fn avg_query_time(&self) -> std::time::Duration {
        if self.query_count == 0 {
            std::time::Duration::ZERO
        } else {
            self.total_query_time / self.query_count as u32
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
}

impl OutputFormat {
    fn as_str(&self) -> &str {
        match self {
            OutputFormat::Table => "table",
            OutputFormat::Json => "json",
            OutputFormat::Csv => "csv",
        }
    }
}

/// Format a tool schema for display in the REPL
fn format_tool_schema(schema: &tools::ToolSchema) -> String {
    use colored::Colorize;

    let mut output = String::new();

    // Title box
    let title_line = format!(" {} ", schema.name);
    let border_len = title_line.len() + 2;
    let top_border = format!("╭─{}─╮", "─".repeat(border_len));
    let bottom_border = format!("╰─{}─╯", "─".repeat(border_len));

    output.push_str(&format!("\n{}\n", top_border.cyan()));
    output.push_str(&format!("│ {} │\n", schema.name.bold().cyan()));
    output.push_str(&format!("│ {} │\n", " ".repeat(border_len)));
    output.push_str(&format!("│ {} │\n", schema.description.white()));
    output.push_str(&format!("{}\n", bottom_border.cyan()));

    // Parameters section
    if let Some(properties) = schema
        .input_schema
        .get("properties")
        .and_then(|p| p.as_object())
    {
        output.push_str(&format!("\n{}:\n", "Parameters".bold().green()));

        // Get required fields
        let required: Vec<String> = schema
            .input_schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        for (param_name, param_schema) in properties {
            let param_type = param_schema
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");

            let is_required = required.contains(param_name);
            let required_str = if is_required {
                "required".red()
            } else {
                "optional".yellow()
            };

            output.push_str(&format!(
                "  {} {} ({}, {})\n",
                "•".cyan(),
                param_name.bold(),
                param_type.italic(),
                required_str
            ));

            // Add description if available
            if let Some(description) = param_schema.get("description").and_then(|d| d.as_str()) {
                output.push_str(&format!("    {}\n", description.white()));
            }

            output.push('\n');
        }
    } else {
        output.push_str(&format!(
            "\n{}\n",
            "No parameters required.".italic().white()
        ));
    }

    // Usage example
    output.push_str(&format!("\n{}:\n", "Usage".bold().green()));
    output.push_str(&format!(
        "  :run {} {}\n",
        schema.name.cyan(),
        "[args...]".italic().white()
    ));

    // Additional usage tip
    output.push_str(&format!(
        "\n{} Arguments should be space-separated. Use quotes for strings with spaces.\n",
        "Tip:".yellow()
    ));

    output
}

/// Main entry point for REPL command
pub async fn execute(
    core: Arc<CrucibleCore>,
    cli_config: crate::config::CliConfig,
    non_interactive: bool,
) -> Result<()> {
    let mut repl = Repl::new(core, &cli_config).await?;

    if non_interactive {
        repl.run_non_interactive().await
    } else {
        repl.run().await
    }
}
