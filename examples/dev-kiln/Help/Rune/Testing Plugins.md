---
description: How to test and debug Rune plugins locally
status: implemented
tags:
  - rune
  - testing
  - guide
aliases:
  - Debugging Plugins
  - Plugin Testing
---

# Testing Plugins

Testing Rune plugins locally ensures your scripts work correctly before deploying them as hooks or tools.

## Running Scripts Directly

The simplest way to test a plugin:

```bash
cru script "Scripts/my-plugin.rn"
```

By default, scripts run in **dry-run mode** to prevent accidental modifications. To enable writes:

```bash
cru script "Scripts/my-plugin.rn" --act
```

## Dry Run Mode

Crucible's dry-run mode prevents scripts from modifying your vault during development:

- **Default behavior**: All write operations are blocked
- **Enable writes**: Use the `--act` flag
- **Check mode in scripts**: Use `crucible::is_act_mode()`

```rune
pub fn main() {
    if crucible::is_act_mode() {
        crucible::create_note("New Note", "Content");
    } else {
        println("DRY RUN: Would create note");
    }
}
```

## Testing Hook Execution

Hooks run automatically when events fire. To test:

1. Register the hook in `crucible.toml`
2. Trigger the event (e.g., make a tool call)
3. Check output or vault changes

Use `println()` statements to debug execution flow.

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Script not found | Wrong path | Use relative path from kiln root |
| Permission denied | Not in act mode | Add --act flag |
| Function not found | Missing pub | Mark entry point as `pub fn` |
| Type error | Wrong arg type | Check schema definition |

## Debugging Tips

**Use print statements liberally:**

```rune
pub fn main() {
    println("Starting plugin");
    let result = some_operation();
    println("Result: {}", result);
}
```

**Check return values with match:**

```rune
match crucible::get_note("Note Name") {
    Ok(note) => println("Found: {}", note.title()),
    Err(e) => println("Error: {}", e),
}
```

**Test incrementally:**
- Start with simple inputs
- Verify basic operations work
- Add complexity gradually

**Path verification:**
- All paths are relative to the kiln root
- Use `crucible::get_kiln_path()` to check the base directory

## See Also

- [[Help/Extending/Creating Plugins]]
- [[Help/Rune/Error Handling]]
- [[Help/Rune/Best Practices]]
