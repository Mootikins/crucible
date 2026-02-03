# Todo List Plugin

A task management plugin for Crucible using the TASKS.md format.

## Features

- **tasks_list** - List all tasks with their completion status
- **tasks_add** - Add a new task
- **tasks_complete** - Mark a task as completed
- **tasks_next** - Get the next uncompleted task
- **/tasks** - Command for quick task management

## Installation

Copy this plugin to your Crucible plugins directory:

```bash
cp -r todo-list ~/.config/crucible/plugins/
# or
cp -r todo-list ~/your-kiln/plugins/
```

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
