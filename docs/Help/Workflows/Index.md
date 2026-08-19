---
title: Workflows
description: Define multi-step planning and orchestration in markdown
tags:
  - workflows
  - automation
---

# Workflows

Workflows let you define multi-step processes — the *what* and *why* of a task — in plain markdown. The system parses workflow notes into a typed AST that can be inspected and executed via the daemon's workflow engine (see [Execution](#execution) below for what is implemented).

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

The runtime walks the parsed workflow, enforces gates (preamble and
step-level), and maintains a per-session output scope. Inline
(`default`) steps drive one turn of the session's configured agent
with the step body (after scope interpolation) as the prompt;
assistant response text is captured as the step's named output when
`-> name` is present. `fan` and `ralph` step types are not yet
implemented — steps annotated with those types fall back to the
default handler.

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

**Output interpolation:** `**name**` tokens in a step body are
replaced with the value of the matching key in the output scope
before the prompt is sent. String values inline verbatim; other JSON
values serialise as pretty JSON. Bold text whose content doesn't
match a scope key passes through unchanged.

**Completion assessment:** when the run reaches `Completed`, the
daemon executes each runnable entry from the workflow's
`## Validation` section (list items with a single backticked command)
and emits a `workflow.assessed` event summarising passes, failures,
and manual (command-less) entries.

Each command first goes through the operator's `[permissions]` rules,
as `bash`, exactly as a plugin's `cru.tools.call` does. The command
text comes out of a note, so whoever can write a `type: workflow`
note into the kiln chooses it — an agent with `create_note` included.
The gate is fail-closed and has no prompt to fall back on: a `deny`
refuses, an `allow` runs, and an `ask` rule refuses because there is
no user attached to a completed run. **The shipped default is
`default = "ask"`, so an unconfigured daemon runs no validation
command at all.** To let a command run, name it in `allow`:

```toml
[permissions]
allow = ["bash:cargo test *", "bash:just ci"]
```

A refused entry is reported as a failure in `workflow.assessed`, with
the reason in its `stderr`, so it is visible rather than silent. See
[[Help/Concepts/Permission Precedence]].

**Resumability:** the daemon persists a compact workflow snapshot
next to the session metadata after each state change (new gate,
approval, cancel). If the daemon restarts mid-run, the next RPC
against the session (`workflow.status`, `workflow.approve_gate`,
etc.) transparently rehydrates the paused execution. A crash
*during* an inline turn loses that turn — the workflow picks back up
at the step that was running.

Set `CRUCIBLE_WORKFLOW_DRY_RUN=1` in the daemon environment to swap
the real inline handler for a placeholder that produces synthetic
output without calling an LLM. Handy for CI and demos.

**Dispatch model (stdlib):**

- default (no annotation) — inline: one agent turn per step
- `[type:: gate]` — pause for human approval
- unknown types — fall back to the default handler until a custom
  executor is registered (ultimately via Lua)
- `[type:: fan]` / `[type:: ralph]` — **not yet implemented**; treated
  as default for now

**Agent hints (`@agent`)** on a step heading are parsed and visible in
`cru workflow show`, but cross-agent dispatch is deferred until
`[type:: fan]` lands — every step currently runs on the session's
configured agent regardless of the `@agent` suffix. The daemon logs a
warning when it sees a mismatched hint so you aren't surprised.

## Roadmap

Planned engine work, in rough order (tracked in the product map, `Meta/Product.md`, not bundled with these docs):

- **`fan`** — dispatch each child of a fan step as a delegated child session, reusing the existing delegation limits (depth, trust, concurrency, result size). This is what makes parallel groups run LLM turns concurrently and makes `@agent` hints route.
- **`ralph`** — repeat a step until the workflow's runnable `## Validation` entries pass, with a bounded attempt count.
- **Typed outputs** — an optional schema on `-> name` outputs, validated before binding into scope, and a warning for `**bold**` tokens that match no output name (today they pass through silently).
- **`verify`** — an adversarial-check step: independent child turns try to refute the previous step's output; majority refutation fails the step.
- **Run budgets** — a token ceiling per run that pauses at a gate instead of failing when reached.

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
