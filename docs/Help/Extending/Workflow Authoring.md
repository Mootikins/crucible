---
title: Workflow Authoring
description: How to create custom workflows in Crucible
status: planned
tags:
  - extending
  - workflows
aliases:
  - Creating Workflows
  - Workflow Development
---

# Workflow Authoring

> **⚠️ Design document, not the shipped feature.** Nothing on this page is
> implemented, and the YAML format below **conflicts with the workflow system
> Crucible actually ships**: real workflows are *markdown notes* in your kiln,
> parsed into goals, validation, and a step tree — see
> [[Help/Workflows/Index]] and [[Help/Workflows/Workflow Syntax]]. There is no
> YAML workflow engine, no `trigger:` support (no schedules, no webhooks), and
> no `{{...}}` templating. Author workflows in markdown; treat this page as an
> unadopted design sketch.

Create automated workflows that combine multiple operations.

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

The CLI that exists today runs *markdown* workflows (the subcommand is
`start`, not `run`):

```bash
# Start a workflow execution against a new session
cru workflow start "daily-review"

# List workflow notes in the active kiln
cru workflow list
```

## See Also

- [[Help/Workflows/Index]] - Workflow system overview
- [[Help/Workflows/Workflow Syntax]] - Markdown workflow syntax
- [[Help/Core/Sessions]] - Session tracking
- [[Help/Extending/Creating Plugins]] - Plugin development
- [[Help/Extending/Custom Tools]] - Creating tools
