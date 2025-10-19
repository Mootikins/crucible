# Database Access in Rune Tools

This document describes how to use database access in Rune tools.

## Overview

Rune tools can execute SurrealQL queries against the Crucible database using the `db` module. This enables tools to search notes, query metadata, and retrieve information from the knowledge base.

## API

The `db` module provides two functions:

### `db::query_simple(query: String) -> Vec<String>`

Execute a SurrealQL query without parameters.

**Parameters:**
- `query`: SurrealQL query string

**Returns:**
- Vector of results as strings

**Example:**
```rune
pub fn main() {
    let results = db::query_simple("SELECT * FROM notes LIMIT 10");

    if results.is_empty() {
        "No notes found"
    } else {
        `Found ${results.len()} notes`
    }
}
```

### `db::query(query: String, params: Vec<String>) -> Vec<String>`

Execute a SurrealQL query with parameters.

**Parameters:**
- `query`: SurrealQL query string with `?` placeholders
- `params`: Vector of parameter values

**Returns:**
- Vector of results as strings

**Example:**
```rune
pub fn main(tag) {
    let results = db::query(
        "SELECT * FROM notes WHERE tags CONTAINS ?",
        [tag]
    );

    `Found ${results.len()} notes with tag '${tag}'`
}
```

## Usage in ToolRegistry

To enable database access in the ToolRegistry:

```rust
use crucible_daemon::tools::{ToolRegistry, DbHandle};

// Create registry with database access
let db_handle = DbHandle::new();
let registry = ToolRegistry::new(tool_dir)?
    .with_database(db_handle)?;

// Discover and load tools (they now have access to db module)
registry.discover_tools().await?;

// Execute a tool that uses database queries
let result = registry.execute_tool("search", &["tag:project"]).await?;
```

## Implementation Status

### Current (Placeholder)

The database module is currently implemented as a placeholder:
- `db::query_simple()` returns an empty vector
- `db::query()` returns an empty vector
- All queries compile and run without errors

### Future (Production)

When SurrealDB integration is complete:
- Queries will be executed against actual SurrealDB instance
- Results will be returned as JSON-serializable values
- Error handling will report database errors properly
- Connection pooling and caching will be implemented

## Examples

See the `examples/` directory for complete examples:
- `db_list_notes.rn` - List notes using query_simple
- `db_search_tags.rn` - Search by tag using parameterized query

## Architecture

### Module Registration

The database module is registered when creating a ToolRegistry with database access:

1. Create `DbHandle` (placeholder for database connection)
2. Call `with_database(db_handle)` on registry
3. This installs the `db` module into the Rune context
4. All subsequently compiled tools have access to `db::query()` and `db::query_simple()`

### Function Implementation

Functions are implemented using the `#[rune::function]` macro:

```rust
#[rune::function]
fn query_simple(query: &str) -> Vec<String> {
    // TODO: Execute against real database
    Vec::new()
}

#[rune::function]
fn query(query_str: &str, params: Vec<String>) -> Vec<String> {
    // TODO: Execute against real database with parameters
    Vec::new()
}
```

### Database Handle

The `DbHandle` struct will eventually wrap:
- SurrealDB client connection
- Connection pool
- Query cache
- Transaction support

For now, it's a placeholder that allows the API to be stable while database integration is completed.

## Testing

Tests verify:
- Tools can compile with `db::query()` calls
- Registry can be created with database access
- Database module is properly installed
- Tools execute without runtime errors

Run tests with:
```bash
cargo test --package crucible-daemon --test tool_registry test_tool_with_database_access
```

## SurrealQL Reference

Common query patterns for Crucible:

### Select all notes
```sql
SELECT * FROM notes LIMIT 10
```

### Filter by tag
```sql
SELECT * FROM notes WHERE tags CONTAINS 'project'
```

### Search by title
```sql
SELECT * FROM notes WHERE title ~ 'search term'
```

### Get note by path
```sql
SELECT * FROM notes WHERE path = '/path/to/note.md'
```

### Count notes
```sql
SELECT count() FROM notes GROUP ALL
```

## Error Handling

Currently, database functions return empty results on error. In production:

```rune
pub fn main(tag) {
    // Future: queries will return Result type
    match db::query("SELECT * FROM notes WHERE tags CONTAINS ?", [tag]) {
        Ok(results) => format_results(results),
        Err(e) => `Database error: ${e}`
    }
}
```

## Performance Considerations

When implementing real database access:
- Use connection pooling to avoid connection overhead
- Cache frequently accessed queries
- Limit result set sizes with LIMIT clauses
- Use indexes for common query patterns
- Consider async/await for non-blocking queries

## Security

Future implementation will include:
- Query sanitization to prevent injection
- Parameter binding for all user inputs
- Access control based on user permissions
- Query timeout limits
- Resource usage limits
