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

Workflows define high-level planning and orchestration in readable markdown. Unlike [[Tasks]] which focus on execution details, workflows describe *what* needs to happen and *why*, letting the system derive execution steps.

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

## Checkbox Semantics

Extended checkbox syntax for richer status:

| Syntax | Meaning |
|--------|---------|
| `- [ ]` | Pending |
| `- [x]` | Complete |
| `- [/]` | In progress |
| `- [-]` | Blocked |
| `- [?]` | Needs clarification |

## Parallel Execution (Future)

*This is post-MVP. Possible approaches being considered:*

**Heading suffix:**
```markdown
## Build Artifacts (parallel)

- [ ] Build frontend
- [ ] Build backend
```

**Symbol prefix:**
```markdown
- [ ] &Build frontend
- [ ] &Build backend
- [ ] Run tests (waits for above)
```

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
- [[Tasks]] — Task execution format
- [[Markdown Handlers]] — Event-driven context injection
