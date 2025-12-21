---
description: How to create custom workflows in Crucible
status: implemented
tags:
  - extending
  - workflows
aliases:
  - Creating Workflows
  - Workflow Development
---

# Workflow Authoring

Create automated workflows that combine multiple operations.

> [!warning] A new prose-based workflow syntax is planned. See [[Help/Workflows/Markup]].

## Overview

Workflows are sequences of steps that:
- Process notes automatically
- Chain agent operations
- React to file changes
- Schedule recurring tasks

## Workflow Definition (YAML)

```yaml
# workflows/daily-review.yaml
name: Daily Review
description: Generate daily summary of changes
trigger:
  schedule: "0 18 * * *"  # 6 PM daily

steps:
  - name: Find today's notes
    tool: search
    params:
      query: "modified:today"

  - name: Summarize
    agent: Researcher
    prompt: "Summarize these notes: {{previous.results}}"

  - name: Create summary
    tool: create_note
    params:
      title: "Daily Summary - {{date}}"
      content: "{{previous.response}}"
```

## Triggers

| Type | Description |
|------|-------------|
| `schedule` | Cron expression |
| `file_change` | On note modification |
| `manual` | Explicit invocation |
| `webhook` | HTTP trigger |

## Steps

Each step can:
- Call a **tool** with parameters
- Invoke an **agent** with a prompt
- Reference **previous step results** with `{{previous.*}}`

## Variables

Use template variables in steps:

| Variable | Description |
|----------|-------------|
| `{{date}}` | Current date |
| `{{time}}` | Current time |
| `{{previous.results}}` | Output from previous step |
| `{{previous.response}}` | Agent response from previous step |

## Running Workflows

```bash
# Run a workflow manually
cru workflow run "daily-review"

# List available workflows
cru workflow list
```

## See Also

- [[Help/Workflows/Index]] - Workflow system overview
- [[Help/Workflows/Markup]] - Planned prose syntax
- [[Help/Workflows/Sessions]] - Session tracking
- [[Help/Extending/Creating Plugins]] - Plugin development
- [[Help/Extending/Custom Tools]] - Creating tools
- [[Scripts/Daily Summary]] - Example workflow script
