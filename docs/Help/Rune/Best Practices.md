---
description: Guidelines for writing effective and reliable Crucible plugins
status: implemented
tags:
  - rune
  - best-practices
  - reference
aliases:
  - Rune Guidelines
  - Plugin Best Practices
---

# Rune Best Practices

Guidelines for writing plugins that are reliable, maintainable, and user-friendly.

## Always Handle Errors

Don't let errors crash your plugin silently.

```rune
// Good: Handle the error
pub fn main() {
    match process_notes() {
        Ok(count) => println("Processed {} notes", count),
        Err(e) => println("Error: {}", e),
    }
    Ok(())
}

// Bad: Ignore errors
pub fn main() {
    process_notes()?;  // Crashes if error
    Ok(())
}
```

## Respect Act Mode

Always check before modifying files.

```rune
// Good: Check mode and inform user
if crucible::is_act_mode() {
    crucible::create_note(...)?;
    println("Created note");
} else {
    println("Would create note. Run with --act to apply.");
}

// Bad: Assume we can write
crucible::create_note(...)?;  // Fails if not in act mode
```

## Provide Progress Feedback

Let users know what's happening, especially in long operations.

```rune
pub fn process_all() {
    let notes = crucible::search_by_properties(#{})?;

    for (i, note) in notes.iter().enumerate() {
        println("[{}/{}] Processing: {}", i + 1, notes.len(), note.path);
        process(note)?;
    }

    println("Done! Processed {} notes", notes.len());
    Ok(())
}
```

## Check Before Accessing

Don't assume fields exist. Check first.

```rune
// Good: Check before accessing
if let Some(tags) = note.frontmatter.get("tags") {
    for tag in tags {
        println("Tag: {}", tag);
    }
}

// Bad: Assume it exists
let tags = note.frontmatter.tags;  // Crashes if missing
```

## Document Your Plugins

Add a doc comment explaining what the plugin does.

```rune
/// Auto-Tagger Plugin
///
/// Analyzes note content and suggests appropriate tags.
///
/// Usage:
///   cru script "Scripts/auto-tag.rn"
///   cru script "Scripts/auto-tag.rn" --act  # Apply changes
///
/// What it does:
///   - Finds notes without tags
///   - Analyzes content for keywords
///   - Suggests or applies tags

pub fn main() {
    // ...
}
```

## Use Dry Run by Default

Make the safe option the default.

```rune
pub fn main(apply) {
    let apply = apply.unwrap_or(false);

    if !apply {
        println("DRY RUN - no changes will be made");
        println("Run with --apply to make changes");
    }

    // ... processing ...

    if apply && crucible::is_act_mode() {
        // Make changes
    }

    Ok(())
}
```

## Keep Plugins Focused

One plugin, one responsibility.

```rune
// Good: Single purpose
/// Suggest tags for untagged notes
pub fn suggest_tags() { ... }

// Bad: Does everything
/// Tag notes, create daily notes, clean up old files, and more
pub fn do_everything() { ... }
```

## Debugging Tips

### Print Debug Output

```rune
println("Debug: note = {:?}", note);
println("Debug: tags = {:?}", tags);
```

### Check Intermediate Values

```rune
let notes = crucible::search_by_properties(#{})?;
println("Found {} notes", notes.len());

for note in notes {
    println("Processing: {}", note.path);
    let data = crucible::read_note(note.path)?;
    println("  Content length: {}", data.content.len());
    println("  Frontmatter: {:?}", data.frontmatter);
}
```

### Add a Verbose Flag

```rune
pub fn main(verbose) {
    let verbose = verbose.unwrap_or(false);

    if verbose {
        println("Verbose mode enabled");
    }

    // ... processing ...

    if verbose {
        println("Debug info here");
    }
}
```

## Plugin Limitations

Be aware of what plugins can't do:

- **No network access** - Security restriction
- **No filesystem outside kiln** - Sandboxed
- **Single-threaded** - No concurrency
- **Limited to Crucible API** - No arbitrary system calls

## Common Patterns

### Process All Notes

```rune
pub fn main() {
    let notes = crucible::search_by_properties(#{})?;

    for note in notes {
        if should_process(note) {
            process(note)?;
        }
    }

    Ok(())
}
```

### Filter by Criteria

```rune
pub fn find_incomplete() {
    let notes = crucible::search_by_properties(#{
        status: "in-progress"
    })?;

    for note in notes {
        println("{}", note.path);
    }

    Ok(())
}
```

### Generate Summary

```rune
pub fn daily_summary() {
    let today = crucible::today();
    let notes = crucible::search_by_properties(#{
        modified: today
    })?;

    let summary = format!("# Summary for {}\n\nModified {} notes today.\n", today, notes.len());

    for note in notes {
        summary += format!("- [[{}]]\n", note.path);
    }

    if crucible::is_act_mode() {
        crucible::create_note("Summaries/" + today + ".md", summary, #{
            title: "Summary for " + today,
            tags: ["summary", "daily"]
        })?;
    } else {
        println(summary);
    }

    Ok(())
}
```

## See Also

- [[Help/Rune/Language Basics]] - Rune syntax
- [[Help/Rune/Crucible API]] - Available functions
- [[Help/Rune/Error Handling]] - Error handling patterns
- [[Help/Rune/Testing Plugins]] - Debugging and testing
- [[Help/Extending/Creating Plugins]] - Plugin overview
- [[Scripts/Auto Tagging]] - Example plugin
