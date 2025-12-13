---
title: Workflow Authoring
description: How to create custom workflows in Crucible
tags:
  - help
  - extending
  - workflows
---

# Workflow Authoring

Create automated workflows that combine multiple operations.

## Overview

Workflows are sequences of steps that:
- Process notes automatically
- Chain agent operations
- React to file changes
- Schedule recurring tasks

## Workflow Definition

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

## See Also

- [[Help/Extending/Writing Plugins]] - Plugin development
- [[Help/Extending/Custom Tools]] - Creating tools
- [[Scripts/Daily Summary]] - Example workflow script
