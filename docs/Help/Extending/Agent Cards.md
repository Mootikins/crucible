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
- **What can it do?** - Which tools it can access
- **How does it behave?** - System prompt and instructions
- **What model powers it?** - LLM configuration

## Basic Example

Create `Agents/Researcher.md`:

```markdown
---
title: Researcher
description: Explores and synthesizes knowledge
type: agent
model: llama3.2
tools:
  - semantic_search
  - read_note
  - search_by_tags
---

# Researcher

You are a research assistant specializing in knowledge exploration.

## Your Approach

- Search thoroughly before answering
- Cite sources using [[wikilinks]]
- Synthesize information from multiple notes
- Acknowledge gaps in knowledge

## Available Tools

You can search semantically, read notes, and filter by tags.
```

## Frontmatter Fields

| Field | Required | Description |
|-------|----------|-------------|
| `title` | Yes | Agent name |
| `description` | Yes | Brief description |
| `type` | Yes | Must be `agent` |
| `model` | No | LLM model override |
| `tools` | No | List of allowed tools |
| `temperature` | No | Response creativity (0-1) |
| `max_tokens` | No | Response length limit |

## Available Tools

Common tools to include:

**Search tools:**
- `semantic_search` - Find by meaning
- `text_search` - Find by keywords
- `search_by_tags` - Filter by tags
- `search_by_properties` - Filter by frontmatter

**Note tools:**
- `read_note` - Read note content
- `create_note` - Create new notes (requires act mode)
- `update_note` - Modify notes (requires act mode)

**Kiln tools:**
- `list_notes` - List all notes
- `get_stats` - Kiln statistics

**External tools (if [[Help/Extending/MCP Gateway|MCP Gateway]] configured):**
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
title: Creative Writer
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
title: Reader
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
title: Local Agent
type: agent
model: llama3.2
provider: ollama
---

---
title: Cloud Agent
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
