---
title: Agent Skills
description: Specification reference for Agent Skills — folder-based knowledge packages that agents load via ACP
status: implemented
tags:
  - concept
  - skills
  - agents
  - reference
aliases:
  - Skills Specification
  - SKILL.md
---

# Agent Skills

Agent Skills are portable knowledge packages for AI agents. A skill is a folder containing a `SKILL.md` entry point and optional supporting markdown files. Skills give agents domain-specific knowledge, instructions, and context without bloating the initial prompt.

**Key facts:**

- Specification: [agentskills.io](https://agentskills.io)
- Entry point: `SKILL.md` (required, with YAML frontmatter)
- Format: Markdown with wikilinks for navigation
- Design principle: progressive disclosure (summary first, depth on demand)
- Crucible discovers skills automatically from well-known directories

## Structure

A skill is a directory with a `SKILL.md` at its root:

```
my-skill/
├── SKILL.md          # Entry point (required)
├── concepts/         # Concept reference docs
│   ├── architecture.md
│   └── data-model.md
├── guides/           # How-to guides
│   └── getting-started.md
└── reference/        # Detailed reference material
    ├── api.md
    └── config.md
```

Only `SKILL.md` is required. Supporting files are optional and loaded on demand when the agent follows links.

## SKILL.md Format

The entry point file has YAML frontmatter followed by a markdown body:

```yaml
---
name: my-skill
description: One-sentence description of what this skill provides
license: MIT
compatibility: Claude Code, OpenCode, Gemini CLI
allowed-tools: semantic_search read_note create_note
---
```

### Frontmatter Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | 1-64 chars, lowercase alphanumeric + hyphens |
| `description` | Yes | 1-1024 chars, what the skill provides |
| `license` | No | License identifier (e.g. MIT, Apache-2.0) |
| `compatibility` | No | Which agents this skill works with |
| `allowed-tools` | No | Space-delimited list of tools this skill may use |

Additional metadata fields are preserved as key-value pairs and available to the agent.

### Body Content

After frontmatter, write a concise summary (1-2 pages) of what the skill covers. Include wikilinks to detail docs. Agents load the summary first and follow links only when they need depth.

```markdown
# Code Review

This skill provides guidelines for reviewing code in this project.

Key areas:
- [[concepts/style-guide]]: naming, formatting, idioms
- [[concepts/error-handling]]: error types and propagation patterns
- [[reference/testing]]: test structure and coverage expectations

When reviewing, check for...
```

## Progressive Disclosure

This is the core design principle. Skills avoid dumping everything into the agent's context window at once.

**How it works:**

1. `SKILL.md` is short and scannable. Agents read it on session start.
2. The body contains wikilinks to deeper reference docs.
3. Agents decide which linked docs to load based on the current task.
4. Reference docs can be arbitrarily detailed without affecting initial context cost.

**Why it matters:** Context windows are finite and attention degrades with length. A 200-line `SKILL.md` that links to 2,000 lines of reference is far more effective than a 2,200-line monolith. The agent loads what it needs, when it needs it.

## Discovery

Crucible discovers skills from three scopes, in priority order:

| Scope | Path | Priority |
|-------|------|----------|
| Personal | `~/.config/crucible/skills/` | Lowest |
| Workspace | `<project>/.<agent>/skills/` | Medium |
| Kiln | `<kiln>/skills/` | Highest |

Within each scope, Crucible globs for `*/SKILL.md` patterns. For workspace scope, it checks directories for known agents: `.claude/skills/`, `.codex/skills/`, `.opencode/skills/`, `.crucible/skills/`.

### Priority and Shadowing

When the same skill name appears in multiple scopes, the higher-priority scope wins. A workspace skill named `commit` shadows a personal skill with the same name. A kiln-scoped skill shadows both.

Shadowed skills are tracked but not loaded. You can see what's shadowed with `cru skills list`.

### Content Hashing

Each discovered skill gets a SHA-256 content hash for change detection. Crucible can detect when a skill file changes and re-index it without rescanning everything.

## Crucible Kiln as a Skill

A Crucible kiln is, structurally, a skill. The `docs/` directory with wikilinked markdown files and semantic search is exactly what the skills spec describes. Any repository with documentation becomes an agent skill when:

- A `SKILL.md` or equivalent entry point exists
- Documents use wikilinks for navigation
- [[Help/Concepts/Semantic Search|Semantic search]] is enabled (Precognition)

This means your existing knowledge base already functions as agent context. Skills are just a formalization of the pattern Crucible already uses.

## Loading via ACP

Skills are delivered to agents through the [[Help/Concepts/Agent Client Protocol|ACP]] session lifecycle:

1. **Discovery**: Crucible scans skill directories on startup
2. **Resolution**: Priority ordering resolves name conflicts
3. **Formatting**: Skills are formatted as markdown context blocks
4. **Injection**: On `session.configure_agent`, Crucible injects skill summaries into the agent's system prompt
5. **Augmentation**: Per-turn, Precognition adds semantic search results from skill content

The agent sees skills as part of its instructions. It can invoke a skill by name (e.g. `/commit`) when the task matches.

### Context Formatting

Discovered skills are formatted into a structured block:

```markdown
# Available Skills

You have access to these skills. Invoke with /<skill-name> when relevant.

## /commit
Create well-formatted git commits with conventional commit messages

## /code-review
Perform comprehensive code quality review
```

This block appears in the agent's system prompt. The agent can then request the full `SKILL.md` body when it needs detailed instructions.

## Writing Good Skills

**Keep `SKILL.md` focused.** It should answer: "What does this skill do, and when should I use it?" in under 2 pages.

**Link, don't inline.** Put detailed reference material in subdirectories and link to it. The agent will follow links when it needs depth.

**Use concrete examples.** Agents learn patterns better from examples than from abstract rules.

**Name skills with lowercase-hyphens.** The `name` field must be 1-64 characters, lowercase alphanumeric plus hyphens. Match the directory name.

**Test with real agents.** Run `cru chat -a claude` and ask the agent to use your skill. Watch whether it finds the right information or gets lost.

## CLI Commands

```bash
# List all discovered skills with scope and shadowing info
cru skills list

# Show details for a specific skill
cru skills show commit
```

## See Also

- [[Help/Concepts/Agent Client Protocol]]: ACP spec (protocol that delivers skills)
- [[Help/Concepts/Agents & Protocols]]: overview of agent architecture
- [[Help/Concepts/Semantic Search]]: how Precognition augments skill context
- [[Help/CLI/skills]]: skills command reference
