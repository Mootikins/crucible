---
title: Delegation
description: Cross-agent delegation in Crucible
status: implemented
tags:
  - delegation
  - agents
  - acp
  - orchestration
---

# Delegation

Delegation lets one agent hand off a task to another agent mid-conversation. The parent agent keeps working while a child agent tackles a specific subtask, then returns the result.

Think of it like asking a colleague for help. You're writing a report and need someone to research a specific topic. You delegate that research, get the answer back, and fold it into your work.

## Why Delegate?

Different agents have different strengths. Claude might be great at reasoning through architecture decisions, while Cursor excels at codebase-wide refactors. Delegation lets you combine these strengths in a single workflow.

Common reasons to delegate:

- **Specialization.** Route coding tasks to an agent that's better at a specific language or framework.
- **Parallel work.** Split a large task into pieces and farm them out to multiple agents simultaneously.
- **Separation of concerns.** Keep your main conversation focused while offloading research or analysis.
- **Tool access.** Some agents have capabilities others don't. Delegate to the one with the right tools.

## How It Works

Delegation flows through the `delegate_session` tool. When an agent calls this tool, Crucible:

1. Creates a real **child session** (linked to the parent via `parent_session_id`) with the target agent's configuration
2. Runs the task through the same scheduler every session uses — working tools, [[Precognition]] knowledge injection, Lua hooks, and standard persistence included
3. Waits for the child's turn to complete (or returns immediately in background mode)
4. Returns the result to the parent agent

The parent agent can then use that result however it likes: summarize it, build on it, or pass it along.

### Parent and Child Sessions

Every delegation creates a parent-child relationship between sessions. Children are full sessions in behavior, but **not first-class in visibility**:

- Children are hidden from `session.list` by default (`cru session list --include-children` reveals them; `cru session show <id>` works normally).
- Each child gets its own conversation history and transcript, persisted like any session.
- Results flow back to the parent as tool call responses; the parent also receives `delegation_spawned` / `delegation_completed` events carrying the child session id.
- Children emit their own per-turn events (text, tool calls, results) on their session id — a UI can subscribe to watch a child live.
- Cancelling, ending, archiving, or deleting the parent cascades to its children.
- Children can't see the parent's conversation, only the task description they were given.
- A child ends automatically when its delegated turn completes; its transcript remains readable.

## Available Targets

A delegation target is resolved in this order:

1. **[[Help/Extending/Agent Cards|Agent cards]]** — specialized internal agents defined as markdown cards in your kiln, project, or config directory. This is the primary way to define delegation targets: a card carries its own system prompt, optional model, and per-tool policy.
2. **[[Agent Client Protocol]] profiles** — external agents (Claude Code, OpenCode, Cursor, Gemini, Codex, or custom profiles from `crucible.toml`).

Omit the target to hand the task to a clone of the parent's own agent configuration.

## Configuration

Delegation settings live in your agent profile under `crucible.toml`. Each agent can have its own delegation rules.

```toml
[acp.agents.my-claude]
extends = "claude"

[acp.agents.my-claude.delegation]
enabled = true
max_depth = 2
allowed_targets = ["opencode", "cursor"]
result_max_bytes = 102400
max_concurrent_delegations = 3
timeout_secs = 300
```

### Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `enabled` | `false` | Whether this agent can delegate at all |
| `max_depth` | `1` | How many levels deep delegation can go |
| `allowed_targets` | all agents | Which agents this one can delegate to |
| `result_max_bytes` | `51200` | Maximum size of a delegation result (bytes) |
| `max_concurrent_delegations` | `3` | How many delegations can run at once |
| `timeout_secs` | `300` | Seconds a delegated child may run before it is cancelled |

### Depth Limits

The `max_depth` setting controls how deep delegation chains can go. With `max_depth = 1`, an agent can delegate to another agent, but that child can't delegate further. With `max_depth = 2`, one level of re-delegation is allowed.

Setting this too high risks runaway chains. For most workflows, `1` or `2` is plenty.

## Trust and Safety

Delegation follows a principle of least privilege. Child agents run with restricted capabilities compared to their parents.

**What children can't do:**

- Re-delegate deeper than `max_depth` allows (depth is derived from the parent chain, so a child can't lift its own cap)
- Use tools their agent card marks `deny`
- Read or write files outside the workspace, kilns, and session directory (filesystem containment)
- Answer permission prompts — children run non-interactively, so a tool that would prompt is denied unless a permission pattern, Lua hook, or `[permissions]` config allows it

**What Crucible enforces:**

- Every tool call from a delegated child goes through the same permission gate as direct calls, including the `[permissions]` config and the project `[security.shell]` policy
- The child's **provider trust level** (not a blanket assumption) must satisfy the kiln's data classification — a local-model card can serve a confidential kiln that a cloud target cannot
- Results are truncated to `result_max_bytes` to prevent context overflow
- Concurrent delegation limits prevent resource exhaustion; `timeout_secs` cancels hung children
- Child sessions inherit the parent's kiln context but not its full conversation

For more on how Crucible scopes agent permissions, see [[Agent Client Protocol]].

## Example Workflow

Here's what delegation looks like in practice. You're chatting with Claude through Crucible:

> **You:** Refactor the authentication module to use JWT tokens. Have another agent update all the tests.

Claude handles the auth refactor directly, then delegates the test updates:

1. Claude calls `delegate_session` targeting `opencode` with the task "Update all authentication tests to work with the new JWT-based auth module"
2. Crucible spawns an OpenCode session, passes the task
3. OpenCode reads the updated auth code, rewrites the tests, and returns a summary
4. Claude receives the result and reports back to you with the full picture

All of this happens within your single conversation. You see the delegation happen and get the combined result.

## Limitations

A few things to keep in mind:

- **No shared state.** Child agents don't see the parent's conversation history. They only get the task description you provide.
- **Result size caps.** Large outputs get truncated. If a child agent produces a massive diff, only the first `result_max_bytes` come back.
- **Cold start for external agents.** Delegating to an ACP target spawns a new agent process. Internal card targets have no process overhead.
- **No streaming into the parent.** The parent waits for the full result. The child's live progress IS streamed as events on the child's own session id, but the parent transcript only records the delegation call and its result.

## See Also

- [[Help/Extending/Agent Cards]] for defining specialized delegation targets
- [[Agent Client Protocol]] for the external-agent protocol
- [[Agents & Protocols]] for an overview of agent architecture
- [[Agent Skills]] for how agents discover and load context
