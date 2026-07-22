---
title: Agent Cards
description: Define specialized AI agents for your kiln and projects
status: implemented
tags:
  - extending
  - agents
  - ai
  - configuration
  - delegation
aliases:
  - Agents
  - AI Agents
---

# Agent Cards

Agent cards define specialized AI agents. Each card is a markdown file: YAML frontmatter for configuration, markdown body as the system prompt. Cards are the primary way to define **delegation targets** — an agent with delegation enabled can hand a task to any card by name via `delegate_session`, and the child runs as a real session with the card's prompt, model, and tool policy.

## What's in an Agent Card

- **Who is this agent?** — Name, description, system prompt
- **What can it do?** — Per-tool permissions and MCP servers
- **What model?** — Optional provider/model override (omit to inherit the spawning context's model)
- **How long may it run?** — `max_turns` caps the tool loop

## File Locations

Discovery order (later locations shadow earlier ones, by card name):

1. `~/.config/crucible/agents/` — personal cards
2. `KILN/.crucible/agents/` — kiln hidden config
3. `KILN/agents/` or `KILN/Agents/` — kiln content (shared with the team)
4. `PROJECT/.crucible/agents/` — project-scoped cards (checked into a repo)

## Basic Example

Create `agents/researcher.md`:

```markdown
---
description: Explores and synthesizes knowledge
tools:
  semantic_search: true
  read_note: true
  create_note: ask
  bash: deny
mcps:
  - context7
---

You are a research assistant specializing in knowledge exploration.

## Your Approach

- Search thoroughly before answering
- Cite sources using [[wikilinks]]
- Synthesize information from multiple notes
- Acknowledge gaps in knowledge
```

Only `description` is required. The card's name defaults to its file stem (`researcher` above); `version` defaults to `0.1.0`.

## Frontmatter Fields

| Field | Required | Description |
|-------|----------|-------------|
| `description` | Yes | Brief description (shown in delegation target listings) |
| `name` | No | Card name (default: file stem) |
| `version` | No | Semantic version (default: `0.1.0`) |
| `tools` | No | Per-tool permissions (`true`/`false`/`allow`/`ask`/`deny`) |
| `mcps` | No | MCP servers this agent can use (alias: `mcp_servers`) |
| `provider` | No | Provider override (`ollama`, `anthropic`, …); omit to inherit |
| `model` | No | Model override; omit to inherit (better portability) |
| `temperature` | No | Sampling temperature override |
| `max_tokens` | No | Max output tokens override |
| `max_turns` | No | Max tool-loop turns per message |
| `mode` | No | Initial mode (`auto`/`plan`) |
| `specialty` | No | Model category resolved via `[llm.models]` (see below) |
| `tags` | No | Tags for discovery |

## Model Resolution

A card's model resolves through one explicit chain, most specific first:

1. **Card-explicit** `provider:` / `model:` — always wins.
2. **`specialty:`** mapped through your `[llm.models]` config table.
3. **Inherit from the spawning context** — the delegating parent's
   provider/model, or the configured default for `session.create`.

The `specialty` layer keeps cards portable: the card says what *kind* of
model it wants, and each machine maps that to its own preferred model:

```toml
[llm.models]
reasoning = "openai/o1"          # provider/model — switches both
coder = "qwen2.5-coder"          # bare model — provider inherited
writing = "anthropic/claude-haiku"
```

An unmapped specialty simply falls through to inheritance, so sharing a
card with a specialty the recipient hasn't configured still works.

## Tool Permissions

```yaml
tools:
  semantic_search: true    # Always allowed, never prompts
  write_file: ask          # Always prompts (even in permissive contexts)
  bash: deny               # Not advertised, refused if called
```

Permission values:
- `true` or `allow` — auto-approve; the permission gate is skipped
- `ask` — force a prompt for every use, even for read-only tools
- `false` or `deny` — the tool is removed from the agent's toolset AND refused at dispatch if requested anyway

Tools not listed use the default behavior (safe read-only tools run freely; mutating tools go through the permission gate). Note: delegated child sessions run non-interactively — for them, `ask` is effectively `deny` unless a permission pattern or Lua hook answers the prompt.

**Trust note:** `allow` skips the interactive prompt, so only install cards from sources you trust — a kiln-shipped card granting `bash: allow` runs shell commands unattended when delegated to. The operator's `[permissions]` deny rules always win over a card's `allow`, so `deny = ["bash:*"]` in your permissions config is an absolute backstop.

## Delegating to a Card

An agent whose session has delegation enabled (`delegation_config.enabled`) sees a `delegate_session` tool listing the available cards. Delegation resolves targets in this order: agent card → ACP profile (external agents like `claude`, `opencode`).

```jsonc
// the parent agent calls:
delegate_session { "prompt": "Survey what we know about X", "target": "researcher" }
```

The child runs as a real (hidden) session: the card's system prompt, tool policy, and model, with Precognition and session persistence. See [[Help/Concepts/Delegation]].

## Using Cards from the CLI

```bash
# List / inspect / validate cards
cru agents list
cru agents show researcher
cru agents validate

# Create a session with a card-configured internal agent
cru session create --type chat   # then session.configure_agent, or:
# via RPC: session.create { configure_agent: true, agent_name: "researcher" }
```

## Writing Good Prompts

The markdown body becomes the system prompt. Write it like instructions:

**Be specific about role:**
```markdown
You are a code reviewer focused on Rust best practices.
You catch common mistakes and suggest idiomatic improvements.
```

**Define behavior:**
```markdown
## How You Work

1. Read the code carefully before commenting
2. Prioritize correctness over style
3. Explain the "why" behind suggestions
4. Acknowledge good patterns too
```

**Set boundaries:**
```markdown
## What You Don't Do

- Don't rewrite entire files
- Don't suggest unrelated refactors
- Don't ignore the user's stated goals
```

## See Also

- [[Help/Concepts/Delegation]] - Delegating tasks to other agents
- [[Help/CLI/chat]] - Chat command
- [[AI Features]] - All AI capabilities
- [[Help/Concepts/Agents & Protocols]] - MCP/ACP explained
