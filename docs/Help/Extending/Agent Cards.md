---
description: Define AI agent personalities and capabilities for your kiln
status: implemented
tags:
  - extending
  - agents
  - ai
  - configuration
aliases:
  - Agents
  - AI Agents
---

# Agent Cards

Agent cards define who your AI assistants are and what they can do. Each card is a markdown file that combines a system prompt with tool access and configuration.

## What's in an Agent Card

An agent card answers these questions:
- **Who is this agent?** - Name, role, personality
- **What can it do?** - Which tools and MCPs it can access
- **How does it behave?** - System prompt and instructions
- **What kind of model?** - Specialty (not specific model)

## File Location

Place agent cards in:
- `~/.config/crucible/agents/` - Personal agents
- `KILN/Agents/` - Project-specific agents (shared with team)

## Basic Example

Create `Agents/Researcher.md`:

```markdown
---
description: Explores and synthesizes knowledge
specialty: reasoning
tools:
  semantic_search: true
  read_note: true
  create_note: ask
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

## Frontmatter Fields

| Field | Required | Description |
|-------|----------|-------------|
| `description` | Yes | Brief description |
| `specialty` | Yes | Model category (coder, vision, reasoning, etc.) |
| `tools` | No | Tool permissions (true/ask/deny) |
| `mcps` | No | List of MCP servers to connect |
| `model` | No | Specific model override (avoid for portability) |

## Specialty Field

The `specialty` field maps to a model category from your config. This keeps agent cards portable - recipients use their own preferred model for each specialty.

| Specialty | Use Case |
|-----------|----------|
| `coder` | General programming tasks |
| `vision` | Image analysis, diagrams |
| `designer` | UI/UX, visual design |
| `writing` | Documentation, prose |
| `reasoning` | Complex analysis, planning |

Configure your preferred models in `config.toml`:

```toml
[models]
coder = "claude-sonnet-4"
vision = "claude-sonnet-4"
reasoning = "o1-preview"
writing = "claude-haiku"
```

## Tool Permissions

Tools can have three permission levels that interact with [[Help/TUI/Modes|runtime modes]]:

```yaml
tools:
  semantic_search: true    # Always allowed
  write_file: ask          # Prompt for permission
  execute_command: deny    # Never allowed for this agent
```

Permission values:
- `true` or `allow` - Auto-approve
- `ask` - Prompt user for each use  
- `false` or `deny` - Block the tool

Tools not listed use the current mode's default behavior.

## MCP Connections

The `mcps` field lists MCP servers this agent can access:

```yaml
mcps:
  - github        # GitHub API access
  - context7      # Documentation lookup
  - filesystem    # Local file access
```

MCP servers must be configured in your `config.toml`:

```toml
[mcps.github]
command = "npx"
args = ["-y", "@anthropic/mcp-server-github"]
env = { GITHUB_TOKEN = "..." }

[mcps.context7]
url = "https://context7.example.com"
```

## Available Tools

Common tools to include:

**Search tools:**
- `semantic_search` - Find by meaning
- `text_search` - Find by keywords
- `search_by_tags` - Filter by tags
- `search_by_properties` - Filter by frontmatter

**Note tools:**
- `read_note` - Read note content
- `create_note` - Create new notes
- `update_note` - Modify notes

**Kiln tools:**
- `list_notes` - List all notes
- `get_stats` - Kiln statistics

**External tools (via MCP):**
- `gh_search_code` - GitHub code search
- `fs_read_file` - Filesystem access

## Directory Structure

```
your-kiln/
├── Agents/
│   ├── Researcher.md    # Research-focused
│   ├── Coder.md         # Code-focused
│   ├── Reviewer.md      # Quality review
│   └── Custom.md        # Your own
```

## Using Agents

### From CLI

```bash
# Chat with default agent
cru chat

# Chat with specific agent
cru chat --agent Researcher

# List available agents
cru agents list
```

### In Chat

```
/agent Researcher
/agent Coder
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

## Examples in This Kiln

- [[Agents/Researcher]] - Deep exploration
- [[Agents/Coder]] - Code analysis
- [[Agents/Reviewer]] - Quality review

## Configuration Override

Agents can override global settings:

```yaml
---
type: agent
model: claude-3-opus
temperature: 0.9
max_tokens: 4000
---
```

This agent uses a different model and higher temperature than the default.

## Tool Restrictions

Limit what an agent can do:

```yaml
---
type: agent
tools:
  - read_note
  - semantic_search
# No create_note or update_note - read-only agent
---
```

## Custom Model Endpoints

Use different providers per agent:

```yaml
---
type: agent
model: llama3.2
provider: ollama
---

---
type: agent
model: gpt-4
provider: openai
---
```

## See Also

- [[Help/Config/agents]] - Agent configuration
- [[Help/CLI/chat]] - Chat command
- [[AI Features]] - All AI capabilities
- [[Help/Concepts/Agents & Protocols]] - MCP/ACP explained
