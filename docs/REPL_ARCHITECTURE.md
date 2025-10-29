# Crucible REPL Architecture

> **Status**: Design Phase (2025-10-19)
> **Component**: crucible-daemon REPL
> **Purpose**: Interactive query and command interface for the Crucible daemon

## Executive Summary

The REPL (Read-Eval-Print Loop) is the primary human interface to the Crucible daemon. It must handle three distinct input types: built-in commands (`:tools`, `:run`, etc.), raw SurrealQL queries, and tool invocations. The design prioritizes **responsiveness**, **excellent error messages**, and **async-first patterns** to prevent blocking on long-running queries.

## Design Rationale

### Key Architectural Decisions

#### 1. Line Editor: Reedline (Winner)

**Decision**: Use `reedline` over `rustyline`

**Rationale**:
- **Modern async-first design**: Built with Tokio integration in mind
- **Rich editing features**: Vi/Emacs modes, syntax highlighting, validation
- **Maintained by Nushell team**: Active development, production-proven
- **Better integration**: Works seamlessly with crossterm (already in use)
- **Extensibility**: Plugin architecture for custom validators and highlighters

**Rustyline drawbacks**:
- Synchronous API requires workarounds for async integration
- Less active maintenance
- Limited customization without forking

**Code impact**: ~200 LoC for custom validator, highlighter, and history integration

#### 2. Command Parsing: Regex + Manual Parsing (Winner)

**Decision**: Use regex for command prefix detection + manual parsing for arguments

**Rationale**:
- **Simplicity**: Commands have simple structure (`:cmd arg1 arg2`)
- **Performance**: Regex is sufficient for prefix detection, manual parsing for args
- **Error messages**: Full control over error reporting
- **Zero dependencies**: No parser combinator library needed

**Parser combinator (nom/pest) drawbacks**:
- Overkill for simple command syntax
- Harder to generate helpful error messages
- Additional dependency weight
- Steeper learning curve for contributors

**Example**:
```rust
// Simple and clear
if input.starts_with(':') {
    parse_command(input)
} else {
    parse_surrealql(input)
}
```

#### 3. Async Query Execution: Non-Blocking with Cancellation

**Decision**: Background task with cancellable queries

**Pattern**:
```rust
// User input doesn't block during query execution
// Ctrl+C cancels running query without exiting REPL
tokio::select! {
    result = query_task => handle_result(result),
    _ = ctrl_c_signal => cancel_query(),
}
```

**Benefits**:
- User can cancel long-running queries (Ctrl+C)
- REPL remains responsive during execution
- Multiple queries can run concurrently (future enhancement)
- Progress indicators for slow queries

#### 4. Output Rendering: Buffer Full Results (PoC), Stream Later

**Decision**: Buffer full results for PoC, design for streaming

**PoC Approach**:
- Query returns `Vec<QueryRow>`, format after completion
- Simple and debuggable
- Acceptable for <10k rows

**Future Streaming Design**:
```rust
// When result sets grow large
async fn stream_results(mut rows: impl Stream<Item = QueryRow>) {
    while let Some(row) = rows.next().await {
        render_row(row);
    }
}
```

**Why buffer first**:
- Table formatting needs column widths (requires full data scan)
- Simpler error handling
- Most queries return <100 rows
- Can add streaming when needed (YAGNI principle)

#### 5. Syntax Highlighting: Yes (Minimal Cost)

**Decision**: Implement basic SurrealQL syntax highlighting

**Rationale**:
- Reedline provides highlighting hooks (minimal code)
- Significantly improves UX for SQL queries
- ~100 LoC for keyword highlighting
- No runtime performance impact (highlighting is incremental)

**Scope**:
- Keywords: `SELECT`, `FROM`, `WHERE`, `ORDER BY`, `LIMIT`
- Identifiers: table names, field names
- Strings and numbers
- Comments

#### 6. Autocomplete: Commands + Table Names (Phase 1)

**Decision**: Implement for `:` commands and table names

