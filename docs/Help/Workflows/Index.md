---
title: Workflows
description: Define multi-step planning and orchestration in markdown
tags:
  - workflows
  - automation
---

# Workflows

Workflows let you define multi-step processes — the *what* and *why* of a task — in plain markdown. The system parses workflow notes into a typed AST that can be inspected today and (in a future phase) executed end-to-end.

## What Workflows Do

A workflow note describes:

1. **Goals** — outcomes to aim for (`## Goals` task list)
2. **Validation** — success criteria, including runnable commands (`## Validation`)
3. **Steps** — a tree of headings with optional `@agent`, `-> output`, and `[type:: ...]` attributes
4. **Gates** — `> [!gate]` callouts for human approval checkpoints

## Authoring (available today)

Write a workflow as a markdown note with `type: workflow` in the frontmatter:

```markdown
---
type: workflow
title: Deploy New Feature
---

## Goals

- [ ] Users can export data in CSV format
- [ ] Large exports don't block the UI

## Validation

- `cargo test` passes
- Manual: happy-path export completes in under 2s

## Plan -> plan

Analyze requirements and identify affected components.

## Implement @developer

Use **plan** to drive changes.

## Review and Deploy [type:: fan]

> [!gate]
> Requires ops sign-off before production

### Code Review @reviewer
### Deploy to Staging
### Deploy to Production
```

Inspect parsed workflows with the CLI:

```bash
# List all workflow notes in the active kiln
cru workflow list

# Show a workflow's parsed structure
cru workflow show "Deploy New Feature"
cru workflow show deploy                 # by filename stem
cru workflow show -f json deploy         # JSON for scripting
```

See [[Help/Workflows/Workflow Syntax]] for the full syntax reference.

## Execution (planned)

> **⚠️ Not yet implemented.** The runtime that actually *runs* a workflow — spawning sub-sessions, enforcing gates, joining parallel fans, looping ralph steps — is a later phase.

Planned commands (design-only):

```bash
cru workflow start "deploy-feature"    # start an execution session
cru workflow status                     # inspect in-flight workflow
cru workflow resume                     # resume after interruption
cru workflow approve <session> <gate>   # resolve a gate
```

The execution model dispatches per step:

- default (no annotation) — inline turn in the workflow session
- `[type:: gate]` — pause for human approval
- `[type:: fan]` — parallel sub-sessions, one per child
- `[type:: ralph]` — loop until validation criteria pass
- unknown types — dispatched to Lua executors registered via `crucible.workflow.register`

See the plan at `thoughts/shared/plans/workflows_2026-04-22-2030.md` for the execution design.

## Example Use Cases

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

- [[Help/Workflows/Workflow Syntax]] — full syntax reference
- [[Help/Extending/Workflow Authoring]] — authoring workflows
- [[Extending Crucible]] — all extension points
- [[Help/Extending/Event Hooks]] — triggering workflows from events
