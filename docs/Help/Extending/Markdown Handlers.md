---
title: Markdown Handlers
description: Define event handlers in pure markdown that inject context into agents
tags:
  - handlers
  - events
  - markdown
  - syntax
---

# Markdown Handlers

Define event handlers using pure markdown — no code required. Handler content flows into agent context when events fire, influencing agent behavior through contextual instructions.

## Overview

Handlers let you write guidance that activates on specific events. Instead of writing code, you write the content that should enter the agent's context.

```markdown
---
type: handlers
---

## [Event:: tool:before]
### edit_file

When editing files:
- Preserve existing code style
- Add tests for changed functions
- Update related documentation
```

When `edit_file` is about to execute, this guidance joins the agent's working context.

## Syntax

### Document Type

Handler documents declare their type in frontmatter:

```yaml
---
type: handlers
---
```

### Event Headings

Mark a heading as an event handler with `[Event:: <event-type>]`:

```markdown
## [Event:: tool:before]
```

Case-insensitive — `[event:: tool:before]` and `[EVENT:: TOOL:BEFORE]` are equivalent. UX may normalize display.

### Pattern Narrowing

Child headings narrow the event pattern using glob-style matching:

```markdown
## [Event:: tool:before]
### edit_file

Content for edit_file...

### create_file

Content for create_file...

### *.rs

Content for any Rust file operation...
```

Patterns can also be inline with slashes:

```markdown
## [Event:: tool:before/edit_file/*.rs]

Rust-specific file editing guidance...
```

Both forms are equivalent — use nesting for organization, inline for simple cases.

### Content Scope

Everything under the deepest matching heading (excluding child headings) is the handler content:

```markdown
## [Event:: tool:before]

This paragraph is skipped (explanation only).

### edit_file

This content IS injected when edit_file fires.

More paragraphs here also injected.

#### Subsection

This is part of edit_file content too.

### create_file

Different content for create_file.
```

Paragraphs between the event heading and first pattern heading are explanations — not injected.

## Event Types

### Tool Events

| Event | Fires | Use Case |
|-------|-------|----------|
| `tool:before` | Before tool executes | Inject guidance, constraints |
| `tool:after` | After tool completes | Post-processing hints |
| `tool:error` | When tool fails | Error handling guidance |

### Note Events

| Event | Fires | Use Case |
|-------|-------|----------|
| `note:parsed` | After note is parsed | Content-aware processing |
| `note:created` | When note is created | Template injection |
| `note:modified` | When note changes | Change-aware guidance |

### Agent Events

| Event | Fires | Use Case |
|-------|-------|----------|
| `agent:before_llm` | Before LLM call | System prompt additions |

## Pattern Matching

Patterns use glob-style matching:

| Pattern | Matches |
|---------|---------|
| `edit_file` | Exact match |
| `edit_*` | Starts with `edit_` |
| `*_file` | Ends with `_file` |
| `*.rs` | Any `.rs` file |
| `**/*.rs` | Rust files in any subdirectory |
| `*` | Everything |

## Examples

### Project-Specific Style Guide

```markdown
---
type: handlers
---

## [Event:: tool:before]
### edit_file
### create_file

Follow the project style guide:
- 2-space indentation
- Single quotes for strings
- Trailing commas in multi-line structures

### edit_file/*.test.*

For test files:
- Use descriptive test names
- One assertion per test when possible
- Mock external dependencies
```

### Safety Constraints

```markdown
---
type: handlers
---

## [Event:: tool:before]
### *delete*

Before deleting anything:
- Confirm the file is not in use
- Check for dependent files
- Consider moving to archive instead

### bash

Shell commands must:
- Avoid `rm -rf` without confirmation
- Use explicit paths, not globs
- Check exit codes
```

### Language-Specific Guidance

```markdown
---
type: handlers
---

## [Event:: tool:before/edit_file]

### *.rs

Rust conventions:
- Run `cargo fmt` style
- Add doc comments to public items
- Use `Result` over panics

### *.py

Python conventions:
- Follow PEP 8
- Add type hints
- Use f-strings over format()

### *.ts

TypeScript conventions:
- Strict mode always
- Explicit return types on exports
- Prefer interfaces over type aliases
```

## How It Works

1. Event fires (e.g., `tool:before` for `edit_file`)
2. System finds handler documents (`type: handlers`)
3. Matches event type and pattern against handler headings
4. Collects content from all matching handlers
5. Injects into agent context before processing

Multiple handlers can match — all matching content is combined.

## Editor Integration

Editors can enhance handler documents with:

- **Virtual text**: Show resolved discriminant (`tool:before/edit_file/*.rs`)
- **Folding**: Collapse `[Event::]` headings to event type
- **Highlighting**: Distinguish patterns from content
- **Validation**: Warn on invalid event types or patterns

## Related

- [[Event Hooks]] — Code-based event handlers in Lua
- [[Creating Plugins]] — Full plugin development
- [[Workflows]] — Higher-level orchestration
