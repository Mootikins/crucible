# Agent Naming Clarification

**Date:** 2025-12-04
**Status:** Approved

## Problem

The codebase has multiple uses of "agent" terminology that create confusion:

- `AgentDefinition` - prompt template stored as markdown
- `ChatAgent` trait - runtime handle to chat backend
- `AgentProvider` trait - placeholder with no implementation
- `AgentRegistry`, `AgentLoader`, etc. - supporting types
- ACP agents - external processes (claude-code, gemini-cli)

The word "agent" is overloaded, making it unclear what each type represents.

## Solution

Establish clear terminology:

| Term | Meaning | Lifecycle |
|------|---------|-----------|
| `AgentCard` | Static definition (prompt + metadata) | Loaded from disk, immutable |
| `AgentHandle` | Runtime handle to active agent | Created on spawn, dropped on completion |
| `LlmProvider` | Text generation backend | Long-lived, reusable |
| ACP agent | External process | Managed by ACP protocol |

### Naming Analogy

- **AgentCard** follows the "Model Card" pattern from HuggingFace - metadata *about* something, not the thing itself
- **AgentHandle** follows systems programming conventions - a runtime reference that gets dropped when done

## Changes

### 1. Rename `AgentDefinition` → `AgentCard`

Location: `crates/crucible-core/src/agent/types.rs`

Markdown files with YAML frontmatter describing an agent:
- name, description, system_prompt
- capabilities, required_tools, tags
- config, dependencies

### 2. Simplify AgentCard fields

Remove over-engineered fields that don't serve the core use case:

**Remove from `AgentCard`:**
- `Skill.experience_years`
- `Skill.certifications`
- `Skill.proficiency` (keep just name + category)
- `Personality` struct entirely (tone/style can be in system_prompt)
- `AgentCapabilities` (redundant with tags + description)

**Keep:**
- name, version, description
- system_prompt
- capabilities (simplified: just name + description)
- required_tools, optional_tools
- tags, config, dependencies
- status, author, documentation_url
- created_at, updated_at

### 3. Rename `ChatAgent` trait → `AgentHandle`

Location: `crates/crucible-core/src/traits/chat.rs`

Runtime handle to an active agent:
- `send_message()` - send and receive
- `set_mode()` - plan/act/auto
- `is_connected()` - check liveness

### 4. Delete `AgentProvider` trait

Location: `crates/crucible-core/src/traits/agent.rs`

This is a placeholder with no implementations. The actual needs are covered by:
- `LlmProvider` - text generation
- `AgentHandle` - chat sessions

Delete the file entirely.

### 5. Update related types

| Old | New |
|-----|-----|
| `AgentRegistry` | `AgentCardRegistry` |
| `AgentLoader` | `AgentCardLoader` |
| `AgentFrontmatter` | `AgentCardFrontmatter` |
| `AgentQuery` | `AgentCardQuery` |
| `AgentMatch` | `AgentCardMatch` |
| `AgentStatus` | `AgentCardStatus` |

### 6. Update directory references

- `.crucible/agents/` → keep as-is (cards describe agents)
- `~/.config/crucible/agents/` → keep as-is

## Non-Changes

- **ACP terminology** - keep as-is, it's protocol-level
- **`AgentRuntime` in crucible-llm** - different concept (autonomous tool-calling loop)
- **Subagent execution model** - future work, don't over-design now

## A2A Semantics Note

An `AgentHandle` represents our interface to one logical agent. If that agent internally spawns subagents (like claude-code's Task tool), that's opaque to us. We don't manage or inspect nested agents - same as how a file handle doesn't care about filesystem internals.

## Files to Modify

```
crates/crucible-core/src/agent/
  types.rs        - Rename types, simplify fields
  mod.rs          - Rename AgentRegistry
  loader.rs       - Rename AgentLoader
  matcher.rs      - Update type references
  tests.rs        - Update tests

crates/crucible-core/src/traits/
  agent.rs        - DELETE
  chat.rs         - Rename ChatAgent → AgentHandle
  mod.rs          - Update exports

crates/crucible-core/src/lib.rs - Update re-exports

crates/crucible-cli/src/chat/
  mod.rs          - Update imports
  session.rs      - Update AgentHandle usage

crates/crucible-acp/src/
  client.rs       - Implement AgentHandle (was ChatAgent)
```

## Migration

This is a breaking change to internal APIs. No user-facing impact since agent cards aren't in active use yet.
