# REPL Testing Guide

This document describes how to test the REPL implementation without spawning processes.

## Overview

The REPL has been refactored to expose testable components while maintaining its external behavior. Key functionality can now be tested directly by calling public methods.

## Testable Components

### 1. Command Parsing (`Input::parse` and `Command::parse`)

These functions are already public and can be tested directly:

```rust
use crucible_cli::commands::repl::{Input, Command};

#[test]
fn test_command_parsing() {
    // Test command parsing
    let input = Input::parse(":tools").unwrap();
    assert!(input.is_command());

    // Test query parsing
    let input = Input::parse("SELECT * FROM notes").unwrap();
    assert!(input.is_query());
}
```

### 2. Command Processing (`Repl::process_input`)

This method is now `pub(crate)` and can be called directly in integration tests:

```rust
use crucible_cli::commands::repl::Repl;

#[tokio::test]
async fn test_process_command() {
    let mut repl = Repl::new_test().await.unwrap();

    // Process a command
    let result = repl.process_input(":tools").await;
    assert!(result.is_ok());

    // Verify statistics were updated
    let stats = repl.get_stats();
    assert_eq!(stats.command_count, 1);
}
```

### 3. Command Handlers (`Repl::execute_command`)

Individual command handlers can be tested:

```rust
use crucible_cli::commands::repl::{Repl, Command};

#[tokio::test]
async fn test_list_tools_command() {
    let mut repl = Repl::new_test().await.unwrap();

    // Execute the ListTools command
    let result = repl.execute_command(Command::ListTools).await;
    assert!(result.is_ok());

    // Verify command was counted
    assert_eq!(repl.get_stats().command_count, 1);
}
```

### 4. Query Execution (`Repl::execute_query`)

Query execution can be tested with the in-memory database:

```rust
use crucible_cli::commands::repl::Repl;

#[tokio::test]
async fn test_query_execution() {
    let mut repl = Repl::new_test().await.unwrap();

    // Execute a query
    let result = repl.execute_query("SELECT * FROM notes").await;
    assert!(result.is_ok());

    // Verify query was counted
    let stats = repl.get_stats();
    assert_eq!(stats.query_count, 1);
}
```

### 5. Database Integration via Core

Database queries are executed through CrucibleCore:

```rust
use crucible_cli::commands::repl::Repl;

#[tokio::test]
async fn test_database_queries() {
    let repl = Repl::new_test().await.unwrap();
    let core = repl.get_core();

    // Execute a query via Core
    let result = core.query("SELECT * FROM notes").await;
    assert!(result.is_ok());

    let query_result = result.unwrap();
    assert!(!query_result.is_empty());
}
```

### 6. Tool Registry Integration

The tool registry can be accessed and tested:

```rust
use crucible_cli::commands::repl::Repl;

#[tokio::test]
async fn test_tool_execution() {
    let mut repl = Repl::new_test().await.unwrap();

    // List available tools
    let tools = repl.get_tools().list_tools().await;
    assert!(!tools.is_empty());

    // Execute a tool (if available)
    // let result = repl.run_tool("tool_name", vec![]).await;
}
```

## New Public Interfaces

### Repl Methods

- `pub(crate) async fn process_input(&mut self, input: &str) -> Result<(), ReplError>`
  - Process a line of input (command or query)
  - Returns error if parsing or execution fails

- `pub(crate) async fn execute_command(&mut self, cmd: Command) -> Result<(), ReplError>`
  - Execute a specific command
  - Useful for testing individual command handlers

- `pub(crate) async fn execute_query(&mut self, query: &str) -> Result<(), ReplError>`
  - Execute a SurrealQL query
  - Useful for testing query execution and formatting

- `pub fn get_core(&self) -> &Arc<CrucibleCore>`
  - Access the Core coordinator
  - Allows database queries via `core.query()` and `core.list_tables()` in tests

- `pub(crate) fn get_stats(&self) -> &ReplStats`
  - Access execution statistics
  - Verify command/query counts and timing

- `#[cfg(test)] pub async fn new_test() -> Result<Self>`
  - Create a test REPL with in-memory database
  - No file system dependencies
  - Populates database with sample data from examples/test-kiln

### CrucibleCore Methods

Core provides database access through a facade pattern:

