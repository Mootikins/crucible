---
description: Ideas for DAG-based workflow markup and soft/hard hook guides
status: idea
tags:
  - workflows
  - hooks
  - markdown-extensions
  - dags
  - agent-orchestration
related:
  - "[[Help/Workflows/Markup]]"
  - "[[Help/Extending/Workflow Authoring]]"
---

# Executable Markdown and Hooks

Ideas for extending workflow markup with DAG execution and hook-based guides.

## Two Layers of Execution

| Layer | Purpose | Vocabulary | Processor |
|-------|---------|------------|-----------|
| **Agent guardrails** | Shape LLM behavior | Checkboxes + prose | Agent reads & infers |
| **Programmatic DAGs** | Formal execution | Rich metadata | Lua/TaskGraph code |

**Key insight**: Tasks ARE inline system prompts. The agent's context window IS the runtime.

## Checkbox Vocabulary (QWERTY-typeable)

All characters from Obsidian ecosystem (Tasks plugin, Minimal/Things/ITS themes):

### Core States
| Char | Status | Agent Interpretation |
|------|--------|---------------------|
| `[ ]` | Pending | Not started |
| `[x]` | Done | Completed |
| `[/]` | In progress | Currently working |
| `[-]` | Cancelled | Skip this |
| `[!]` | Important | **Hard guide** - non-negotiable |

### Extended States (for agent flows)
| Char | Status | Agent Interpretation |
|------|--------|---------------------|
| `[>]` | Forwarded | Delegate to subagent |
| `[<]` | Scheduled | Wait/defer |
| `[?]` | Question | **Soft guide** - ask before assuming |
| `[*]` | Starred | Priority - do first |
| `[R]` | Research | Parallel-safe, use cheap model |
| `[~]` | Conflict | Needs resolution before continuing |

### Plan→Split→Verify Pattern

```markdown
- [ ] Plan the decomposition
- [>] Research API options
- [>] Research auth patterns
- [~] Resolve any conflicts
- [!] Verify before merge
```

**Economics**:
- Plan/verify = expensive model (needs judgment)
- `[>]` tasks = cheap/local models (isolated, parallel)
- Context "waste" is minimal IF plan is explicit

**Challenges**:
- Context cliff: each subagent starts cold, plan must be fully explicit
- Merge conflicts: independent agents may contradict, verify must reconcile
- Retry semantics: same prompt? with error? escalate model?

## The Trichotomy

| Concept | Trigger | Scope | Example |
|---------|---------|-------|---------|
| **Skill** | Explicit invocation | Single capability | `/deep-research` |
| **Workflow** | Goal-oriented | Multi-step DAG | "Review and merge PR" |
| **Hook** | Event/condition | Cross-cutting | "On test fail → debug" |

A skill with a DAG *is* a workflow. The distinction may be:
- **Skill**: Reusable building block (function)
- **Workflow**: Composed skills for specific outcome (program)
- **Hook**: Reactive modifier that can invoke either

## DAG Syntax in Markdown

Extend prose workflows with explicit structure:

```markdown
# PR Review Workflow

## Steps

[fetch] Fetch the PR diff and comments.
  → [analyze], [check-ci]

[analyze] Analyze code changes for issues.
  → [summarize]

[check-ci] Check CI status.
  → [summarize]

[summarize] Create review summary from analysis and CI.
  requires: [analyze], [check-ci]
```

Or use Mermaid-style inline:

```markdown
The workflow follows this structure:

    fetch → analyze ─┐
      │              ├→ summarize
      └→ check-ci ──┘

Each step can include prose instructions...
```

## Soft vs Hard Guides (Hooks)

Hooks as guardrails with different enforcement levels:

### Soft Guides (Advisory)

```yaml
hooks:
  on_file_write:
    suggest: "Consider adding tests for new functions"
    severity: info
```

- Yellow squiggle, not red
- LLM sees suggestion, decides whether to act
- Logged for pattern learning

### Hard Guides (Blocking)

```yaml
hooks:
  on_commit:
    require:
      - tests_pass
      - no_lint_errors
    severity: error
    block: true
```

- Red error, cannot proceed
- Forces correction before continuing
- Like pre-commit hooks

### Reward Signals

```yaml
hooks:
  on_test_pass:
    reward: "Good - tests passing, continue confidently"
  on_coverage_increase:
    reward: "Excellent - coverage improved"
```

- Positive reinforcement
- Could feed into skill refinement over time

## Learning from Usage

### Explicit Capture

> "I just did research → plan → implement → test. Encode this as a workflow."

System extracts:
- Steps taken
- Conditions checked
- Branches taken

### Implicit Pattern Detection

Over time, observe:
- "You always run tests after editing `src/`"
- "You check docs before API changes"

Suggest: "Create hook for this pattern?"

## Relation to Claude Code Skills

