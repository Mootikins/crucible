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

## Execution

A minimal slice of the execution runtime is now wired up. It walks the
parsed workflow, enforces gates (including step-level and preamble
gates), and maintains a per-session output scope. Inline (`default`)
steps currently produce a placeholder output — real LLM invocation
lands in the next slice. `fan` and `ralph` step types are not yet
implemented.

```bash
cru workflow start deploy-feature                 # begin execution
cru workflow status <session>                     # current step / pending gate
cru workflow approve <session> [<gate-id>]        # resolve a gate
cru workflow cancel <session>                     # stop mid-run
```

`start` creates a new workflow session against the active kiln and
drives the engine to the first gate (or to completion if there are
none). Progress arrives on the existing session event stream as
`workflow.step_started`, `workflow.gate_reached`,
`workflow.step_completed`, `workflow.completed`, etc. — subscribe
with any existing session client.

**Dispatch model (stdlib):**

- default (no annotation) — inline (placeholder; LLM turn coming next)
- `[type:: gate]` — pause for human approval
- unknown types — fall back to the default handler until a custom
  executor is registered (ultimately via Lua; see Phase 3b in the
  plan)
- `[type:: fan]` / `[type:: ralph]` — **not yet implemented**; treated
  as default for now

See the plan at `thoughts/shared/plans/workflows_2026-04-22-2030.md`
for the complete execution design.

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