**Phase 1 (PoC)**:
- `:` command completion (`:tools`, `:run`, `:stats`, etc.)
- Table name completion after `FROM` keyword
- Tool name completion for `:run <tool>`

**Phase 2 (Future)**:
- Column name completion
- Function name completion
- Fuzzy matching for all completions

**Implementation**: Reedline's `Completer` trait with SurrealDB schema introspection

#### 7. Error Display: Structured with Context

**Decision**: Rich error formatting with color and context

**Pattern**:
```rust
// Bad: "Error: invalid query"
// Good:
❌ Query Error (line 2, column 15):
  SELECT * FROM notes WHERE tags CONTAINS #project
                                         ^
  Unexpected token '#'. Did you mean: tags CONTAINS '#project'?
```

**Features**:
- Error type categorization (parse, runtime, database)
- Color coding (errors red, warnings yellow, hints cyan)
- Contextual help messages
- "Did you mean?" suggestions for typos

## Architecture Components

### 1. Repl Struct

Core state management for the REPL session.

```rust
pub struct Repl {
    /// Line editor with history and completion
    editor: Reedline,

    /// SurrealDB connection for query execution
    db: Surreal<Db>,

    /// Service layer for tool registry and search
    services: crucible_services::ServiceRegistry,

    /// Tool registry (built-in + dynamic Rune tools)
    tools: crucible_tools::ToolRegistry,

    /// Rune runtime for script execution with hot-reload
    rune_runtime: crucible_rune::RuneRuntime,

    /// Configuration from ~/.crucible/config.yaml
    config: DaemonConfig,

    /// Current log level filter
    log_level: LevelFilter,

    /// Query history for persistence
    history: CommandHistory,

    /// Graceful shutdown signal
    shutdown_tx: watch::Sender<bool>,
}

impl Repl {
    /// Create a new REPL instance
    pub async fn new(
        db: Surreal<Db>,
        config: DaemonConfig,
        shutdown_tx: watch::Sender<bool>,
    ) -> Result<Self>;

    /// Run the REPL loop (blocks until :quit)
    pub async fn run(&mut self) -> Result<()>;

    /// Process a single input line
    async fn process_input(&mut self, input: &str) -> Result<()>;

    /// Execute a built-in command
    async fn execute_command(&mut self, cmd: Command) -> Result<()>;

    /// Execute a SurrealQL query
    async fn execute_query(&mut self, query: &str) -> Result<QueryResult>;

    /// Cancel currently running query
    fn cancel_query(&mut self);
}
```

**Key State**:
- `editor`: Manages input, history, completion
- `db`: SurrealDB connection for query execution
- `services`: Service layer providing search, indexing, and tool management
- `tools`: Registry combining built-in and dynamic Rune tools
- `rune_runtime`: Rune runtime with hot-reload for script execution
- `history`: Persists to `~/.crucible/history`

### 2. Command Enum

Represents parsed built-in commands.

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// :tools - List available tools
    ListTools,

    /// :run <tool> <args...> - Execute a tool
    RunTool {
        tool_name: String,
        args: Vec<String>,
    },

    /// :rune <script> [args...] - Run a Rune script
    RunRune {
        script_path: String,
        args: Vec<String>,
    },

    /// :stats - Show kiln statistics
    ShowStats,

    /// :config - Display configuration
    ShowConfig,

    /// :log <level> - Set log level (trace|debug|info|warn|error)
    SetLogLevel(LevelFilter),

    /// :help [command] - Show help
    Help(Option<String>),

    /// :history [limit] - Show command history
    ShowHistory(Option<usize>),

    /// :clear - Clear screen
    ClearScreen,

    /// :quit - Exit daemon
    Quit,
}

impl Command {
    /// Parse a command string (assumes starts with ':')
    pub fn parse(input: &str) -> Result<Self, CommandParseError>;

    /// Get help text for a command
    pub fn help_text(&self) -> &'static str;
}
```

**Parsing Logic**:
```rust
// Input: ":run search_by_tags project ai"
// Output: Command::RunTool {
//     tool_name: "search_by_tags",
//     args: vec!["project", "ai"]
// }

