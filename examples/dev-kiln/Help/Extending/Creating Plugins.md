---
description: Build Rune plugins to extend Crucible's capabilities
status: implemented
tags:
  - extending
  - rune
  - plugins
aliases:
  - Plugin Development
  - Writing Plugins
---

# Creating Plugins

Crucible plugins are Rune scripts that automate tasks in your kiln. They can read notes, search content, and create new files.

## Quick Start

1. Create a script in `Scripts/`:

```rune
// Scripts/hello.rn

/// My first plugin
pub fn main() {
    println("Hello from my plugin!");
    Ok(())
}
```

2. Run it:

```bash
cru script "Scripts/hello.rn"
```

## Plugin Location

Place plugins in your kiln's `Scripts/` folder:

```
your-kiln/
├── Scripts/
│   ├── auto-tagging.rn
│   ├── daily-summary.rn
│   └── cleanup.rn
```

## Basic Structure

Every plugin needs a `main` function:

```rune
/// Plugin description (shown in help)
pub fn main() {
    // Your code here
    Ok(())
}
```

## Running Plugins

```bash
# Run a plugin
cru script "Scripts/my-plugin.rn"

# With act mode (allows file modifications)
cru script "Scripts/my-plugin.rn" --act

# With arguments
cru script "Scripts/my-plugin.rn" --arg value
```

## What Plugins Can Do

- **Read notes** - Access content and frontmatter
- **Search** - Semantic, text, and property searches
- **Create notes** - Generate new files (requires `--act`)
- **Update notes** - Modify frontmatter (requires `--act`)

See [[Help/Rune/Crucible API]] for all available functions.

## Example: Tag Suggester

```rune
/// Suggest tags for untagged notes
pub fn main() {
    let notes = crucible::search_by_properties(#{})?;

    for note in notes {
        let data = crucible::read_note(note.path)?;
        let tags = data.frontmatter.get("tags").unwrap_or([]);

        if tags.len() == 0 {
            let suggestions = suggest_tags(data.content);
            println("{}: {}", note.path, suggestions);
        }
    }

    Ok(())
}

fn suggest_tags(content) {
    let tags = [];
    if content.contains("TODO") { tags.push("actionable"); }
    if content.contains("```") { tags.push("has-code"); }
    tags
}
```

## Example: Daily Summary

```rune
/// Generate a summary of today's changes
pub fn main() {
    let today = crucible::today();

    let summary = format!("# Summary for {}\n\n", today);
    summary += "Notes modified today:\n";

    let notes = crucible::search_by_properties(#{})?;
    // Filter by today's changes...

    if crucible::is_act_mode() {
        crucible::create_note("Summaries/" + today + ".md", summary, #{
            tags: ["summary"]
        })?;
        println("Created summary");
    } else {
        println(summary);
        println("Run with --act to create file");
    }

    Ok(())
}
```

## Learning Rune

- [[Help/Rune/Language Basics]] - Syntax fundamentals
- [[Help/Rune/Crucible API]] - Available functions
- [[Help/Rune/Best Practices]] - Writing good plugins
- [[Help/Rune/Testing Plugins]] - Debugging and testing
- [[Help/Rune/Error Handling]] - Error handling patterns

## Examples in This Kiln

- [[Scripts/Auto Tagging]] - Tag suggestion
- [[Scripts/Daily Summary]] - Summary generation

## See Also

- [[Help/Extending/Event Hooks]] - React to events
- [[Help/Extending/Custom Tools]] - Create MCP tools
- [[Help/Rune/Rune vs Just]] - When to use Rune vs Just
- [[Extending Crucible]] - All extension points