Claude Code skills are prompt-based. This extends them with:

1. **Structure**: DAG of steps, not just linear prompt
2. **Reactivity**: Hooks fire on conditions
3. **Composability**: Skills reference other skills
4. **Learning**: Patterns become skills over time

```markdown
---
type: skill
uses:
  - "[[deep-research]]"
  - "[[tdd-workflow]]"
hooks:
  on_error: soft-guide("Check if this is a known issue pattern")
  on_test_fail: hard-guide("Must fix before proceeding")
---

# My Custom Workflow

First, use [[deep-research]] to understand the codebase.
Then, apply [[tdd-workflow]] for implementation.
```

## Existing Infrastructure: task.rs

**Key insight**: We already have executable markdown in `crucible-core/src/parser/types/task.rs`:

### Current Capabilities

```rust
// TaskFile parses TASKS.md with frontmatter and checkbox items
// TaskGraph provides DAG with topo_sort, cycle detection, ready_tasks
```

**Supported Checkboxes:**
| Char | Status | Purpose |
|------|--------|---------|
| `[ ]` | Pending | Step not started |
| `[x]` | Done | Step completed |
| `[/]` | InProgress | Currently executing |
| `[-]` | Cancelled | Step skipped |
| `[!]` | Blocked | **Hard guide** - requires action |

**Inline Metadata:**
```markdown
- [ ] Implement feature [id:: feat-1] [deps:: research] [priority:: high]
```

- `[id:: x]` - Task identifier for DAG edges
- `[deps:: a, b]` - Dependencies (enables topo_sort)
- Any key-value metadata (priority, estimate, etc.)

### Obsidian Checkbox Extensions

Themes like Things, Minimal, and plugins like Tasks/ToggleList add:

| Char | Meaning | Skill/Workflow Use |
|------|---------|-------------------|
| `[>]` | Rescheduled | Deferred step |
| `[<]` | Scheduled | Future step |
| `[?]` | Question | **Soft guide** - advisory |
| `[*]` | Starred | Priority marker |
| `[i]` | Info | Context/note |
| `[I]` | Idea | Captured thought |

### Mapping to Skills/Workflows

```markdown
---
type: skill
name: tdd-workflow
---

# TDD Workflow

- [ ] Write failing test [id:: test] [phase:: red]
- [ ] Implement to pass [id:: impl] [deps:: test] [phase:: green]
- [ ] Refactor [id:: refactor] [deps:: impl] [phase:: refactor]
- [?] Consider edge cases [soft:: true]
- [!] Tests must pass before commit [hard:: true]
```

**Interpretation:**
- `[?]` with `[soft:: true]` = advisory suggestion (yellow)
- `[!]` with `[hard:: true]` = blocking requirement (red)
- DAG from `[deps::]` = execution order
- `[phase::]` = grouping/labels

### Extension Points

1. **Add CheckboxStatus variants** to lists.rs:
   - `Question` for `[?]`
   - `Deferred` for `[>]`
   - `Scheduled` for `[<]`
   - `Starred` for `[*]`

2. **Metadata conventions** for skill behavior:
   - `[soft:: true/false]` - advisory vs blocking
   - `[phase:: name]` - workflow phase
   - `[hook:: on_error]` - event binding

3. **TaskGraph already handles**:
   - Topological ordering
   - Cycle detection
   - Ready task computation
   - Parallel step identification (tasks with same deps)

## Open Questions

1. How to represent parallel execution in prose?
   - **Answer**: Tasks with same deps can run in parallel (TaskGraph.ready_tasks)
2. Should hooks be inline or separate config?
   - **Answer**: Inline via `[soft::]`/`[hard::]` metadata
3. How to handle hook conflicts between composed skills?
   - Hard guides from any source = blocking
   - Soft guides accumulate
4. What's the runtime model? (Interpreter? Compiler to DAG?)
   - **Answer**: TaskGraph already IS the DAG; execution is iteration

## Sources

### Obsidian Checkbox Ecosystem
- [Obsidian Tasks Plugin - Statuses](https://publish.obsidian.md/tasks/Getting+Started/Statuses)
- [Obsidian Tasks - Status Collections](https://publish.obsidian.md/tasks/Reference/Status+Collections/About+Status+Collections)
- [Minimal Theme - Checklists](https://minimal.guide/checklists)
- [Things Theme](https://github.com/colineckert/obsidian-things)
- [ITS Theme - Alternate Checkboxes](https://github.com/SlRvb/Obsidian--ITS-Theme/blob/main/Guide/Alternate-Checkboxes.md)
- [Tasks Plugin Alternate Checkboxes Discussion](https://github.com/obsidian-tasks-group/obsidian-tasks/discussions/509)

## See Also

- [[Help/Workflows/Markup]] - Planned prose syntax
- [[Help/Core/Sessions]] - Tracking execution state
