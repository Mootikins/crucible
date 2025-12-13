---
title: Writing Plugins
description: Create Rune plugins to extend Crucible functionality
tags:
  - reference
  - extending
  - rune
---

# Writing Plugins

Crucible plugins are written in Rune, a lightweight scripting language designed for embedding in Rust applications. Plugins can add custom tools, automate workflows, and extend the AI agent's capabilities.

## Plugin Basics

### File Location

Place plugin scripts in your kiln:

```
my-kiln/
  Scripts/
    auto-tagging.rn
    daily-summary.rn
    custom-tool.rn
```

### Basic Structure

```rune
// my-plugin.rn

/// Plugin description
/// This appears in help output

/// Main entry point
pub fn main() {
    println("Hello from plugin!");
    Ok(())
}
```

## Rune Language Basics

### Variables and Types

```rune
// Immutable binding
let name = "value";

// Mutable binding
let mut count = 0;
count += 1;

// Types
let text = "string";
let number = 42;
let decimal = 3.14;
let flag = true;
let items = ["a", "b", "c"];
let map = #{ key: "value" };
```

### Functions

```rune
/// Public function (callable from outside)
pub fn greet(name) {
    format!("Hello, {}!", name)
}

// Private function
fn helper() {
    // internal use
}
```

### Control Flow

```rune
// Conditionals
if condition {
    // ...
} else if other {
    // ...
} else {
    // ...
}

// Loops
for item in items {
    println(item);
}

while condition {
    // ...
}
```

### Error Handling

```rune
// Return Result
pub fn might_fail() {
    if something_wrong {
        return Err("error message");
    }
    Ok(result)
}

// Propagate errors with ?
let value = might_fail()?;

// Handle errors
match might_fail() {
    Ok(v) => println("Got: {}", v),
    Err(e) => println("Error: {}", e),
}
```

## Crucible API

### Reading Notes

```rune
// Read a note by path
let note = crucible::read_note("path/to/note.md")?;

// Access content
let content = note.content;
let frontmatter = note.frontmatter;

// Get specific frontmatter field
let tags = frontmatter.get("tags").unwrap_or([]);
```

### Searching

```rune
// Semantic search
let results = crucible::semantic_search("query", 10)?;

// Text search
let matches = crucible::text_search("pattern", #{
    folder: "Projects",
    limit: 20
})?;

// Property search
let notes = crucible::search_by_properties(#{
    tags: ["important"]
})?;
```

### Creating/Updating Notes

**Requires act mode!**

```rune
// Create a new note
crucible::create_note("New Note.md", content, #{
    tags: ["auto-generated"],
    created: crucible::today()
})?;

// Update frontmatter
crucible::update_frontmatter("note.md", #{
    status: "reviewed",
    reviewed_at: crucible::now()
})?;

// Append to note
crucible::append_to_note("note.md", "\n\n## New Section\n\nContent here.")?;
```

### Mode Checking

```rune
// Check if we can modify files
if crucible::is_act_mode() {
    crucible::create_note(...)?;
} else {
    println("Run with --act to create notes");
}
```

### Utilities

```rune
// Dates
let today = crucible::today();     // 2024-01-15
let now = crucible::now();         // 2024-01-15T14:30:00

// Paths
let kiln = crucible::kiln_path();

// Configuration
let config = crucible::get_config("embedding.provider");
```

## Example Plugins

### Auto-Tagger

See [[Scripts/Auto Tagging]] for a complete example:

```rune
pub fn suggest_tags(content) {
    let suggestions = [];

    if content.contains("TODO") {
        suggestions.push("actionable");
    }

    if content.contains("```") {
        suggestions.push("has-code");
    }

    suggestions
}
```

### Daily Summary

See [[Scripts/Daily Summary]] for a complete example.

### Custom MCP Tool

Register a plugin as an MCP tool:

```rune
// Export tool definition
pub const TOOL_DEFINITION = #{
    name: "my_tool",
    description: "Does something useful",
    parameters: #{
        input: #{ type: "string", required: true }
    }
};

// Tool implementation
pub fn my_tool(params) {
    let input = params.get("input")?;

    // Process input...
    let result = process(input);

    Ok(#{
        output: result
    })
}
```

## Running Plugins

### From CLI

```bash
# Run a script
cru script "Scripts/my-plugin.rn"

# With arguments
cru script "Scripts/my-plugin.rn" --arg value

# In act mode (allows writes)
cru script "Scripts/my-plugin.rn" --act
```

### From Chat

```
/run Scripts/my-plugin.rn
```

### Via MCP

If exported as a tool, plugins are available to MCP clients.

## Best Practices

### 1. Handle Errors Gracefully

```rune
pub fn main() {
    match process_notes() {
        Ok(count) => println("Processed {} notes", count),
        Err(e) => println("Error: {}", e),
    }
    Ok(())
}
```

### 2. Respect Mode

```rune
if crucible::is_act_mode() {
    // Modify files
} else {
    println("Would modify files. Run with --act to apply changes.");
}
```

### 3. Provide Progress Feedback

```rune
for (i, note) in notes.iter().enumerate() {
    println("[{}/{}] Processing: {}", i + 1, notes.len(), note.path);
    process(note)?;
}
```

### 4. Document Your Plugin

```rune
/// My Plugin
///
/// Processes notes and does useful things.
///
/// Usage:
///   cru script "Scripts/my-plugin.rn" --arg value
///
/// Options:
///   --arg: Description of argument
```

## Debugging

### Print Statements

```rune
println("Debug: value = {}", value);
println("Debug: note = {:?}", note);
```

### Dry Run Mode

Add a dry-run option:

```rune
pub fn main(dry_run) {
    let dry_run = dry_run.unwrap_or(true);

    if dry_run {
        println("DRY RUN - no changes will be made");
    }

    // ... processing ...

    if !dry_run && crucible::is_act_mode() {
        crucible::create_note(...)?;
    }
}
```

## Limitations

- No network access (security)
- No filesystem access outside kiln
- Limited to Crucible's API
- Single-threaded execution

## Implementation

**Rune integration:** `crates/crucible-rune/src/`

**Built-in modules:** `crates/crucible-rune/src/` (plugin_loader.rs, registry.rs)

## See Also

- [[Scripts/Auto Tagging]] - Example tagging script
- [[Scripts/Daily Summary]] - Example summary script
- [[Help/Extending/Custom Tools]] - Creating MCP tools
- `:h mcp` - MCP server reference
