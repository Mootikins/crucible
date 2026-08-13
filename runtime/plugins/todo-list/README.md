---
title: todo list
description: Todo List Plugin
tags:
  - plugins
---

# Todo List Plugin

A task management plugin for Crucible using the TASKS.md format.

## Features

- **tasks_list** - List all tasks with their completion status
- **tasks_add** - Add a new task
- **tasks_complete** - Mark a task as completed
- **tasks_next** - Get the next uncompleted task
- **/tasks** - Command for quick task management

## Installation

None — `todo-list` ships with Crucible and is enabled by default:

```toml
# ~/.config/crucible/config.toml
[plugins.todo-list]
enabled = false            # the kill switch
default_file = "TASKS.md"  # relative to the KILN root; an absolute path is used as given
show_completed = false
```

Config is the durable lever: editing the extracted `plugin.yaml` does not
survive, because the runtime tree is re-stamped from the binary whenever the
build changes.

Task **ids are positions in the file**, not positions in a listing. An id from
`tasks_list { show_completed = false }` still means the same task.

## Usage

### Tools (for agents)

```lua
-- List all tasks
tasks_list({ file = "TASKS.md", show_completed = true })

-- Add a task
tasks_add({ text = "Review pull request" })

-- Complete a task by ID
tasks_complete({ id = 1 })

-- Get next task to work on
tasks_next({})
```

### Command (for users)

```
/tasks              # Show all tasks
/tasks list         # Same as above
/tasks add Buy milk # Add a new task
/tasks next         # Show next uncompleted task
```

## TASKS.md Format

```markdown
# Tasks

- [ ] Uncompleted task
- [x] Completed task
- [ ] Another task to do
```

## Configuration

In your `plugin.yaml`:

```yaml
config:
  properties:
    default_file:
      type: string
      default: "TASKS.md"
    show_completed:
      type: boolean
      default: false
```

## Capabilities Required

- `filesystem` - Read and write TASKS.md files
- `kiln` - Access to kiln for task storage
