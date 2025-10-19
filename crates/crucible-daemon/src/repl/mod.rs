// REPL module for Crucible daemon
//
// This module implements the Read-Eval-Print Loop that handles:
// - Built-in commands (`:tools`, `:run`, etc.)
// - SurrealQL query execution
// - Output formatting (tables, JSON, CSV)
// - Command history and autocomplete

use anyhow::Result;
use reedline::{Reedline, Signal, DefaultPrompt, DefaultPromptSegment};
use tokio::sync::{watch, oneshot};
use tracing::{debug, error, info, warn};
use std::sync::Arc;
use std::time::Instant;

pub mod command;
pub mod input;
pub mod formatter;
pub mod highlighter;
pub mod completer;
pub mod history;
pub mod error;

use command::Command;
use input::Input;
use formatter::{OutputFormatter, TableFormatter, JsonFormatter, CsvFormatter, QueryResult};
use highlighter::SurrealQLHighlighter;
use completer::ReplCompleter;
use history::CommandHistory;
use error::ReplError;

/// REPL state and configuration
pub struct Repl {
    /// Line editor with history and completion
    editor: Reedline,

    /// Database connection (placeholder - will be SurrealDB)
    /// TODO: Replace with actual Surreal<Db> when SurrealDB is integrated
    db: DummyDb,

    /// Tool registry for `:run` commands
    tools: Arc<ToolRegistry>,

    /// Rune runtime for script execution
    rune_runtime: Arc<RuneRuntime>,