// Simple split-based parsing (no complex grammar)
let parts: Vec<&str> = input.trim_start_matches(':')
    .split_whitespace()
    .collect();

match parts[0] {
    "run" => {
        let tool_name = parts.get(1)
            .ok_or(CommandParseError::MissingToolName)?;
        let args = parts[2..].iter().map(|s| s.to_string()).collect();
        Ok(Command::RunTool { tool_name: tool_name.to_string(), args })
    },
    // ... other commands
}
```

### 3. Input Parser

Determines input type and routes to appropriate handler.

```rust
pub enum Input {
    /// Built-in command (starts with ':')
    Command(Command),

    /// SurrealQL query
    Query(String),

    /// Empty line (ignored)
    Empty,
}

impl Input {
    /// Parse raw input line
    pub fn parse(line: &str) -> Result<Self, ParseError> {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            return Ok(Input::Empty);
        }

        if trimmed.starts_with(':') {
            let cmd = Command::parse(trimmed)
                .map_err(ParseError::CommandError)?;
            return Ok(Input::Command(cmd));
        }

        // Everything else is a SurrealQL query
        Ok(Input::Query(trimmed.to_string()))
    }
}
```

**No ambiguity**: `:` prefix explicitly marks commands, everything else is SurrealQL.

### 4. OutputFormatter Trait

Abstraction for rendering query results in different formats.

```rust
#[async_trait]
pub trait OutputFormatter: Send + Sync {
    /// Format query results
    async fn format(&self, result: QueryResult) -> Result<String>;

    /// Format error message
    fn format_error(&self, error: &Error) -> String;
}

/// Table format (default, human-readable)
pub struct TableFormatter {
    max_column_width: usize,
    truncate_content: bool,
}

impl OutputFormatter for TableFormatter {
    async fn format(&self, result: QueryResult) -> Result<String> {
        use comfy_table::{Table, Cell, Row};

        let mut table = Table::new();
        table.load_preset("││──╞═╪╡│    ┬┴┌┐└┘");

        // Add headers
        if let Some(first_row) = result.rows.first() {
            let headers: Row = first_row.keys()
                .map(|k| Cell::new(k))
                .collect();
            table.set_header(headers);
        }

        // Add rows
        for row in result.rows {
            let cells: Row = row.values()
                .map(|v| Cell::new(format_value(v)))
                .collect();
            table.add_row(cells);
        }

        Ok(table.to_string())
    }

    fn format_error(&self, error: &Error) -> String {
        use colored::*;
        format!("{} {}", "❌".red(), error.to_string().red())
    }
}

/// JSON format (machine-readable, for piping)
pub struct JsonFormatter {
    pretty: bool,
}

impl OutputFormatter for JsonFormatter {
    async fn format(&self, result: QueryResult) -> Result<String> {
        if self.pretty {
            serde_json::to_string_pretty(&result.rows)
        } else {
            serde_json::to_string(&result.rows)
        }.map_err(Into::into)
    }

    fn format_error(&self, error: &Error) -> String {
        json!({
            "error": error.to_string(),
            "type": error_type_name(error),
        }).to_string()
    }
}

/// CSV format (export-friendly)
pub struct CsvFormatter;

impl OutputFormatter for CsvFormatter {
    async fn format(&self, result: QueryResult) -> Result<String> {
        let mut writer = csv::Writer::from_writer(vec![]);

        // Write headers
        if let Some(first_row) = result.rows.first() {
            writer.write_record(first_row.keys())?;
        }

        // Write rows
        for row in result.rows {
            let values: Vec<String> = row.values()
                .map(|v| format_value(v))
                .collect();
            writer.write_record(&values)?;
        }

        Ok(String::from_utf8(writer.into_inner()?)?)
    }

