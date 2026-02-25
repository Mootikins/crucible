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

1. Spawns a new child session with the target agent
2. Passes the task description and any relevant context
3. Waits for the child agent to complete its work
4. Returns the result to the parent agent

The parent agent can then use that result however it likes: summarize it, build on it, or pass it along.

### Parent and Child Sessions

Every delegation creates a parent-child relationship between sessions. The parent session is the one that initiated the delegation. The child session runs independently but reports back when finished.

Key behaviors:

- Child sessions get their own conversation history, separate from the parent.
- Results flow back to the parent as tool call responses.
- If the parent session ends, any active child sessions are cleaned up.
- Children can't see the parent's full conversation, only the task they were given.

## Available Agents

You can delegate to any [[Agent Client Protocol]] agent that Crucible knows about. Built-in agents include:

| Agent | Good For |
|-------|----------|
| Claude Code | Complex reasoning, architecture, writing |
| OpenCode | Code generation, refactoring |
| Cursor | Codebase-wide edits, multi-file changes |
| Gemini | Research, analysis, long-context tasks |
| Codex | Code generation, quick edits |

Custom agent profiles work too. If you've defined a profile in `crucible.toml`, any agent can delegate to it by name.

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
```

### Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `enabled` | `false` | Whether this agent can delegate at all |
| `max_depth` | `1` | How many levels deep delegation can go |
| `allowed_targets` | all agents | Which agents this one can delegate to |
| `result_max_bytes` | `51200` | Maximum size of a delegation result (bytes) |
| `max_concurrent_delegations` | `3` | How many delegations can run at once |

### Depth Limits

The `max_depth` setting controls how deep delegation chains can go. With `max_depth = 1`, an agent can delegate to another agent, but that child can't delegate further. With `max_depth = 2`, one level of re-delegation is allowed.

Setting this too high risks runaway chains. For most workflows, `1` or `2` is plenty.

## Trust and Safety

Delegation follows a principle of least privilege. Child agents run with restricted capabilities compared to their parents.

**What children can't do:**

- Re-delegate (unless `max_depth` allows it, and even then with tighter restrictions)
- Access tools the parent hasn't been granted
- Read or write outside the kiln's boundaries

**What Crucible enforces:**

- Every tool call from a delegated agent goes through the same permission checks as direct calls
- Results are truncated to `result_max_bytes` to prevent context overflow
- Concurrent delegation limits prevent resource exhaustion
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
- **Cold start.** Each delegation spawns a new agent process. There's startup overhead, especially for agents that need to load models or authenticate.
- **No streaming.** The parent agent waits for the full result. You won't see the child's progress in real-time.

## See Also

- [[Agent Client Protocol]] for the underlying protocol
- [[Agents & Protocols]] for an overview of agent architecture
- [[Agent Skills]] for how agents discover and load context
