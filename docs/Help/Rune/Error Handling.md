---
description: Error handling patterns and the fail-open execution model
status: implemented
tags:
  - rune
  - errors
  - reference
aliases:
  - Rune Errors
  - Exception Handling
---

# Error Handling

Rune provides multiple patterns for handling errors in scripts, from simple propagation to explicit error handling and fail-open semantics for event hooks.

## The ? Operator

The `?` operator propagates errors up the call stack:

```rune
pub fn process_note(path) {
    let note = crucible::read_note(path)?;  // Propagates error
    let blocks = note.blocks()?;
    println("Processed {} blocks", blocks.len());
}
```

If `read_note()` returns an error, execution stops and the error returns to the caller. This is the most concise pattern for internal functions.

## Match Expressions

For explicit error handling, use `match`:

```rune
pub fn safe_process(path) {
    match crucible::read_note(path) {
        Ok(note) => {
            println("Successfully read: {}", note.title());
            process(note);
        },
        Err(e) => {
            println("Failed to read note: {}", e);
        }
    }
}
```

## Fail-Open Semantics for Hooks

Event hooks use a **fail-open execution model**:

- **Non-fatal errors** are logged but don't stop the event pipeline
- **Fatal errors** halt the handler chain
- If a handler fails, the original event is returned unchanged

Your hooks won't break the system if they encounter errors.

## HandlerError Types

For fine-grained control in hooks:

### Non-Fatal Errors

Logged but allow the pipeline to continue:

```rune
pub fn on_note_parsed(event) {
    if !event.note.has_frontmatter() {
        return Err(HandlerError::non_fatal(
            "metadata_extractor",
            "Note missing frontmatter, skipping"
        ));
    }
    event
}
```

### Fatal Errors

Stop the entire handler chain:

```rune
pub fn on_note_parsed(event) {
    if crucible::is_corrupted(event.note) {
        return Err(HandlerError::fatal(
            "corruption_detector",
            "Note data corrupted, halting pipeline"
        ));
    }
    event
}
```

## Best Practices

**1. Handle Errors in Main Functions**

```rune
pub fn main() {
    match run() {
        Ok(result) => println("Success: {}", result),
        Err(e) => println("Error: {}", e),
    }
}
```

**2. Use ? for Internal Functions**

```rune
fn internal_logic() {
    let data = fetch_data()?;
    process(data)?;
}
```

**3. Check Mode Before Writes**

```rune
if crucible::is_act_mode() {
    crucible::write_note(note)?;
} else {
    println("DRY RUN: Would write {}", note.path());
}
```

**4. Provide User Feedback**

```rune
Err(e) => println("Failed to parse {}: {}", path, e)
```

## See Also

- [[Help/Rune/Best Practices]] - Writing robust Rune scripts
- [[Help/Extending/Event Hooks]] - Hook lifecycle and execution model
- [[Help/Rune/Language Basics]] - Rune syntax fundamentals
