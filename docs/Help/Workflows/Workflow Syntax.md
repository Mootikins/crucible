---
title: Workflow Syntax
description: Define planning and execution workflows in readable markdown
tags:
  - workflows
  - planning
  - orchestration
  - syntax
---

# Workflow Syntax

Workflows define high-level planning and orchestration in readable markdown. Unlike [[Task Management|task notes]] which focus on execution details, workflows describe *what* needs to happen and *why*, letting the system derive execution steps.

> **Status:** the syntax, parsing, and execution on this page are implemented — workflows parse into a typed `WorkflowDoc` AST (inspect with `cru workflow list` / `cru workflow show`) and run via the daemon's workflow engine, including parallel step groups. Sections still marked **(Future)** are not yet implemented; see [[Help/Workflows/Index]] for the roadmap.

## Overview

```markdown
---
type: workflow
title: Deploy New Feature
---

## Goals

- [ ] Users can export data in CSV format
- [ ] Export respects active filters
- [ ] Large exports don't block the UI

## Plan the Implementation

Analyze requirements and identify affected components.
Consider backwards compatibility with existing exports.

## Implement Changes @developer

Make code changes following existing patterns.
Run tests locally before proceeding.

## Review and Deploy

> [!gate]
> Requires sign-off before production deployment

### Code Review @reviewer

### Deploy to Staging

### Deploy to Production
```

## Document Structure

### Frontmatter

Workflows declare their type and optional metadata:

```yaml
---
type: workflow
title: Feature Name
description: Optional longer description
---
```

### Steps as Headings

Each heading defines a step. Heading order implies execution sequence:

```markdown
## First Step
## Second Step
## Third Step
```

Nesting creates sub-steps:

```markdown
## Deploy Phase
### Deploy to Staging
### Deploy to Production
```

### Content Types

**Bullet lists** = guidance (flows to agent context):
```markdown
## Implement Feature

- Follow existing patterns in the codebase
- Prefer composition over inheritance
- Keep functions small and focused
```

**Task lists** = actionable goals (tracked):
```markdown
## Goals

- [ ] Feature works offline
- [ ] Performance under 100ms
- [ ] Accessible to screen readers
```

**Prose paragraphs** = context and explanation:
```markdown
## Validate Schema

The configuration must match the expected schema before
we can proceed. Invalid configs should fail early with
clear error messages.
```

## Step Attributes

Steps can carry attributes using Dataview-style inline metadata on the heading line:

```markdown
## Build Artifacts [type:: fan] [timeout:: 5m]
```

Reserved attribute keys (interpreted by the execution runtime):

- `[type:: gate|fan|ralph|<custom>]` — step execution kind. Custom types dispatch to Lua executors registered via `crucible.workflow.register`.
- `[timeout:: <duration>]` — max wall-clock time (e.g. `30s`, `5m`, `1h`).
- `[on_error:: halt|skip|retry|continue]` — per-step failure policy.

Any `[k:: v]` pair is accepted on a heading; unrecognized keys are passed through to plugins.

## Agent Hints

Reference agents or roles with `@name`:

```markdown
## Design API @architect

## Implement Backend @backend-developer

## Write Tests @qa
```

The `@` hint is light — the system uses it to route or select appropriate agents, but the step can still execute without a specific agent.

## Gates

Use callouts to mark human approval checkpoints:

```markdown
## Deploy to Production

> [!gate]
> Requires ops lead approval before proceeding

Rolling deployment with monitoring...
```

Gates pause workflow execution until a human approves continuation.

Alternative gate styles:

```markdown
> [!gate] Security review required

> [!gate]
> - Legal approval
> - Compliance sign-off
```

## Data Flow

Use `->` to name outputs that flow between steps:

```markdown
## Parse Configuration -> config

Reads `config.yaml` and parses into structured data.

## Validate Schema -> validated_config

Validates **config** against the expected schema.
Fails fast with clear error messages.

## Generate Output

Uses **validated_config** to produce final artifacts.
```

**Conventions:**
- `-> name` in heading = this step produces named output
- **Bold names** in prose = references to outputs from other steps
- Output names are "soft files" — structured data, not necessarily filesystem files

## Goals Section

A `## Goals` heading has special semantics:

```markdown
## Goals

- [ ] Users can search by date range
- [ ] Search results paginate correctly
- [ ] Empty results show helpful message
```

Goals are:
- Tracked separately from execution tasks
- Used to validate workflow completion
- Visible in workflow status/progress views

Use task list syntax (`- [ ]`) for trackable goals, bullets for aspirational guidance.

## Validation Section

A `## Validation` heading, parallel to Goals, captures success/failure criteria — *how* we know the workflow succeeded, distinct from *what* we're building:

