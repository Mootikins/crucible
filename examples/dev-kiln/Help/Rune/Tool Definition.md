---
description: Define tools with #[tool] and #[param] metadata attributes
status: implemented
tags:
  - rune
  - tools
  - reference
aliases:
  - Tool Metadata
  - Tool Attributes
---

# Tool Definition

Crucible supports defining custom tools in Rune scripts using metadata attributes. Tools defined this way become available to AI agents through the Model Context Protocol (MCP).

## The `#[tool]` Attribute

The `#[tool(...)]` attribute marks a function as an agent-callable tool. It accepts these parameters:

- **desc**: Human-readable description of what the tool does (required)
- **version**: Semantic version string (optional, defaults to "0.1.0")
- **tags**: Array of categorization tags (optional)

```rune
#[tool(desc = "Creates a new note in the vault")]
pub fn create_note(title, content) {
    // Implementation
}
```

## The `#[param]` Attribute

Each parameter needs a `#[param(...)]` attribute to define its schema. Place these directly before the `#[tool]` attribute.

Parameters for `#[param]`:

- **name**: Parameter name matching the function signature (required)
- **type**: JSON Schema type (required)
- **desc**: Human-readable description (required)
- **required**: Whether the parameter is mandatory (optional, defaults to true)

```rune
#[param(name = "title", type = "string", desc = "The note title")]
#[param(name = "tags", type = "array", desc = "Optional tags", required = false)]
#[tool(desc = "Creates a tagged note")]
pub fn create_tagged_note(title, tags) {
    // Implementation
}
```

## Supported Types

The `type` parameter accepts JSON Schema primitive types:

- **string**: Text values
- **number**: Numeric values (integers or floats)
- **boolean**: True/false values
- **array**: Lists of values
- **object**: Key-value structures

## Complete Example

Here's a full example with multiple parameters of different types:

```rune
/// Search for notes matching criteria
#[param(name = "query", type = "string", desc = "Search query text")]
#[param(name = "max_results", type = "number", desc = "Maximum results to return", required = false)]
#[param(name = "include_archived", type = "boolean", desc = "Include archived notes", required = false)]
#[param(name = "tags", type = "array", desc = "Filter by these tags", required = false)]
#[tool(
    desc = "Search notes with optional filters",
    version = "1.0.0",
    tags = ["search", "query"]
)]
pub fn search_notes(query, max_results, include_archived, tags) {
    let results = [];
    // Search implementation
    results
}
```

## Tool Naming

When Crucible discovers tools in Rune scripts, they are automatically prefixed with `rune_` to distinguish them from built-in tools:

- Function `create_note` becomes tool `rune_create_note`
- Function `search_vault` becomes tool `rune_search_vault`

This prevents naming conflicts and makes the tool's origin clear.

## Parameter Limits

Rune functions used as tools support up to 4 positional parameters. If you need more than 4 parameters, they will be passed as an array. For complex parameter structures, consider using the `object` type.

## Legacy Format

Crucible also supports a legacy single-tool format using `//!` comments at the top of the file. However, the modern `#[tool]` and `#[param]` attributes are preferred as they support multiple tools per file and provide better type safety.

## See Also

- [[Help/Rune/Language Basics]] - Rune language fundamentals
- [[Help/Extending/Custom Tools]] - Creating custom tool implementations
- [[Help/Rune/Crucible API]] - Built-in Crucible functions for Rune scripts