- `pub async fn query(&self, query: &str) -> Result<Vec<BTreeMap<String, serde_json::Value>>, String>`
  - Execute SurrealQL queries

- `pub async fn list_tables(&self) -> Result<Vec<String>, String>`
  - Get list of database tables (used for autocomplete)

- `pub async fn query(&self, query_str: &str) -> Result<QueryResult, String>`
  - Execute a query and return formatted results

- `pub async fn list_tables(&self) -> Result<Vec<String>>`
  - Get list of available tables

- `pub async fn get_stats(&self) -> Result<BTreeMap<String, serde_json::Value>>`
  - Get database statistics

### ReplConfig

- `pub(crate) fn from_cli_config(...) -> Result<Self>`
  - Create config from CLI config
  - Useful for custom test setups

### ReplStats

- `pub(crate) command_count: usize` - Number of commands executed
- `pub(crate) query_count: usize` - Number of queries executed
- `pub(crate) total_query_time: Duration` - Total query execution time
- `pub(crate) fn avg_query_time(&self) -> Duration` - Average query time

## Example Test Suite

```rust
#[cfg(test)]
mod repl_integration_tests {
    use super::*;
    use crucible_cli::commands::repl::{Repl, Command, Input};

    #[tokio::test]
    async fn test_command_execution_flow() {
        let mut repl = Repl::new_test().await.unwrap();

        // Execute multiple commands
        repl.process_input(":tools").await.unwrap();
        repl.process_input(":config").await.unwrap();
        repl.process_input(":stats").await.unwrap();

        // Verify statistics
        let stats = repl.get_stats();
        assert_eq!(stats.command_count, 3);
        assert_eq!(stats.query_count, 0);
    }

    #[tokio::test]
    async fn test_query_execution_flow() {
        let mut repl = Repl::new_test().await.unwrap();

        // Execute queries
        repl.process_input("SELECT * FROM notes").await.unwrap();
        repl.process_input("SELECT * FROM tags").await.unwrap();

        // Verify statistics
        let stats = repl.get_stats();
        assert_eq!(stats.command_count, 0);
        assert_eq!(stats.query_count, 2);
    }

    #[tokio::test]
    async fn test_mixed_execution() {
        let mut repl = Repl::new_test().await.unwrap();

        // Mix commands and queries
        repl.process_input(":tools").await.unwrap();
        repl.process_input("SELECT * FROM notes").await.unwrap();
        repl.process_input(":stats").await.unwrap();

        // Verify statistics
        let stats = repl.get_stats();
        assert_eq!(stats.command_count, 2);
        assert_eq!(stats.query_count, 1);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let mut repl = Repl::new_test().await.unwrap();

        // Invalid command should fail
        let result = repl.process_input(":invalid").await;
        assert!(result.is_err());

        // Statistics should not be incremented on error
        let stats = repl.get_stats();
        assert_eq!(stats.command_count, 0);
    }

    #[tokio::test]
    async fn test_database_state() {
        let mut repl = Repl::new_test().await.unwrap();

        // Execute query
        repl.execute_query("SELECT * FROM notes").await.unwrap();

        // Verify database state directly
        let db = repl.get_database();
        let result = db.query("SELECT * FROM notes").await.unwrap();
        assert!(!result.rows.is_empty());
    }
}
```

## Testing Best Practices

1. **Use `new_test()` for isolation**: Each test should create its own REPL instance
2. **Test at the right level**:
   - Unit tests for parsing logic
   - Integration tests for command execution
   - Direct database tests for query behavior
3. **Verify side effects**: Check statistics, database state, etc.
4. **Test error conditions**: Invalid commands, malformed queries
5. **Use in-memory database**: Faster and no cleanup required

## Limitations

- The `quit()` method calls `std::process::exit()` and cannot be tested directly
- Progress indicators and terminal output are difficult to capture
- Some tool execution depends on file system state
- History persistence requires file system access

## Migration from Process-Based Tests

If you have existing tests that spawn the REPL process:

**Before:**
```rust
let output = Command::new("crucible-cli")
    .args(&["repl", "--non-interactive"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;
```

**After:**
```rust
let mut repl = Repl::new_test().await?;
repl.process_input(":tools").await?;
```

Benefits:
- Faster execution (no process spawning)
- Direct access to internal state
- Better error messages
- Easier to debug
- No cleanup required
