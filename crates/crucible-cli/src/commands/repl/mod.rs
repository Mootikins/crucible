// REPL module for Crucible CLI
//
// This module implements the Read-Eval-Print Loop that handles:
// - Built-in commands (`:tools`, `:run`, etc.)
// - SurrealQL query execution
// - Output formatting (tables, JSON, CSV)
// - Command history and autocomplete

use anyhow::Result;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{oneshot, watch};
use tracing::{debug, error, info, warn};

pub mod command;
pub mod completer;
pub mod database;
pub mod error;
pub mod formatter;
pub mod highlighter;
pub mod history;
pub mod input;
pub mod tools;

use crate::config::CliConfig;
use tools::UnifiedToolRegistry;

use command::Command;
use completer::ReplCompleter;
use database::ReplDatabase;
use error::ReplError;
use formatter::{CsvFormatter, JsonFormatter, OutputFormatter, TableFormatter};
use highlighter::SurrealQLHighlighter;
use history::CommandHistory;
use input::Input;

/// REPL state and configuration
pub struct Repl {
    /// Line editor with history and completion
    editor: Reedline,

    /// Database connection using SurrealDB
    db: ReplDatabase,

    /// Tool registry for `:run` commands (includes system and Rune tools)
    tools: Arc<UnifiedToolRegistry>,

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
        cli_config: &CliConfig,
        db_path: Option<String>,
        tool_dir: Option<String>,
        format: String,
    ) -> Result<Self> {
        info!("Initializing REPL");

        // Create REPL config from CLI config
        let config = ReplConfig::from_cli_config(cli_config, db_path, tool_dir, format)?;

        // Create line editor with history and completion
        let history = CommandHistory::new(config.history_file.clone())?;

        // Setup editor with custom highlighter and completer
        let highlighter = SurrealQLHighlighter::new();

        // Create real database connection
        let db_path = config.db_path.to_string_lossy().to_string();
        let db = match ReplDatabase::new(&db_path).await {
            Ok(db) => {
                info!("Connected to database at: {}", db_path);
                db
            }
            Err(e) => {
                warn!(
                    "Failed to connect to database at {}: {}, falling back to in-memory",
                    db_path, e
                );
                ReplDatabase::new_memory().await?
            }
        };

        // Initialize unified tool registry
        let tool_dir = config.tool_dir.clone();
        let tools = UnifiedToolRegistry::new(tool_dir).await?;

        info!("Initialized unified tool registry");

        let tools_arc = Arc::new(tools);
        let completer = ReplCompleter::new(db.clone(), tools_arc.clone());

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
            editor,
            db,
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
                self.list_tools().await;
                Ok(())
            }
            Command::RunTool { tool_name, args } => self.run_tool(&tool_name, args).await,
            Command::RunRune { script_path, args } => {
                // :rune can run arbitrary .rn files, not just registered tools
                // Extract filename without extension as tool name
                let tool_name = std::path::Path::new(&script_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&script_path);
                self.run_tool(tool_name, args).await
            }
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
    async fn execute_query(&mut self, query: &str) -> Result<(), ReplError> {
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
        let db = self.db.clone();
        let query = query.to_string();
        let query_task = tokio::spawn(async move { db.query(&query).await });

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
            println!(
                "\n{} No tools found. Add .rn files to {}\n",
                "ℹ".blue(),
                self.config.tool_dir.display()
            );
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
                "rune" => colored::Color::Cyan,
                _ => colored::Color::White,
            };

            println!(
                "  {} ({}) [{} tools]:",
                group_name.to_uppercase().color(group_color),
                match group_name.as_str() {
                    "system" => "crucible-tools",
                    "rune" => "scripted tools",
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
                    .unwrap_or_else(|| String::new());

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

        // Execute the tool
        let result = self
            .tools
            .execute_tool(tool_name, &args)
            .await
            .map_err(|e| {
                let err_str = e.to_string();
                // Check if this is a parameter conversion error
                if err_str.contains("Parameter conversion failed:") {
                    // Extract the actual error message after the prefix
                    let clean_msg = err_str
                        .strip_prefix("Parameter conversion failed: ")
                        .unwrap_or(&err_str);
                    eprintln!(
                        "\n{} {}\n",
                        "❌ Tool Execution Failed:".red().bold(),
                        clean_msg
                    );
                } else {
                    eprintln!("\n{} {}\n", "❌ Tool Error:".red(), err_str);
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
        match self.db.get_stats().await {
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
    #[cfg(test)]
    pub fn get_tools(&self) -> &Arc<UnifiedToolRegistry> {
        &self.tools
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
    fn from_cli_config(
        cli_config: &crate::config::CliConfig,
        db_path: Option<String>,
        tool_dir: Option<String>,
        format: String,
    ) -> Result<Self> {
        let kiln_path = cli_config.kiln.path.clone();
        let config_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Failed to get home directory"))?
            .join(".crucible");

        Ok(Self {
            kiln_path,
            db_path: db_path
                .map(|p| p.into())
                .unwrap_or_else(|| config_dir.join("kiln.db")),
            history_file: config_dir.join("repl_history"),
            tool_dir: tool_dir
                .map(|p| p.into())
                .unwrap_or_else(|| config_dir.join("tools")),
            default_format: format,
            query_timeout_secs: 300,
            max_column_width: 50,
        })
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
    cli_config: crate::config::CliConfig,
    db_path: Option<String>,
    tool_dir: Option<String>,
    format: String,
) -> Result<()> {
    let mut repl = Repl::new(&cli_config, db_path, tool_dir, format).await?;
    repl.run().await
}
