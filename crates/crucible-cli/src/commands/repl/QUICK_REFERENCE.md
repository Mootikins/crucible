# REPL Testing Quick Reference

Quick guide for testing the REPL without spawning processes.

## Setup

```rust
use crucible_cli::commands::repl::{Repl, ReplCommand, ReplInput};

#[tokio::test]
async fn my_test() {
    let mut repl = Repl::new_test().await.unwrap();
    // ... test code
}
```

## Common Patterns

### Test Command Execution

```rust
// Process a command string
repl.process_input(":tools").await.unwrap();

// Or execute a command directly
repl.execute_command(ReplCommand::ListTools).await.unwrap();
```

### Test Query Execution

```rust
// Execute a query
repl.execute_query("SELECT * FROM notes").await.unwrap();

// Or process as input
repl.process_input("SELECT * FROM notes").await.unwrap();
```

### Verify Statistics

```rust
let stats = repl.get_stats();
assert_eq!(stats.command_count, 1);
assert_eq!(stats.query_count, 2);
assert!(stats.total_query_time.as_millis() > 0);
```

### Access Database

```rust
let db = repl.get_database();
let result = db.query("SELECT * FROM notes").await.unwrap();
assert!(!result.rows.is_empty());
```

### Test Error Handling

```rust
let result = repl.process_input(":invalid").await;
assert!(result.is_err());
```

### Test Parsing

```rust
let input = ReplInput::parse(":tools").unwrap();
assert!(input.is_command());

let input = ReplInput::parse("SELECT * FROM notes").unwrap();
assert!(input.is_query());
```

## API Cheat Sheet

| Method | Purpose | Returns |
|--------|---------|---------|
| `Repl::new_test()` | Create test REPL | `Result<Repl>` |
| `repl.process_input(s)` | Process input line | `Result<(), ReplError>` |
| `repl.execute_command(cmd)` | Execute command | `Result<(), ReplError>` |
| `repl.execute_query(q)` | Execute query | `Result<(), ReplError>` |
| `repl.get_core()` | Access Core coordinator | `&Arc<CrucibleCore>` |
| `repl.get_stats()` | Get statistics | `&ReplStats` |
| `ReplInput::parse(s)` | Parse input | `Result<Input, ReplError>` |
| `core.query(q)` | Query database | `Result<Vec<BTreeMap>, String>` |
| `core.list_tables()` | List tables | `Result<Vec<String>, String>` |

## Testing Checklist

- [ ] Test command parsing
- [ ] Test command execution
- [ ] Test query execution
- [ ] Test error handling
- [ ] Test statistics tracking
- [ ] Test database state
- [ ] Test empty input handling
- [ ] Test mixed command/query execution

## Common Assertions

```rust
// Statistics
assert_eq!(stats.command_count, expected);
assert_eq!(stats.query_count, expected);

// Results
assert!(result.is_ok());
assert!(result.is_err());

// Input parsing
assert!(input.is_command());
assert!(input.is_query());
assert!(input.is_empty());

// Database
assert!(!result.rows.is_empty());
assert_eq!(result.row_count(), expected);
```

## Full Example

```rust
#[tokio::test]
async fn test_repl_workflow() {
    // Setup
    let mut repl = Repl::new_test().await.unwrap();

    // Execute commands
    repl.process_input(":tools").await.unwrap();
    repl.process_input("SELECT * FROM notes").await.unwrap();

    // Verify statistics
    let stats = repl.get_stats();
    assert_eq!(stats.command_count, 1);
    assert_eq!(stats.query_count, 1);

    // Verify database state
    let db = repl.get_database();
    let result = db.query("SELECT * FROM notes").await.unwrap();
    assert!(!result.rows.is_empty());
}
```

## Tips

1. Use `new_test()` for each test - creates fresh state
2. In-memory database is fast and requires no cleanup
3. `process_input()` increments stats, `execute_command()` doesn't
4. All methods are async - don't forget `.await`
5. Check `TESTING.md` for detailed examples

## See Also

- `TESTING.md` - Comprehensive testing guide
- `tests/repl_testable_components.rs` - Working examples
- `/home/moot/crucible/REPL_REFACTORING_SUMMARY.md` - Full refactoring details
