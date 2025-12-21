---
title: Dev Kiln Architecture
description: Structure, purpose, and conventions for the Crucible dev-kiln documentation system
tags:
  - meta
  - architecture
  - documentation
---

# Dev Kiln Architecture

This document defines the structure, purpose, and conventions for the Crucible dev-kiln - a self-documenting, self-testing example kiln.

## Purpose

The dev-kiln serves four roles:

1. **User Guide** - Tutorial content for learning Crucible
2. **Reference System** - `:h`-style queryable documentation
3. **Test Fixture** - Integration tests run against this content
4. **Coverage Check** - CI verifies code changes are documented

## Directory Structure

```
examples/dev-kiln/
│
├── Index.md                    # Entry point, links to all sections
│
├── Extending Crucible.md       # MoC - extension points
├── Search & Discovery.md       # MoC - finding things
├── AI Features.md              # MoC - agents, chat, protocols
├── Configuration.md            # MoC - all config topics
│
├── Guides/                     # Tutorial content (learn by doing)
│   ├── Getting Started.md
│   ├── Your First Kiln.md
│   └── Basic Commands.md
│
├── Organization Styles/        # Reference for PKM patterns
│   ├── Index.md               # Overview and comparison
│   ├── PARA.md                # Projects, Areas, Resources, Archive
│   ├── Johnny Decimal.md      # Numbered organization
│   ├── Zettelkasten.md        # Atomic linked notes
│   └── Choosing Your Structure.md
│
├── Help/                       # Reference docs (cru help <topic>)
│   ├── Wikilinks.md           # [[link]] syntax
│   ├── Frontmatter.md         # YAML metadata
│   ├── Tags.md                # #tag system
│   ├── Block References.md    # ^block-id syntax
│   │
│   ├── Concepts/              # User-facing foundational ideas
│   │   ├── Kilns.md
│   │   ├── Semantic Search.md
│   │   ├── The Knowledge Graph.md
│   │   ├── Agents & Protocols.md
│   │   └── Plaintext First.md
│   │
│   ├── CLI/                   # Command reference
│   │   ├── Index.md
│   │   ├── search.md
│   │   ├── process.md
│   │   ├── stats.md
│   │   └── chat.md
│   │
│   ├── Config/                # Configuration reference
│   │   ├── llm.md
│   │   ├── embedding.md
│   │   ├── storage.md
│   │   └── agents.md
│   │
│   ├── Rune/                  # Rune scripting guides
│   │   ├── Language Basics.md
│   │   ├── Crucible API.md
│   │   └── Best Practices.md
│   │
│   ├── Extending/             # Extension guides
│   │   ├── Creating Plugins.md
│   │   ├── Event Hooks.md
│   │   ├── MCP Gateway.md
│   │   ├── Custom Tools.md
│   │   ├── Agent Cards.md
│   │   └── Workflow Authoring.md
│   │
│   ├── Workflows/             # Workflow system
│   │   ├── Index.md
│   │   ├── Markup.md
│   │   └── Sessions.md
│   │
│   └── Query/                 # Query language
│       └── Index.md
│
├── Agents/                     # Example agent cards (IN the kiln)
│   ├── Researcher.md
│   ├── Coder.md
│   └── Reviewer.md
│
├── Scripts/                    # Example Rune scripts
│   ├── Auto Tagging.rn
│   └── Daily Summary.rn
│
├── Meta/                       # About this kiln
│   ├── Dev Kiln Architecture.md  # This file
│   └── Dogfooding Notes.md
│
└── Config.toml                 # Kiln configuration
```

## File Naming Conventions

- **Titlecase with spaces**: `Getting Started.md` not `getting-started.md`
- **Subtopics with hyphen**: `Parser - Block Extraction.md`
- **Prose-first**: Names should read like document titles
- **Rune scripts**: `.rn` extension

## Content Conventions

### Frontmatter

Every note should have frontmatter:

```yaml
---
title: Getting Started
description: Your first steps with Crucible
tags:
  - guide
  - beginner
order: 1
---
```

### Wikilinks

Use wikilinks for internal references:

```markdown
See [[Wikilinks]] for syntax details.
For organization ideas, check [[Organization Styles/PARA]].
```

### Code Examples

Reference actual code locations:

```markdown
The process command is implemented in:
`crates/crucible-cli/src/commands/process.rs`
```

### Help References

Format for `:h`-style lookups:

```markdown
## See Also

- `:h frontmatter` - YAML metadata format
- `:h config.llm` - LLM provider configuration
```

## Source Code References

When writing Help/ docs, reference these code locations:

### CLI Commands
- `crates/crucible-cli/src/commands/` - All CLI commands
- `crates/crucible-cli/src/main.rs` - Command registration

### Parser
- `crates/crucible-parser/src/` - Markdown parsing
- `crates/crucible-core/src/parser/types/` - Parser types

### Storage
- `crates/crucible-surrealdb/src/` - Database layer
- `crates/crucible-core/src/storage/` - Storage traits

### Agents
- `crates/crucible-agents/src/` - Agent implementation
- `crates/crucible-core/src/traits/chat.rs` - AgentHandle trait

### Configuration
- `crates/crucible-config/src/` - Configuration loading and types

### MCP/Tools
- `crates/crucible-tools/src/` - MCP server
- `crates/crucible-rune/src/` - Rune integration

## Test Integration

Tests should verify:

1. **All notes parse** - No syntax errors
2. **Links resolve** - No broken wikilinks
3. **Frontmatter valid** - Required fields present
4. **Code refs exist** - Referenced files/lines exist
5. **Examples run** - Code snippets are valid

Test location: `crates/crucible-core/tests/dev_kiln.rs`

```rust
#[test]
#[ignore]
fn dev_kiln_all_notes_parse() { /* ... */ }

#[test]
#[ignore]
fn dev_kiln_links_resolve() { /* ... */ }

#[test]
#[ignore]
fn dev_kiln_code_refs_valid() { /* ... */ }
```

## Writing Guidelines

### For Guides/
- Assume no prior knowledge
- Step-by-step instructions
- Show expected output
- Link to Help/ for details

### For Help/
- Reference format, not tutorial
- Include all options/flags
- Link to source code
- Show edge cases

### For Organization Styles/
- Describe the system objectively
- Show example structure
- Note tradeoffs
- Don't prescribe - inform

## Verification Checklist

Before considering a doc complete:

- [ ] Frontmatter with title, description, tags
- [ ] Links to related docs
- [ ] Code references verified against source
- [ ] Examples tested
- [ ] Spelling/grammar checked
