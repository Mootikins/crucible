# Crucible Rune Macros

[![Crates.io](https://img.shields.io/crates/v/crucible-rune-macros.svg)](https://crates.io/crates/crucible-rune-macros)
[![Documentation](https://docs.rs/crucible-rune-macros/badge.svg)](https://docs.rs/crucible-rune-macros)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Procedural macros for generating Rune tools in the Crucible knowledge management system.

## Overview

`crucible-rune-macros` provides the `#[rune_tool]` attribute macro that automatically converts Rust functions into service-based tools. It handles:

- Automatic JSON schema generation for parameters
- Metadata extraction and storage
- Parameter validation
- Async function support
- Tool categorization and tagging
- Comprehensive compile-time error messages

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
crucible-rune-macros = "0.1.0"
```

Create your first tool:

```rust
use crucible_rune_macros::rune_tool;

#[rune_tool(
    desc = "Creates a new note with title and content",
    category = "file",
    tags = ["note", "create"]
)]
pub fn create_note(title: String, content: String) -> Result<String, String> {
    Ok(format!("Created note '{}' with {} characters", title, content.len()))
}

#[rune_tool(
    desc = "Searches for notes matching a query",
    category = "search",
    async
)]
pub async fn search_notes(query: String, limit: Option<i32>) -> Result<Vec<String>, String> {
    // Search implementation...
    Ok(vec!["note1.md".to_string(), "note2.md".to_string()])
}
```

## Features

### 🚀 **Automatic Schema Generation**
- Generates JSON Schema for tool parameters
- Supports complex types (arrays, objects, optionals)
- Validates parameter types at compile time

### 📝 **Rich Metadata**
- Extracts function signatures for tool definitions
- Supports categorization and tagging
- Handles async function detection

### ✅ **Compile-Time Validation**
- Comprehensive error messages with suggestions
- Validates function signatures
- Checks for reserved parameter names
- Ensures public visibility

### 🔧 **Developer Experience**
- Simple macro syntax
- Helpful error messages
- Works with standard Rust types
- Async-first design

## Macro Reference

### `#[rune_tool]`

Main attribute macro for creating tools.

#### Attributes

| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `desc` / `description` | String | ✅ | Human-readable description |
| `category` | String | ❌ | Tool category (file, search, etc.) |
| `async` | Flag | ❌ | Mark as async tool (auto-detected) |
| `tags` | Array | ❌ | Tags for discovery |

#### Function Requirements

- ✅ Must be public (`pub fn ...` or `pub async fn ...`)
- ❌ No `self` parameters (free functions only)
- ✅ Simple parameter names (identifiers only)
- ✅ Documentation comments recommended

#### Supported Types

| Rust Type | JSON Schema | Notes |
|-----------|-------------|-------|
| `String`, `&str` | `string` | Text values |
| `i32`, `i64`, `f64`, etc. | `number` | Numeric values |
| `bool` | `boolean` | True/false values |
| `Vec<T>` | `array` | Arrays of items |
| `Option<T>`, `T?` | `null` + type | Optional parameters |
| Custom structs | `object` | Complex objects |

### `#[simple_rune_tool]`

Simplified version that extracts description from doc comments:

```rust
use crucible_rune_macros::simple_rune_tool;

/// Creates a greeting message
#[simple_rune_tool]
pub fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
```

## Examples

### Basic Tool

```rust
#[rune_tool(desc = "Adds two numbers")]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### With Optional Parameters

```rust
#[rune_tool(desc = "Searches with optional pagination")]
pub fn search(
    query: String,
    limit: Option<i32>,  // Optional parameter
    offset: Option<i32>, // Optional parameter
) -> Vec<String> {
    // Implementation...
}
```

### Async Tool

```rust
#[rune_tool(
    desc = "Reads file asynchronously",
    category = "file",
    async
)]
pub async fn read_file(path: String) -> Result<String, String> {
    // Async implementation...
}
```

### With Complex Types

```rust
#[rune_tool(desc = "Processes array of strings")]
pub fn process_items(items: Vec<String>) -> Result<serde_json::Value, String> {
    let count = items.len();
    Ok(json!({"processed": count}))
}
```

### With Parameter Documentation

```rust
#[rune_tool(desc = "Creates user with validation")]
pub fn create_user(
    /// User's display name
    name: String,
    /// User's email address
    email: String,
    #[default = "user"] role: Option<String>,
) -> Result<String, String> {
    // Implementation...
}
```

## Generated JSON Schema

The macro automatically generates JSON Schema for validation:

```json
{
  "type": "object",
  "properties": {
    "title": {"type": "string"},
    "content": {"type": "string"},
    "folder": {"type": ["string", "null"]}
  },
  "required": ["title", "content"]
}
```

## Error Messages

The macro provides helpful compile-time errors:

```
error: Tool description is required
  --> src/lib.rs:15:1
   |
15 | #[rune_tool()]
   | ^^^^^^^^^^^^^^
   |
   = help: Add a description: `#[rune_tool(desc = "Your tool description")]`
```

## Architecture

The crate is organized into several modules:

- **`tool_macro`**: Main attribute macro implementation
- **`metadata_storage`**: Thread-safe storage for tool metadata
- **`schema_generator`**: JSON schema generation utilities
- **`ast_utils`**: AST analysis and validation utilities

## Runtime Usage

Tools can be discovered and used at runtime:

```rust
use crucible_rune_macros::metadata_storage::ToolMetadataStorage;

// Get all tools
let storage = ToolMetadataStorage::global();
let tools = storage.list_tools();

// Get tools by category
let file_tools = storage.get_by_category("file");

// Get specific tool metadata
let tool_meta = storage.get("create_note");
```

## Testing

The crate includes comprehensive tests:

```bash
# Run all tests
cargo test

# Run macro expansion tests
cargo test --test macro_tests

# Run compile-fail tests
cargo test --test compile_fail
```

## Performance

The macros are designed to have minimal runtime overhead:

- Zero-cost abstractions
- Compile-time metadata generation
- Efficient schema generation
- Thread-safe storage with minimal contention

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Related Crates

- [`crucible-core`](../crucible-core) - Core business logic
- [`crucible-services`](../crucible-services) - Service architecture
- [`crucible-rune`](../crucible-rune) - Rune scripting system
- [`crucible-tools`](../crucible-tools) - Static system tools

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history.