    fn format_error(&self, error: &Error) -> String {
        format!("ERROR,{}", error)
    }
}
```

**Usage**:
```rust
// User can switch formatters
:format table   // Default, human-readable
:format json    // For piping to jq
:format csv     // For export to spreadsheet
```

### 5. Query Result Types

Standardized result representation.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Result rows (each row is a map of column -> value)
    pub rows: Vec<BTreeMap<String, Value>>,

    /// Query execution time
    pub duration: Duration,

    /// Number of rows affected (for mutations)
    pub affected_rows: Option<u64>,

    /// Query status
    pub status: QueryStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum QueryStatus {
    /// Query completed successfully
    Success,

    /// Query completed with warnings
    Warning,

    /// Query failed
    Error,
}

impl QueryResult {
    /// Create from SurrealDB response
    pub fn from_surreal(response: surrealdb::Response) -> Result<Self>;

    /// Check if result is empty
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Get row count
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}
```

## Async Query Handling Pattern

### Problem: Long-Running Queries Block REPL

**Symptom**: User types query, waits 30 seconds, can't do anything (not even Ctrl+C)

**Root Cause**: Synchronous query execution in REPL loop

### Solution: Async Execution with Cancellation

```rust
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};

impl Repl {
    async fn execute_query(&mut self, query: &str) -> Result<QueryResult> {
        // Create cancellation channel
        let (cancel_tx, cancel_rx) = oneshot::channel();

        // Store cancel handle for Ctrl+C handler
        self.current_query_cancel = Some(cancel_tx);

        // Spawn query in background
        let db = self.db.clone();
        let query = query.to_string();

        let query_task = tokio::spawn(async move {
            // Execute query with timeout (configurable)
            timeout(Duration::from_secs(300), async {
                db.query(&query).await
            }).await
        });

        // Wait for either completion or cancellation
        tokio::select! {
            result = query_task => {
                self.current_query_cancel = None;
                match result {
                    Ok(Ok(Ok(response))) => QueryResult::from_surreal(response),
                    Ok(Ok(Err(_))) => Err(anyhow!("Query timeout")),
                    Ok(Err(e)) => Err(anyhow!("Query error: {}", e)),
                    Err(e) => Err(anyhow!("Task error: {}", e)),
                }
            }
            _ = cancel_rx => {
                self.current_query_cancel = None;
                Err(anyhow!("Query cancelled by user"))
            }
        }
    }

    /// Called when user presses Ctrl+C
    fn cancel_query(&mut self) {
        if let Some(cancel_tx) = self.current_query_cancel.take() {
            let _ = cancel_tx.send(());
            println!("\n⚠️  Query cancelled");
        }
    }
}
```

