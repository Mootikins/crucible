---
description: Track and resume workflow progress
status: planned
tags:
  - workflows
  - sessions
---

# Workflow Sessions

> [!warning] Workflow sessions are not yet implemented.

Sessions track workflow execution so you can pause, resume, and audit multi-step processes.

## What Sessions Track

A session records:

- **Start time** - When the workflow began
- **Current step** - Where execution is now
- **Completed steps** - What has been done
- **Context** - Data passed between steps
- **Errors** - Problems encountered

## Starting a Session

```bash
# Start a named workflow
cru workflow start "weekly-review"

# Start with parameters
cru workflow start "inbox-process" --folder Inbox
```

## Checking Status

```bash
# See current progress
cru workflow status

# Output:
# Workflow: weekly-review
# Status: in_progress
# Step: 3/5 "Generate summary"
# Started: 2024-01-15 10:30:00
# Elapsed: 5 minutes
```

## Resuming a Session

If a workflow is interrupted (computer sleep, error, etc.):

```bash
# Resume where you left off
cru workflow resume

# Resume a specific session
cru workflow resume --session abc123
```

## Session History

View past sessions:

```bash
# List recent sessions
cru workflow history

# Output:
# ID        Workflow        Status      Date
# abc123    weekly-review   completed   2024-01-15
# def456    inbox-process   failed      2024-01-14
# ghi789    daily-capture   completed   2024-01-14
```

## Session Details

Inspect a specific session:

```bash
cru workflow show abc123

# Output:
# Session: abc123
# Workflow: weekly-review
# Status: completed
#
# Steps:
# 1. [✓] Find modified notes (found 15)
# 2. [✓] Group by project (3 projects)
# 3. [✓] Generate summary
# 4. [✓] Create review note
# 5. [✓] Update note status
#
# Created: Reviews/2024-01-15.md
```

## Error Recovery

When a step fails:

```bash
# See what went wrong
cru workflow status
# Step: 3/5 "Generate summary" - FAILED
# Error: Could not access note: Projects/archived.md

# Fix the issue, then resume
cru workflow resume

# Or skip the failed step
cru workflow resume --skip-failed
```

## Session Storage

Sessions are stored in your kiln's data directory:

```
.crucible/
├── sessions/
│   ├── abc123.json
│   ├── def456.json
│   └── ...
```

## Cleanup

Remove old sessions:

```bash
# Remove completed sessions older than 30 days
cru workflow cleanup --older-than 30d

# Remove all completed sessions
cru workflow cleanup --completed
```

## See Also

- [[Help/Workflows/Index]] - Workflow overview
- [[Help/Workflows/Markup]] - Defining workflows
- [[Help/CLI/chat]] - Interactive workflow execution
