# Chat Diff Display Design

## Overview

Display file edit diffs in the chat CLI for better transparency when agents modify files.

**Use cases:**
1. **Post-execution display** - Show diff after edit tool completes (inline in streaming)
2. **Pre-approval preview** - Show diff before user approves in `act` mode

## Architecture

### New Module

```
crates/crucible-cli/src/chat/
├── diff.rs          # NEW - Diff computation and rendering
├── display.rs       # MODIFIED - Integration with edit display
└── ...
```

### Core Type

```rust
// crates/crucible-cli/src/chat/diff.rs

pub struct DiffRenderer {
    context_lines: usize,  // Default: 0, configurable
}

impl DiffRenderer {
    pub fn new() -> Self;
    pub fn with_context(self, lines: usize) -> Self;

    /// Render and print diff as preview (pre-approval in act mode)
    pub fn print_preview(&self, path: &str, old: &str, new: &str);

    /// Render and print diff as result (post-execution)
    pub fn print_result(&self, path: &str, old: &str, new: &str);
}
```

### Dependencies

- `similar` - Diff computation (Myers algorithm, unified diff support)
- `colored` (existing) - Terminal colors

## Output Format

```
  ▷ Edit file(path="/src/main.rs")
    @@ -5,2 +5,3 @@
    - println!("Hello");
    + println!("Hello, world!");
    + println!("Welcome!");
```

- Indented under tool call indicator (`▷`)
- Hunk header (`@@`) in dimmed/cyan
- Deletions in red with `-` prefix
- Insertions in green with `+` prefix
- Default: 0 context lines (compact), configurable

## Integration

### Caller Responsibilities

- Read file content before edit (for `old` content)
- Capture new content from tool result
- Call appropriate `DiffRenderer` method based on mode

### Flow

**Post-execution (all modes):**
```rust
// After edit tool completes
let renderer = DiffRenderer::new();
renderer.print_result(&path, &old_content, &new_content);
```

**Pre-approval (act mode):**
```rust
// Before showing approve/reject prompt
let renderer = DiffRenderer::new();
renderer.print_preview(&path, &old_content, &new_content);
// Then show approval prompt
```

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Diff library | `similar` | Well-tested, no runtime deps, unified diff support |
| Syntax highlighting | Deferred | Start simple with colored +/- lines, add `syntect` later |
| Context lines | 0 default | Most compact, configurable for those who want more |
| Scope context | Deferred | Would require tree-sitter, skip unless added for other reasons |
| Terminal abstraction | None for v1 | KISS - direct prints, swap wholesale when moving to ratatui |

## Future Enhancements

1. **Syntax highlighting** - Add `syntect` for language-aware coloring
2. **Tree-sitter scope context** - Show surrounding function/block headers (like nvim-treesitter-context)
3. **Ratatui migration** - Build as widget for proper screen region management
4. **Concurrent typing** - Update ticker while user types (requires cursor position management)

## Non-Goals (v1)

- Side-by-side diff display
- Inline word-diff
- Streaming diff (diff shown after tool completes, not during)
