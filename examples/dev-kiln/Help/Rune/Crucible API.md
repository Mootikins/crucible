---
title: Crucible API
description: Built-in functions available in Rune plugins for interacting with your kiln
status: implemented
tags:
  - rune
  - api
  - reference
---

# Crucible API

These functions are available in Rune plugins for reading, searching, and creating notes.

## Reading Notes

### `crucible::read_note(path)`

Read a note's content and metadata.

```rune
let note = crucible::read_note("Projects/todo.md")?;

let content = note.content;           // Full markdown text
let frontmatter = note.frontmatter;   // Parsed YAML as object
let tags = frontmatter.get("tags").unwrap_or([]);
```

## Searching

### `crucible::semantic_search(query, limit)`

Find notes by meaning similarity.

```rune
let results = crucible::semantic_search("project management", 10)?;

for result in results {
    println("{}: {}", result.path, result.score);
}
```

### `crucible::text_search(pattern, options)`

Find notes by text pattern.

```rune
let matches = crucible::text_search("TODO", #{
    folder: "Projects",
    limit: 20
})?;
```

### `crucible::search_by_properties(properties)`

Find notes by frontmatter properties.

```rune
let notes = crucible::search_by_properties(#{
    tags: ["important"],
    status: "active"
})?;
```

### `crucible::search_by_tags(tags)`

Find notes with specific tags.

```rune
let notes = crucible::search_by_tags(["project", "active"])?;
```

## Creating and Updating Notes

**These require act mode!** Check with `crucible::is_act_mode()`.

### `crucible::create_note(path, content, frontmatter)`

Create a new note.

```rune
if crucible::is_act_mode() {
    crucible::create_note("Daily/2024-01-15.md", content, #{
        title: "January 15, 2024",
        tags: ["daily"],
        created: crucible::today()
    })?;
}
```

### `crucible::update_frontmatter(path, properties)`

Update note frontmatter.

```rune
if crucible::is_act_mode() {
    crucible::update_frontmatter("Projects/todo.md", #{
        status: "completed",
        completed_at: crucible::now()
    })?;
}
```

### `crucible::append_to_note(path, content)`

Add content to end of note.

```rune
if crucible::is_act_mode() {
    crucible::append_to_note("Journal.md", "\n\n## New Entry\n\nContent here.")?;
}
```

## Mode Checking

### `crucible::is_act_mode()`

Check if the plugin can modify files.

```rune
if crucible::is_act_mode() {
    crucible::create_note(...)?;
} else {
    println("Run with --act to create notes");
}
```

## Utilities

### Dates and Times

```rune
let today = crucible::today();    // "2024-01-15"
let now = crucible::now();        // "2024-01-15T14:30:00"
```

### Paths

```rune
let kiln = crucible::kiln_path(); // "/path/to/kiln"
```

### Configuration

```rune
let provider = crucible::get_config("embedding.provider");
```

## Complete Example

```rune
/// Find notes missing tags and suggest some
pub fn suggest_tags() {
    let notes = crucible::search_by_properties(#{})?;

    for note in notes {
        let data = crucible::read_note(note.path)?;
        let tags = data.frontmatter.get("tags").unwrap_or([]);

        if tags.len() == 0 {
            let content = data.content;
            let suggestions = analyze_content(content);

            println("{}: suggest {}", note.path, suggestions);

            if crucible::is_act_mode() && suggestions.len() > 0 {
                crucible::update_frontmatter(note.path, #{
                    tags: suggestions
                })?;
            }
        }
    }

    Ok(())
}

fn analyze_content(content) {
    let tags = [];

    if content.contains("TODO") || content.contains("FIXME") {
        tags.push("actionable");
    }
    if content.contains("```") {
        tags.push("has-code");
    }
    if content.contains("[[") {
        tags.push("linked");
    }

    tags
}
```

## Error Handling

All API functions return `Result`. Handle errors appropriately:

```rune
pub fn safe_read(path) {
    match crucible::read_note(path) {
        Ok(note) => {
            // Process note
            Ok(note)
        }
        Err(e) => {
            println("Could not read {}: {}", path, e);
            Err(e)
        }
    }
}
```

## See Also

- [[Help/Rune/Language Basics]] - Rune syntax
- [[Help/Rune/Best Practices]] - Writing good plugins
- [[Help/Extending/Creating Plugins]] - Plugin overview