    /// Configuration
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
        config: ReplConfig,
        shutdown_tx: watch::Sender<bool>,
    ) -> Result<Self> {
        info!("Initializing REPL");

        // Create line editor with history and completion
        let history = CommandHistory::new(config.history_file.clone())?;

        // Setup editor with custom highlighter and completer
        let highlighter = SurrealQLHighlighter::new();

        // Placeholder database and tools (to be replaced with real implementations)
        let db = DummyDb::new();
        let tools = Arc::new(ToolRegistry::new());
        let completer = ReplCompleter::new(db.clone(), tools.clone());

        let editor = Reedline::create()
            .with_highlighter(Box::new(highlighter))
            .with_completer(Box::new(completer))
            .with_history(history.clone_backend());

        // Initialize Rune runtime
        let rune_runtime = Arc::new(RuneRuntime::new()?);

        // Default formatter
        let formatter: Box<dyn OutputFormatter> = Box::new(TableFormatter::new());

        Ok(Self {
            editor,
            db,
            tools,
            rune_runtime,
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

    /// Process a single input line
    async fn process_input(&mut self, input: &str) -> Result<(), ReplError> {
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
    async fn execute_command(&mut self, cmd: Command) -> Result<(), ReplError> {
        match cmd {
            Command::ListTools => {
                self.list_tools();
                Ok(())
            }
            Command::RunTool { tool_name, args } => {
                self.run_tool(&tool_name, args).await
            }
            Command::RunRune { script_path, args } => {
                self.run_rune_script(&script_path, args).await
            }
            Command::ShowStats => {
                self.show_stats();
                Ok(())
            }
            Command::ShowConfig => {
                self.show_config();
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
            Command::Help(topic) => {
                self.show_help(topic.as_deref());
                Ok(())
            }
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
    async fn execute_query(&mut self, query: &str) -> Result<(), ReplError> {
        use indicatif::{ProgressBar, ProgressStyle};

        // Create progress indicator
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap()
        );
        pb.set_message("Executing query...");

        // Create cancellation channel
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.current_query_cancel = Some(cancel_tx);

        // Spawn query in background
        let db = self.db.clone();
        let query = query.to_string();
        let query_task = tokio::spawn(async move {
            db.query(&query).await
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
                let formatted = self.formatter.format(query_result).await
                    .map_err(|e| ReplError::Formatting(e.to_string()))?;
                println!("{}", formatted);
                Ok(())
            }
            Err(e) => {
                Err(ReplError::Query(e))
            }
        }
    }

    /// Cancel currently running query
    fn cancel_query(&mut self) {
        if let Some(cancel_tx) = self.current_query_cancel.take() {
            let _ = cancel_tx.send(());
            println!("\n⚠️  Query cancelled");
        }
    }

    /// List available tools
    fn list_tools(&self) {
        println!("\n📦 Available Tools:\n");
        for (name, description) in self.tools.list_tools_with_descriptions() {
            println!("  {:<20} - {}", name, description);
        }
        println!();
    }

    /// Run a tool by name
    async fn run_tool(&self, tool_name: &str, args: Vec<String>) -> Result<(), ReplError> {
        info!("Running tool: {} with args: {:?}", tool_name, args);
        self.tools.execute(tool_name, args).await
            .map_err(|e| ReplError::Tool(e.to_string()))
    }

    /// Run a Rune script
    async fn run_rune_script(&self, script_path: &str, args: Vec<String>) -> Result<(), ReplError> {
        info!("Running Rune script: {} with args: {:?}", script_path, args);
        self.rune_runtime.execute(script_path, args).await
            .map_err(|e| ReplError::Rune(e.to_string()))
    }

    /// Show REPL and database statistics
    fn show_stats(&self) {
        println!("\n📊 Statistics:\n");
        println!("  Commands executed: {}", self.stats.command_count);
        println!("  Queries executed:  {}", self.stats.query_count);
        println!("  Avg query time:    {:?}", self.stats.avg_query_time());
        println!("  History size:      {}", self.history.len());
        println!("  Tools loaded:      {}", self.tools.count());
        println!();
    }

    /// Show current configuration
    fn show_config(&self) {
        println!("\n⚙️  Configuration:\n");
        println!("  Vault path:       {}", self.config.vault_path.display());
        println!("  Database path:    {}", self.config.db_path.display());
        println!("  History file:     {}", self.config.history_file.display());
        println!("  Output format:    {}", self.config.default_format);
        println!("  Query timeout:    {}s", self.config.query_timeout_secs);
        println!("  Max column width: {}", self.config.max_column_width);
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

        println!("\n{}", "╔═══════════════════════════════════════════════════╗".cyan());
        println!("{}", "║        Crucible Daemon REPL v0.1.0                ║".cyan());
        println!("{}", "║   Queryable Knowledge Layer for Terminal Users    ║".cyan());
        println!("{}", "╚═══════════════════════════════════════════════════╝".cyan());
        println!("\nType {} for available commands or {} to exit.", ":help".green(), ":quit".red());
        println!("You can also execute SurrealQL queries directly.\n");
    }
}

/// REPL configuration
#[derive(Debug, Clone)]
pub struct ReplConfig {
    pub vault_path: std::path::PathBuf,
    pub db_path: std::path::PathBuf,
    pub history_file: std::path::PathBuf,
    pub default_format: String,
    pub query_timeout_secs: u64,
    pub max_column_width: usize,
}

impl Default for ReplConfig {
    fn default() -> Self {
        let config_dir = dirs::home_dir()
            .expect("Failed to get home directory")
            .join(".crucible");

        Self {
            vault_path: dirs::home_dir().unwrap().join("Documents/vault"),
            db_path: config_dir.join("db"),
            history_file: config_dir.join("history"),
            default_format: "table".to_string(),
            query_timeout_secs: 300,
            max_column_width: 50,
        }
    }
}

/// REPL usage statistics
#[derive(Debug, Default)]
struct ReplStats {
    command_count: usize,
    query_count: usize,
    total_query_time: std::time::Duration,
}

impl ReplStats {
    fn avg_query_time(&self) -> std::time::Duration {
        if self.query_count == 0 {
            std::time::Duration::ZERO
        } else {
            self.total_query_time / self.query_count as u32
        }
    }
}

// Placeholder types (to be replaced with real implementations)

#[derive(Clone)]
struct DummyDb;

impl DummyDb {
    fn new() -> Self {
        Self
    }

    async fn query(&self, query: &str) -> Result<QueryResult, String> {
        // Placeholder implementation
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Simulate query result
        Ok(QueryResult {
            rows: vec![],
            duration: std::time::Duration::from_millis(100),
            affected_rows: Some(0),
            status: formatter::QueryStatus::Success,
        })
    }
}

struct ToolRegistry;

impl ToolRegistry {
    fn new() -> Self {
        Self
    }

    fn list_tools_with_descriptions(&self) -> Vec<(&str, &str)> {
        vec![
            ("search_by_tags", "Search notes by tags"),
            ("search_by_content", "Full-text content search"),
            ("metadata", "Extract note metadata"),
            ("semantic_search", "Vector similarity search"),
        ]
    }

    fn count(&self) -> usize {
        4
    }

    async fn execute(&self, _name: &str, _args: Vec<String>) -> Result<()> {
        println!("Tool execution placeholder");
        Ok(())
    }
}

struct RuneRuntime;

impl RuneRuntime {
    fn new() -> Result<Self> {
        Ok(Self)
    }

    async fn execute(&self, _script_path: &str, _args: Vec<String>) -> Result<()> {
        println!("Rune execution placeholder");
        Ok(())
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
