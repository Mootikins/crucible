# Crucible Daemon

Terminal-first daemon for the Crucible knowledge management system.

## Overview

The Crucible daemon is a **REPL-based interface** for interacting with your knowledge vault through SurrealQL queries and built-in tools. It runs continuously, watching your vault for changes, indexing content, and providing real-time query capabilities.

## Features

- **SurrealQL REPL**: Direct database queries with syntax highlighting and autocomplete
- **Built-in Commands**: Tool execution, statistics, configuration management
- **Async Query Execution**: Non-blocking queries with cancellation support (Ctrl+C)
- **Multiple Output Formats**: Table (human), JSON (machines), CSV (export)
- **Command History**: Persistent history with fuzzy search (Ctrl+R)
- **Syntax Highlighting**: SurrealQL keyword and function highlighting
- **Autocomplete**: Commands, tool names, table names, keywords
- **Rich Error Messages**: Contextual errors with helpful suggestions

## Architecture

See [`/home/moot/crucible/docs/REPL_ARCHITECTURE.md`](/home/moot/crucible/docs/REPL_ARCHITECTURE.md) for detailed design decisions and implementation rationale.

### Key Components

```
src/
├── main.rs              # Entry point, daemon initialization
└── repl/
    ├── mod.rs           # Repl struct, main loop, state management
    ├── command.rs       # Command enum and parsing
    ├── input.rs         # Input routing (command vs query)
    ├── formatter.rs     # Output formatters (table, JSON, CSV)
    ├── error.rs         # Error types and display
    ├── history.rs       # Command history management
    ├── highlighter.rs   # Syntax highlighting
    └── completer.rs     # Autocomplete
```

## Usage

### Starting the Daemon

```bash
cargo run --bin crucible-daemon
```

### REPL Commands

#### Tools

```
:tools                      # List available tools
:run <tool> [args...]       # Execute a tool
:rune <script> [args...]    # Run a Rune script
```

Examples:
```
:run search_by_tags project ai
:run semantic_search "agent orchestration"
:rune custom_query.rn
```

#### Information

```
:stats                      # Show vault statistics
:config                     # Display configuration
:history [limit]            # Show command history
```

#### Configuration

```
:log <level>                # Set log level (trace|debug|info|warn|error)
:format <fmt>               # Set output format (table|json|csv)
```

#### Utility

```
:help [command]             # Show help
:clear                      # Clear screen
:quit                       # Exit daemon
```

### SurrealQL Queries

Any input not starting with `:` is treated as a SurrealQL query:

```sql
-- Basic queries
SELECT * FROM notes;
SELECT title, tags FROM notes WHERE tags CONTAINS '#project';

-- Graph traversal
SELECT ->links->note.title FROM notes WHERE path = 'foo.md';

-- Aggregation
SELECT COUNT() AS total FROM notes GROUP BY tags;

-- Complex queries
SELECT
    path,
    title,
    array::len(tags) AS tag_count
FROM notes
WHERE tags CONTAINS '#project'
ORDER BY modified DESC
LIMIT 10;
```

### Keyboard Shortcuts

- **Ctrl+C**: Cancel running query (or show quit message if idle)
- **Ctrl+D**: Exit REPL
- **Ctrl+R**: Search command history
- **Tab**: Autocomplete commands, tool names, table names

## Configuration

Configuration file: `~/.crucible/config.yaml`

```yaml
repl:
  history_file: "~/.crucible/history"
  history_max_entries: 10000
  prompt: "> "
  edit_mode: "emacs"  # or "vi"
  syntax_highlighting: true
  autocomplete_enabled: true
  query_timeout_seconds: 300
  default_format: "table"
  max_column_width: 50
```

## Design Decisions

### Why Reedline over Rustyline?

- Modern async-first design
- Better Tokio integration
- Rich editing features (Vi/Emacs modes)
- Maintained by Nushell team
- Extensible architecture

### Why Simple Parsing over Parser Combinators?

- Commands have simple structure (`:cmd arg1 arg2`)
- Regex sufficient for prefix detection
- Full control over error messages
- Zero additional dependencies
- Easy for contributors to understand

### Why Async Query Execution?

- Prevents blocking on long-running queries
- User can cancel queries with Ctrl+C
- Progress indicators during execution
- Future: multiple concurrent queries

### Why Buffer Results (PoC)?

- Table formatting requires column widths (needs full data)
- Simpler error handling
- Most queries return <100 rows
- Can add streaming when needed (YAGNI)

## Testing

Run tests:
```bash
cargo test --package crucible-daemon
```

Run specific test module:
```bash
cargo test --package crucible-daemon repl::command::tests
```

## Development

### Adding a New Command

1. Add variant to `Command` enum in `command.rs`
2. Add parsing logic in `Command::parse()`
3. Add help text in `Command::help_for_command()`
4. Add execution logic in `Repl::execute_command()`
5. Add tests

Example:
```rust
// 1. Add variant
pub enum Command {
    // ... existing variants
    MyNewCommand { arg: String },
}

// 2. Parse it
match cmd {
    "mynew" => {
        let arg = args.get(0).ok_or(...)?;
        Ok(Command::MyNewCommand { arg: arg.to_string() })
    }
}

// 3. Help text
":mynew" => Some("...help text..."),

// 4. Execute
Command::MyNewCommand { arg } => {
    self.do_my_new_thing(arg).await
}

// 5. Test
#[test]
fn test_mynew_command() { ... }
```

### Adding a New Output Format

1. Create struct implementing `OutputFormatter` trait
2. Add variant to `OutputFormat` enum
3. Update `Repl::set_output_format()`
4. Add tests

### Extending Syntax Highlighting

Edit `highlighter.rs`:
- Add keywords to `build_keyword_set()`
- Add functions to `build_function_set()`
- Modify `highlight_token()` for custom highlighting

### Extending Autocomplete

Edit `completer.rs`:
- Add completion logic to `Completer::complete()`
- Create helper methods for specific contexts
- Add tests for new completion scenarios

## Performance

### Expected Latency

| Operation | Latency | Notes |
|-----------|---------|-------|
| Command parsing | <1ms | Regex + split, minimal allocation |
| Query execution | 1-500ms | Depends on complexity |
| Result formatting (100 rows) | 5-10ms | Table layout calculation |
| History search | <10ms | Linear scan, max 10k entries |
| Autocomplete | <20ms | Cached database schema |
| Syntax highlighting | <1ms | Incremental, per keystroke |

### Memory Footprint

- Base REPL: ~2MB
- Query result buffer: ~100KB per 1000 rows
- Tool registry: ~500KB
- Total: <10MB typical usage

## Future Enhancements

### Phase 2 (Post-PoC)
- Multiple query tabs (background execution)
- Query result pagination
- Query plan visualization (EXPLAIN)
- Macro system (`:macro add <name> <query>`)
- Pipe to external tools (`:json | jq`)

### Phase 3 (Advanced)
- Remote REPL (connect from different terminal)
- Query history search (fuzzy)
- Result set diff (compare two queries)
- Export to file (`:export results.csv`)
- Watch mode (re-run on file changes)

## Related Documentation

- [REPL Architecture](/home/moot/crucible/docs/REPL_ARCHITECTURE.md) - Detailed design document
- [Project README](/home/moot/crucible/README.md) - Crucible project overview

## License

See LICENSE file in repository root.
