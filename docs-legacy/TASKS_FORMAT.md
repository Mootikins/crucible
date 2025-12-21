# TASKS.md Format Specification

This document describes the format for `TASKS.md` files used with the `cru tasks` CLI commands.

## Overview

TASKS.md files are structured markdown documents that define implementation plans with:
- **Frontmatter** for project metadata
- **Phases** to organize work into logical stages
- **Tasks** with checkboxes, IDs, and dependencies
- **Inline metadata** for machine parsing

## File Structure

```markdown
---
title: Project Title
description: Brief description of the project
context_files:
  - path/to/relevant/file.rs
  - path/to/another/file.rs
verify: cargo test --workspace
tdd: true
---

## Phase 1: Phase Name

### 1.1 Section Name

- [ ] Task description [id:: 1.1.1]
  - Implementation details
  - [tests:: test_name_1, test_name_2]

- [ ] Another task [id:: 1.1.2] [deps:: 1.1.1]
  - This task depends on 1.1.1
```

## Frontmatter Fields

| Field | Required | Description |
|-------|----------|-------------|
| `title` | Yes | Human-readable project name |
| `description` | Yes | Brief description of what this task list accomplishes |
| `context_files` | No | List of files relevant to this work (for agent context) |
| `verify` | No | Command to run to verify completion (e.g., `just test`) |
| `tdd` | No | Whether to follow TDD (test-driven development) workflow |

## Checkbox Symbols

Standard markdown checkboxes with extended statuses:

| Symbol | Status | Description |
|--------|--------|-------------|
| `[ ]` | pending | Not started |
| `[x]` | completed | Finished successfully |
| `[/]` | in_progress | Currently being worked on |
| `[-]` | blocked | Cannot proceed, needs intervention |
| `[!]` | urgent | High priority, needs immediate attention |
| `[?]` | question | Needs clarification before starting |
| `[w]` | waiting | Waiting on external dependency |

## Inline Metadata

Metadata is embedded in task lines using `[key:: value]` syntax:

| Key | Description | Example |
|-----|-------------|---------|
| `id` | Unique task identifier | `[id:: 1.1.1]` |
| `deps` | Comma-separated dependency IDs | `[deps:: 1.1.1, 1.2.1]` |
| `tests` | Test names to verify this task | `[tests:: test_foo, test_bar]` |
| `priority` | Task priority (low/medium/high) | `[priority:: high]` |

### ID Format

Task IDs follow the pattern: `{phase}.{section}.{task}`

- Phase 1, Section 1, Task 1: `1.1.1`
- Phase 2, Section 3, Task 2: `2.3.2`

## CLI Commands

The `cru tasks` command provides operations on TASKS.md files:

```bash
# List all tasks with status
cru tasks list [path]

# Show next available task (respecting dependencies)
cru tasks next [path]

# Mark a task as in-progress
cru tasks pick <task_id> [path]

# Mark a task as completed
cru tasks done <task_id> [path]

# Mark a task as blocked
cru tasks blocked <task_id> [path]
```

## Dependency Resolution

Tasks with `[deps:: ...]` metadata won't be available until all dependencies are completed:

```markdown
- [x] Create database schema [id:: 1.1.1]
- [ ] Implement CRUD operations [id:: 1.1.2] [deps:: 1.1.1]  # Available
- [ ] Add caching layer [id:: 1.1.3] [deps:: 1.1.2]          # Not available yet
```

## Best Practices

1. **Use descriptive IDs**: IDs should reflect the phase/section structure
2. **Keep tasks small**: Each task should be completable in one focused session
3. **Specify dependencies explicitly**: Don't assume order implies dependency
4. **Include verification steps**: Add `[tests::]` for testable tasks
5. **Group related work**: Use phases and sections to organize logically

## Example

See `examples/test-kiln/Tasks/example-project.md` for a complete example.
