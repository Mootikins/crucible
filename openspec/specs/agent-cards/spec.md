# Agent Cards Specification

## Overview

Agent cards are markdown files with YAML frontmatter that define reusable agent configurations. They follow the "Model Card" pattern from HuggingFace - metadata about an agent, not the agent itself.

Agent cards enable:
- **Reusable prompts**: Define specialized system prompts once, invoke by name
- **Discovery**: Find agents by tags or text search
- **Extensibility**: Foundation for future agent orchestration and ACP delegation

## File Format

Agent cards are markdown files (`.md`) with YAML frontmatter:

```markdown
---
type: agent
name: "Agent Name"
version: "1.0.0"
description: "Brief description of what this agent does"
tags:
  - "category"
  - "specialty"
mcp_servers:
  - "server-name"
config:
  temperature: 0.7
---

# System Prompt

The markdown body becomes the agent's system prompt.

## Instructions

Detailed instructions for the agent go here.
```

## Frontmatter Schema

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Human-readable agent name. Must be non-empty. |
| `version` | string | Semantic version (e.g., "1.0.0"). Must be valid semver. |
| `description` | string | Brief description (1-2 sentences). Must be non-empty. |

### Recommended Fields

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Should be `"agent"` to identify this as an agent card. Enables DB-indexed discovery once entity type system is implemented. |

### Optional Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `tags` | string[] | `[]` | Tags for categorization and discovery |
| `mcp_servers` | string[] | `[]` | MCP servers this agent uses |
| `config` | object | `{}` | Default configuration values |

## System Prompt Extraction

The markdown body (everything after the frontmatter closing `---`) becomes the agent's system prompt. The entire body is used as-is, preserving:
- Headings and structure
- Code blocks
- Lists and formatting
- All markdown content

The system prompt must be non-empty.

## File Naming Conventions

- Use lowercase with hyphens: `code-reviewer.md`, `kiln-specialist.md`
- Name should reflect the agent's purpose
- File name does not need to match the `name` field (but consistency helps)

## Directory Structure and Load Order

Agent cards are loaded from multiple locations. Later sources override earlier ones by `name`.

### Global Scope

1. `~/.config/crucible/agents/` - Global default directory
2. Paths from `~/.config/crucible/config.toml` → `agent_directories`

### Kiln Scope

3. `KILN_DIR/.crucible/agents/` - Kiln config directory (hidden)
4. `KILN_DIR/agents/` - Kiln content directory (visible, shareable)
5. Paths from `KILN_DIR/.crucible/config.toml` → `agent_directories`

### DB-Indexed Discovery (Future)

6. Notes with `type: agent` in frontmatter (discovered via database index)

This will be enabled once the entity type system is implemented.

When agents have the same `name`, later sources take precedence.

### Path Resolution

Paths in `agent_directories` config:
- **Absolute paths** (`/path/to/agents`, `~/my-agents`): Used as-is
- **Relative paths** (`./docs/agents`, `agents`): Relative to config file location
- **No escaping kiln**: Relative paths like `../outside` are not allowed; use absolute paths instead

### Example Configuration

```toml
# ~/.config/crucible/config.toml
agent_directories = ["/home/user/shared-agents"]

# KILN_DIR/.crucible/config.toml
agent_directories = ["./docs/agents"]
```

Load order becomes:
1. `~/.config/crucible/agents/`
2. `/home/user/shared-agents/`
3. `KILN_DIR/.crucible/agents/`
4. `KILN_DIR/agents/`
5. `KILN_DIR/docs/agents/`
6. Any note in kiln with `type: agent` frontmatter

### Directory Layout Example

```
~/.config/crucible/
├── config.toml                    # Global config
└── agents/
    ├── general.md                 # Default system agent
    └── code-reviewer.md           # Shared across kilns

KILN_DIR/
├── .crucible/
│   ├── config.toml                # Kiln config
│   └── agents/
│       └── project-config.md      # Kiln-specific (hidden)
├── agents/
│   └── domain-expert.md           # Kiln-specific (shareable)
└── notes/
    └── my-custom-agent.md         # Has type: agent in frontmatter
```

## Validation Rules

The loader validates:

1. **Frontmatter present**: File must start with `---` and have valid YAML
2. **Required fields**: `name`, `version`, `description` must be present and non-empty
3. **Version format**: Must be valid semantic version (X.Y.Z)
4. **System prompt**: Markdown body must be non-empty

Note: `type: agent` is recommended but not currently enforced. It will enable DB-indexed discovery once the entity type system is implemented.

Invalid agent cards are skipped with a warning during directory loading.

## Runtime Behavior

### Loading

Agent cards are loaded on CLI startup from directories, and discovered via DB index for `type: agent` notes. Each card receives:
- A unique `id` (UUID, generated at load time)
- A `loaded_at` timestamp

### Querying

Agent cards can be found by:
- **Tags**: Exact match on tag values
- **Text search**: Substring match in name and description

### Caching

Agent cards are cached in the database after first discovery. Cache is refreshed when:
- File modification time changes
- Kiln is re-indexed
- `cru agents refresh` is run (future)

## Example Agent Cards

### Minimal Agent Card

```markdown
---
type: agent
name: "Simple Helper"
version: "1.0.0"
description: "A minimal agent card example"
---

You are a helpful assistant. Answer questions concisely.
```

### Full Agent Card

```markdown
---
type: agent
name: "Kiln Specialist"
version: "1.0.0"
description: "Expert in zettelkasten-style atomic note management"
tags:
  - "documentation"
  - "zettelkasten"
  - "knowledge-management"
mcp_servers:
  - "crucible"
config:
  max_note_length: 500
---

# Kiln Documentation Specialist

You are an expert in zettelkasten-style knowledge management...
```

## Future Extensions

### ACP Delegation (Phase 5)

Agent cards may gain an `acp_server` field for delegating execution:

```yaml
acp_server: "claude-code"
```

This would route the agent's execution to an external ACP-compatible agent.

### Agent Orchestration

Future work may enable:
- Agent-to-agent communication
- Workflow composition
- Conditional routing based on context

## Implementation Status

**Status**: ✅ Phase 2 Complete (Specification)

- ✅ `AgentCard` type with simplified fields
- ✅ `AgentCardLoader` for parsing markdown with frontmatter
- ✅ `AgentCardMatcher` for tag and text search
- ✅ `AgentCardRegistry` for managing loaded cards
- ✅ Validation of required fields and semver
- ✅ Specification document
- ✅ Example agent cards
- ⏳ CLI commands (`cru agents list/show/validate`) - Phase 3
- ⏳ Chat integration (`@agent` syntax) - Phase 4
- ⏳ ACP delegation - Phase 5

## Related Specs

- **acp-integration**: Agent cards can delegate to ACP servers
- **tool-execution**: Agents can reference MCP servers for tool access