**Benefits**:
1. User can cancel with Ctrl+C (doesn't exit REPL)
2. Progress indicator can run concurrently
3. Multiple queries possible (future: background tabs)
4. Timeout protection (default 5 minutes)

### Progress Indicator Example

```rust
use indicatif::{ProgressBar, ProgressStyle};

async fn execute_with_progress(&mut self, query: &str) -> Result<QueryResult> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.cyan} {msg}")
        .unwrap());
    pb.set_message("Executing query...");

    let pb_clone = pb.clone();
    tokio::spawn(async move {
        loop {
            pb_clone.tick();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let result = self.execute_query(query).await;
    pb.finish_and_clear();
    result
}
```

## Command History Management

```rust
use reedline::{FileBackedHistory, History};

pub struct CommandHistory {
    /// Reedline history backend
    history: Box<dyn History>,

    /// History file path
    file_path: PathBuf,
}

impl CommandHistory {
    /// Create new history manager
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let history = Box::new(FileBackedHistory::with_file(
            10_000, // Max entries
            file_path.clone(),
        )?);

        Ok(Self { history, file_path })
    }

    /// Add command to history (skip duplicates)
    pub fn add(&mut self, command: &str) {
        // Skip empty lines and duplicates
        if command.trim().is_empty() {
            return;
        }

        if let Some(last) = self.history.last() {
            if last == command {
                return; // Skip duplicate
            }
        }

        self.history.save(command).ok();
    }

    /// Search history (fuzzy match)
    pub fn search(&self, pattern: &str) -> Vec<String> {
        self.history
            .iter()
            .filter(|cmd| cmd.contains(pattern))
            .cloned()
            .collect()
    }

    /// Clear history
    pub fn clear(&mut self) -> Result<()> {
        std::fs::remove_file(&self.file_path)?;
        self.history = Box::new(FileBackedHistory::with_file(
            10_000,
            self.file_path.clone(),
        )?);
        Ok(())
    }
}
```

**Features**:
- Persistent across sessions (`~/.crucible/history`)
- Fuzzy search with Ctrl+R
- Duplicate detection (like bash `HISTCONTROL=ignoredups`)
- Max 10k entries (configurable)

## Syntax Highlighting

```rust
use reedline::{Highlighter, StyledText};
use nu_ansi_term::{Color, Style};

pub struct SurrealQLHighlighter {
    keywords: HashSet<String>,
}

impl SurrealQLHighlighter {
    pub fn new() -> Self {
        let keywords = [
            "SELECT", "FROM", "WHERE", "ORDER", "BY", "LIMIT",
            "CREATE", "UPDATE", "DELETE", "INSERT", "INTO",
            "AND", "OR", "NOT", "IN", "CONTAINS", "AS",
        ].iter().map(|s| s.to_string()).collect();

        Self { keywords }
    }
}

impl Highlighter for SurrealQLHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();

        // Command prefix (cyan)
        if line.starts_with(':') {
            styled.push((Style::new().fg(Color::Cyan), line.to_string()));
            return styled;
        }

        // SQL keywords (blue)
        for word in line.split_whitespace() {
            let upper = word.to_uppercase();
            if self.keywords.contains(&upper) {
                styled.push((Style::new().fg(Color::Blue).bold(), word.to_string()));
            } else if word.starts_with('\'') || word.starts_with('"') {
                // String literals (green)
                styled.push((Style::new().fg(Color::Green), word.to_string()));
            } else if word.parse::<f64>().is_ok() {
                // Numbers (yellow)
                styled.push((Style::new().fg(Color::Yellow), word.to_string()));
            } else {
                // Default (white)
                styled.push((Style::default(), word.to_string()));
            }
            styled.push((Style::default(), " ".to_string()));
        }

        styled
    }
}
```

**Visual Example**:
```
> SELECT * FROM notes WHERE tags CONTAINS '#project'
  ^^^^^^   ^^^^       ^^^^^                         (blue/bold - keywords)
          ^                 ^^^^^                    (white - identifiers)
                                       ^^^^^^^^^^    (green - string)
```

## Autocomplete Implementation

```rust
use reedline::{Completer, Suggestion, Span};

pub struct ReplCompleter {
    /// Service layer for tool name discovery and search
    services: Arc<crucible_services::ServiceRegistry>,

    /// Tool registry for tool name completion
    tools: Arc<crucible_tools::ToolRegistry>,

    /// Built-in commands
    commands: Vec<String>,
}

impl ReplCompleter {
    pub fn new(
        db: Surreal<Db>,
        services: Arc<crucible_services::ServiceRegistry>,
        tools: Arc<crucible_tools::ToolRegistry>
    ) -> Self {
        let commands = vec![
            ":tools".to_string(),
            ":run".to_string(),
            ":rune".to_string(),
            ":stats".to_string(),
            ":config".to_string(),
            ":log".to_string(),
            ":help".to_string(),
            ":history".to_string(),
            ":clear".to_string(),
            ":quit".to_string(),
        ];

        Self { db, tools, commands }
    }

    /// Get table names from database
    async fn get_table_names(&self) -> Result<Vec<String>> {
        let response: Vec<String> = self.db
            .query("INFO FOR DB")
            .await?
            .take(0)?;
        Ok(response)
    }

    /// Complete tool names for `:run` command from both static and dynamic tools
    fn complete_tool_name(&self, partial: &str) -> Vec<Suggestion> {
        // Get tools from static registry
        let static_tools = self.tools
            .list_tools()
            .iter()
            .filter(|name| name.starts_with(partial))
            .map(|name| Suggestion {
                value: name.clone(),
                description: self.tools.get_tool_description(name),
                extra: None,
                span: Span::new(0, partial.len()),
                append_whitespace: true,
            });

        // Get tools from service layer (dynamic Rune tools)
        let dynamic_tools = self.services
            .list_available_tools()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|tool| tool.name.starts_with(partial))
            .map(|tool| Suggestion {
                value: tool.name,
                description: Some(tool.description),
                extra: None,
                span: Span::new(0, partial.len()),
                append_whitespace: true,
            });

        static_tools.chain(dynamic_tools).collect()
    }
}

#[async_trait]
impl Completer for ReplCompleter {
    fn complete(&self, line: &str, pos: usize) -> Vec<Suggestion> {
        let prefix = &line[..pos];

        // Complete commands
        if prefix.starts_with(':') {
            return self.commands
                .iter()
                .filter(|cmd| cmd.starts_with(prefix))
                .map(|cmd| Suggestion {
                    value: cmd.clone(),
                    description: Some(Command::help_text(cmd)),
                    extra: None,
                    span: Span::new(0, prefix.len()),
                    append_whitespace: true,
                })
                .collect();
        }

        // Complete tool names after `:run `
        if prefix.starts_with(":run ") {
            let tool_partial = prefix.strip_prefix(":run ").unwrap();
            return self.complete_tool_name(tool_partial);
        }

        // Complete table names after `FROM `
        if let Some(from_pos) = prefix.rfind("FROM ") {
            let table_partial = &prefix[from_pos + 5..];
            // Note: This requires async, so we'd need to block or cache
            // For PoC, we can use a cached list of tables
            // Future: Background task updates table list
        }

        vec![]
    }
}
```

## Error Handling and Display

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReplError {
    #[error("Command parse error: {0}")]
    CommandParse(#[from] CommandParseError),

    #[error("Query error: {0}")]
    Query(#[from] surrealdb::Error),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Rune execution error: {0}")]
    Rune(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

impl ReplError {
    /// Format error with color and context
    pub fn display_pretty(&self) -> String {
        use colored::*;

        match self {
            ReplError::CommandParse(e) => {
                format!(
                    "{} Command Error: {}\n{} Type ':help' for available commands",
                    "❌".red(),
                    e.to_string().red(),
                    "💡".cyan(),
                )
            }
            ReplError::Query(e) => {
                // Try to extract line/column info from SurrealDB error
                if let Some((line, col, msg)) = parse_query_error(e) {
                    format!(
                        "{} Query Error (line {}, column {}):\n  {}\n  {}",
                        "❌".red(),
                        line,
                        col,
                        self.get_query_context(line, col),
                        msg.red(),
                    )
                } else {
                    format!("{} Query Error: {}", "❌".red(), e.to_string().red())
                }
            }
            ReplError::Tool(msg) => {
                format!(
                    "{} Tool Execution Failed: {}\n{} Use ':tools' to list available tools",
                    "❌".red(),
                    msg.red(),
                    "💡".cyan(),
                )
            }
            ReplError::Rune(msg) => {
                format!("{} Rune Script Error: {}", "❌".red(), msg.red())
            }
            _ => format!("{} {}", "❌".red(), self.to_string().red()),
        }
    }

    /// Extract query context for error display
    fn get_query_context(&self, line: usize, col: usize) -> String {
        // Show the line with an arrow pointing to the error
        // This would require storing the original query
        // For now, simplified version:
        format!("{}^", " ".repeat(col.saturating_sub(1)))
    }
}
```

## Integration with TUI

The REPL will be embedded in a ratatui TUI with log window:

```rust
use ratatui::{Frame, backend::CrosstermBackend};
use ratatui::layout::{Layout, Constraint, Direction};
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct DaemonTui {
    /// REPL instance
    repl: Repl,

    /// Log buffer (rolling window)
    log_buffer: Arc<Mutex<LogBuffer>>,

    /// Terminal backend
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl DaemonTui {
    pub fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),      // Log window
                Constraint::Length(3),    // Status bar
                Constraint::Min(5),       // REPL area
            ])
            .split(frame.size());

        // Render log window
        self.render_logs(frame, chunks[0]);

        // Render status bar
        self.render_status(frame, chunks[1]);

        // Render REPL (reedline handles its own rendering)
        self.render_repl(frame, chunks[2]);
    }

    fn render_logs(&self, frame: &mut Frame, area: Rect) {
        let logs = self.log_buffer.lock().unwrap();
        let log_text = logs.get_last_n(20).join("\n");

        let block = Block::default()
            .title("Logs (last 20 lines)")
            .borders(Borders::ALL);

        let paragraph = Paragraph::new(log_text)
            .block(block)
            .scroll((0, 0));

        frame.render_widget(paragraph, area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let status = format!(
            "SurrealDB: {} | Docs: {} | Query: {}",
            "connected".green(),
            self.repl.get_doc_count(),
            if self.repl.has_running_query() { "running..." } else { "idle" }
        );

        let paragraph = Paragraph::new(status);
        frame.render_widget(paragraph, area);
    }

    fn render_repl(&self, frame: &mut Frame, area: Rect) {
        // Reedline handles rendering within this area
        // We just need to provide the bounding box
    }
}
```

## File Structure

```
crates/crucible-daemon/
├── src/
│   ├── main.rs              # Entry point, TUI setup
│   ├── repl/
│   │   ├── mod.rs           # Repl struct and main loop
│   │   ├── command.rs       # Command enum and parsing
│   │   ├── input.rs         # Input parsing (Command vs Query)
│   │   ├── formatter.rs     # OutputFormatter trait and impls
│   │   ├── highlighter.rs   # SurrealQL syntax highlighting
│   │   ├── completer.rs     # Autocomplete implementation
│   │   ├── history.rs       # Command history management
│   │   └── error.rs         # Error types and display
│   ├── services/
│   │   ├── mod.rs           # Service layer integration
│   │   ├── registry.rs      # ServiceRegistry
│   │   └── search.rs        # Search service integration
│   ├── tools/
│   │   ├── mod.rs           # Tool registry (static tools)
│   │   ├── search.rs        # Built-in search tools
│   │   ├── metadata.rs      # Metadata extraction tools
│   │   └── registry.rs      # crucible_tools::ToolRegistry
│   ├── rune/
│   │   ├── mod.rs           # crucible_rune integration
│   │   └── loader.rs        # Script loading and hot-reload
│   └── tui/
│       ├── mod.rs           # TUI main loop
│       ├── logs.rs          # Log buffer and rendering
│       └── layout.rs        # Layout management
├── Cargo.toml
└── README.md
```

## Performance Characteristics

### Expected Performance

| Operation | Latency | Notes |
|-----------|---------|-------|
| Command parsing | <1ms | Regex + split, no allocation |
| Query execution | 1-500ms | Depends on query complexity |
| Result formatting (100 rows) | 5-10ms | Table layout calculation |
| History search | <10ms | Linear scan, max 10k entries |
| Autocomplete | <20ms | Database introspection cached |
| Syntax highlighting | <1ms | Incremental, runs per keystroke |

### Memory Footprint

- Base REPL: ~2MB (editor state, history)
- Query result buffer: ~100KB per 1000 rows
- Tool registry: ~500KB (including Rune scripts)
- Total: <10MB for typical usage

### Optimization Opportunities

1. **Table name caching**: Background task updates every 10s
2. **Query plan caching**: Cache explain plans for common patterns
3. **Result streaming**: For queries returning >10k rows
4. **Lazy highlighting**: Only highlight visible portion in multiline queries

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parsing() {
        let input = ":run search_by_tags project ai";
        let cmd = Command::parse(input).unwrap();

        match cmd {
            Command::RunTool { tool_name, args } => {
                assert_eq!(tool_name, "search_by_tags");
                assert_eq!(args, vec!["project", "ai"]);
            }
            _ => panic!("Expected RunTool command"),
        }
    }

    #[test]
    fn test_input_routing() {
        let query = "SELECT * FROM notes";
        let input = Input::parse(query).unwrap();
        assert!(matches!(input, Input::Query(_)));

        let cmd = ":tools";
        let input = Input::parse(cmd).unwrap();
        assert!(matches!(input, Input::Command(_)));
    }

    #[tokio::test]
    async fn test_query_execution() {
        let db = setup_test_db().await;
        let repl = Repl::new(db, test_config(), shutdown_channel()).await.unwrap();

        let result = repl.execute_query("SELECT * FROM notes LIMIT 5").await.unwrap();
        assert_eq!(result.status, QueryStatus::Success);
    }

    #[tokio::test]
    async fn test_query_cancellation() {
        let db = setup_test_db().await;
        let mut repl = Repl::new(db, test_config(), shutdown_channel()).await.unwrap();

        // Start long-running query
        let query_task = tokio::spawn(async move {
            repl.execute_query("SELECT * FROM notes WHERE /* long query */").await
        });

        // Cancel after 100ms
        tokio::time::sleep(Duration::from_millis(100)).await;
        repl.cancel_query();

        let result = query_task.await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_full_repl_session() {
    let mut repl = create_test_repl().await;

    // Execute commands
    repl.process_input(":stats").await.unwrap();
    repl.process_input("SELECT * FROM notes LIMIT 1").await.unwrap();
    repl.process_input(":run search_by_tags test").await.unwrap();

    // Verify history
    let history = repl.get_history();
    assert_eq!(history.len(), 3);
}

#[tokio::test]
async fn test_multiline_query() {
    let mut repl = create_test_repl().await;

    let query = r#"
        SELECT
            path,
            title,
            tags
        FROM notes
        WHERE tags CONTAINS '#project'
        LIMIT 10
    "#;

    let result = repl.process_input(query).await.unwrap();
    assert!(result.is_success());
}
```

## Configuration

```yaml
# ~/.crucible/config.yaml
repl:
  # History settings
  history_file: "~/.crucible/history"
  history_max_entries: 10000

  # Prompt customization
  prompt: "> "
  prompt_color: "cyan"

  # Editor settings
  edit_mode: "emacs"  # or "vi"

  # Syntax highlighting
  syntax_highlighting: true

  # Autocomplete
  autocomplete_enabled: true
  autocomplete_fuzzy: true

  # Query settings
  query_timeout_seconds: 300
  default_limit: 100
  max_rows_in_memory: 10000

  # Output format
  default_format: "table"  # or "json", "csv"
  max_column_width: 50
  truncate_long_content: true

  # Colors
  error_color: "red"
  warning_color: "yellow"
  success_color: "green"
  info_color: "cyan"
```

## Future Enhancements

### Phase 2 (Post-PoC)
- **Multiple query tabs**: Run queries in background, switch between them
- **Query result pagination**: Navigate large result sets without buffering
- **Query plan visualization**: EXPLAIN output with visual tree
- **Macro system**: `:macro add search_projects SELECT * FROM notes WHERE ...`
- **Pipe to external tools**: `:json | jq '.[] | .title'`

### Phase 3 (Advanced)
- **Remote REPL**: Connect to daemon from different terminal
- **Query history search**: Fuzzy search through past queries
- **Result set diff**: Compare two query results
- **Export to file**: `:export results.csv`
- **Watch mode**: Re-run query on file changes

## Conclusion

This REPL architecture prioritizes:

1. **User Experience**: Syntax highlighting, autocomplete, helpful errors
2. **Responsiveness**: Async-first, cancellable queries, non-blocking
3. **Simplicity**: Direct SurrealQL access, no abstraction overhead
4. **Extensibility**: Tool registry, Rune scripts, output formatters

**Key Technical Choices**:
- Reedline for modern line editing with async support
- Simple regex + manual parsing for commands (no parser combinator overhead)
- Background query execution with cancellation support
- Buffered results initially, streaming when needed
- Rich error formatting with color and context

The design balances implementation simplicity (PoC) with extensibility (production).
