---
description: Track implementation tasks using structured markdown files
status: planned
tags:
  - reference
  - tasks
  - workflows
  - plugins
aliases:
  - TASKS.md
  - Task Files
---

# Task Management

> **Status**: This feature is planned as an official Rune plugin. It will demonstrate programmatic tool generation, file-as-state patterns, and the tools→workflow bridge.

Task management in Crucible uses structured markdown files (typically `TASKS.md`) to track implementation plans. The file format is designed for both human readability and machine parsing.

## Overview

TASKS.md files combine:
- **Frontmatter** for project metadata
- **Phases** to organize work into logical stages
- **Tasks** with checkboxes, IDs, and dependencies
- **Inline metadata** for machine parsing

## File Format

```markdown
---
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

Metadata is embedded in task lines using `[key:: value]` syntax (Dataview-compatible):

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

## Planned CLI Commands

The `cru tasks` command will provide operations on TASKS.md files:

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

## Plugin Architecture

This feature will be implemented as an official Rune plugin, demonstrating:

1. **Programmatic tool generation**: Tools are generated at initialization based on the TASKS.md format
2. **File-as-state**: No runtime state—the markdown file is the source of truth
3. **Tools→workflow bridge**: Individual task tools compose into a workflow

### Planned Plugin Structure

```
Scripts/
└── tasks/
    ├── plugin.rn          # Main plugin with tool generators
    ├── parser.rn          # TASKS.md format parser
    └── README.md          # Usage documentation
```

### Tool Generation Pattern

```rune
// At initialization, generate tools from task format
pub fn on_init(ctx) {
    // Register task management tools
    ctx.register_tool("tasks_list", tasks_list_handler);
    ctx.register_tool("tasks_next", tasks_next_handler);
    ctx.register_tool("tasks_pick", tasks_pick_handler);
    ctx.register_tool("tasks_done", tasks_done_handler);
}
```

## Best Practices

1. **Use descriptive IDs**: IDs should reflect the phase/section structure
2. **Keep tasks small**: Each task should be completable in one focused session
3. **Specify dependencies explicitly**: Don't assume order implies dependency
4. **Include verification steps**: Add `[tests::]` for testable tasks
5. **Group related work**: Use phases and sections to organize logically

## See Also

- [[Help/Extending/Creating Plugins]] - Plugin development guide
- [[Help/Rune/Tool Definition]] - Defining custom tools
- [[Help/Workflows/Index]] - Workflow system overview
- [[Meta/Plugin User Stories]] - Plugin system user stories