```markdown
## Validation

- `cargo test` passes
- `cargo clippy --all-targets` clean
- Manual: CSV download completes in under 2s for 10k rows
```

Items containing **exactly one** inline-code span are treated as runnable commands; everything else is a manual check. At execution time, validation entries prime agent context ("success looks like X, Y, Z") and serve as the default pass-criterion for `[type:: ralph]` steps.

Goals describe the *outcome*; validation describes the *acceptance criteria*. They are independent — a workflow can have either, both, or neither.

## Checkbox Semantics

Workflows use standard task-list checkboxes in the `## Goals` section:

| Syntax | Meaning |
|--------|---------|
| `- [ ]` | Pending |
| `- [x]` | Complete |

## Parallel Execution

Two equivalent markers mark steps for concurrent execution. Steps are headings, so both markers apply to heading lines.

**Heading suffix** — a trailing `(parallel)` (case-insensitive) on a section heading runs all of that section's direct child steps concurrently. The marker is stripped from the section title:

```markdown
## Build Artifacts (parallel)

### Build frontend

### Build backend
```

**Symbol prefix** — a leading `&` on a step heading. Consecutive `&`-prefixed steps run concurrently; the next unprefixed step waits for all of them:

```markdown
## &Build frontend
## &Build backend
## Run tests
```

The `&` is stripped from the step title and composes with the other heading markers (`## &Build @builder -> artifact`). A literal `&` elsewhere in a title (`## Fix A & B`) is not a marker.

**Semantics:**

- Each parallel step is a *branch*: the step plus its sub-steps, which run sequentially within the branch. Only sibling branches overlap.
- Branches see outputs (`-> name`) produced *before* the group started; sibling branches cannot see each other's outputs. When all branches finish, their outputs merge into the scope in document order.
- Step started/completed events are emitted per step and delivered in document order once the group joins.
- If any branch fails, the engine still waits for every branch, then fails the workflow with **all** branch failures reported. Successful branches keep their outputs.
- Gates (`> [!gate]` callouts or `[type:: gate]` steps) are not supported inside a parallel group — a gated branch fails with a clear reason. Place gates before or after the group instead.
- Steps that drive the session's own agent (the default step type) serialize their LLM turns — a session holds a single conversation. The group's join and failure semantics still apply; per-branch agent dispatch is `fan` territory (future).

## Branches (Future)

*Complex branching is deferred. Simple conditional flow may use:*

```markdown
## Validate Input

> [!branch] If validation fails
> Skip to Error Handling section

## Process Data
...

## Error Handling
```

## Workflow vs Tasks

| Aspect | Workflow | Tasks |
|--------|----------|-------|
| **Purpose** | Planning, orchestration | Execution details |
| **Audience** | Humans | Agents/system |
| **Syntax** | Minimal, prose-friendly | Can be metadata-heavy |
| **Granularity** | Phases, milestones | Individual actions |
| **Derivation** | Source of truth | Derived from workflow |

A workflow might have a step like:

```markdown
## Implement Authentication @backend
```

Which generates tasks like:

```
- [ ] Add auth middleware [id:: auth-1]
- [ ] Create login endpoint [id:: auth-2] [deps:: auth-1]
- [ ] Add session handling [id:: auth-3] [deps:: auth-1]
- [ ] Write auth tests [id:: auth-4] [deps:: auth-2, auth-3]
```

## Examples

### Feature Development

```markdown
---
type: workflow
title: Add Export Feature
---

## Goals

- [ ] Export to CSV format
- [ ] Export to JSON format
- [ ] Progress indicator for large exports

## Research

Review existing export patterns in the codebase.
Check for reusable utilities.

## Design @architect

- Define export interface
- Plan streaming for large datasets
- Consider cancellation support

## Implement @developer

### Core Export Logic -> exporter

### CSV Formatter

### JSON Formatter

## Test @qa

### Unit Tests

### Integration Tests

### Performance Tests

> [!gate]
> All tests must pass

## Deploy

### Staging

### Production
```

### Incident Response

```markdown
---
type: workflow
title: Production Incident Response
---

## Assess Severity -> severity

Determine impact scope and urgency.

- Is the service down or degraded?
- How many users affected?
- Is data at risk?

## Immediate Mitigation

Based on **severity**, take immediate action:

- [ ] Enable maintenance mode if needed
- [ ] Scale up resources if load-related
- [ ] Rollback if deployment-related

> [!gate]
> Confirm service stability before proceeding

## Root Cause Analysis @oncall

## Document and Follow-up

- [ ] Write incident report
- [ ] Create follow-up tasks
- [ ] Schedule retrospective
```

## Related

- [[Sessions]] — Workflow execution tracking
- [[Task Management]] — Task execution format
- [[Markdown Handlers]] — Event-driven context injection
