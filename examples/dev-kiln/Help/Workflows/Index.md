---
title: Workflows
description: Automate multi-step processes in your kiln
status: planned
tags:
  - workflows
  - automation
---

# Workflows

> [!warning] Workflows are not yet fully implemented.

Workflows let you define multi-step processes that can be executed automatically or triggered by events.

## What Workflows Do

A workflow is a series of steps that accomplish a goal:

1. **Trigger** - What starts the workflow (manual, schedule, event)
2. **Steps** - Actions to perform in order
3. **Session** - Track progress and resume if interrupted

## Planned Features

### Workflow Markup

Define workflows in natural prose format:

```markdown
# Weekly Review Workflow

Start by gathering all notes modified this week.
Then categorize them by project using [[tags]].
Finally, create a summary note in Summaries/.
```

See [[Help/Workflows/Markup]] for syntax details.

### Workflow Sessions

Track workflow progress across sessions:

```bash
# Start a workflow
cru workflow start "weekly-review"

# Check progress
cru workflow status

# Resume if interrupted
cru workflow resume
```

See [[Help/Workflows/Sessions]] for session management.

## Example Workflows

### Weekly Review
1. Find notes modified in last 7 days
2. Check for incomplete tasks
3. Generate summary report

### Daily Capture
1. Create today's daily note
2. Link to yesterday's note
3. Add template sections

### Project Archive
1. Find all notes in project folder
2. Update status to "archived"
3. Move to Archive/

## See Also

- [[Extending Crucible]] - All extension points
- [[Help/Extending/Event Hooks]] - Triggering workflows from events
- [[AI Features]] - AI-assisted workflow execution
