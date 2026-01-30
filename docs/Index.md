---
description: The dev-kiln - documentation and working example for Crucible
status: implemented
tags:
  - index
  - guide
  - welcome
order: 0
---

# Welcome to Crucible

**Crucible** is a local-first AI assistant where every conversation becomes a searchable note. Chat with AI agents, build a knowledge graph from wikilinks, extend with plugins in multiple languages — all backed by markdown files you own.

## What is This?

This **dev-kiln** serves as:

- **Learning Environment** — Tutorials and guides to get started
- **Reference System** — Queryable documentation for all features
- **Working Example** — A real kiln demonstrating best practices
- **Test Fixture** — Integration tests verify this content works

Think of it as Crucible documenting itself, using itself.

## Maps of Content

Navigate by topic:

- **[[Meta/Product]]** — Feature map: capabilities, status, and documentation
- **[[AI Features]]** — Agents, chat, sessions, and AI-powered capabilities
- **[[Extending Crucible]]** — Plugins, hooks, tools, and scripting
- **[[Search & Discovery]]** — Finding and navigating your notes
- **[[Configuration]]** — Setting up providers and backends

## Core Concepts

Understand the fundamentals:

- **[[Help/Concepts/Agents & Protocols]]** — MCP, ACP, and how agents connect
- **[[Help/Concepts/Kilns]]** — What makes a kiln
- **[[Help/Concepts/The Knowledge Graph]]** — How wikilinks connect ideas
- **[[Help/Concepts/Semantic Search]]** — Finding by meaning
- **[[Help/Concepts/Plaintext First]]** — Why markdown matters
- **[[Help/Concepts/Scripting Languages]]** — Lua with Fennel support

## Getting Started

New to Crucible? Start here:

1. **[[Guides/Getting Started]]** — Your first steps with Crucible
2. **[[Guides/Your First Kiln]]** — Create and configure a new kiln
3. **[[Help/CLI/chat]]** — Start chatting with AI
4. **[[Help/Wikilinks]]** — The link syntax that powers everything

## Quick Start Commands

```bash
# Start an AI chat session
cru chat

# Process your notes for search
cru process

# View kiln statistics
cru stats

# Start MCP server for external agents
cru mcp
```

## Reference Documentation

### Chat & Agents
- **[[Help/CLI/chat]]** — Chat command and modes
- **[[Help/Extending/Agent Cards]]** — Configure AI agents
- **[[Help/Extending/Internal Agent]]** — Built-in agent with session memory
- **[[Help/Config/agents]]** — Agent configuration options

### Scripting Languages
- **[[Help/Concepts/Scripting Languages]]** — Overview
- **[[Help/Lua/Language Basics]]** — Lua with Fennel support
- **[[Help/Lua/Configuration]]** — Lua configuration system

### Core Features
- **[[Help/Wikilinks]]** — `[[link]]` syntax and behavior
- **[[Help/Frontmatter]]** — YAML metadata for notes
- **[[Help/Tags]]** — Organizing with `#tags`
- **[[Help/Block References]]** — Linking to paragraphs with `^block-id`

### Commands
- **[[Help/CLI/chat]]** — AI agent integration
- **[[Help/CLI/search]]** — Text and semantic search
- **[[Help/CLI/process]]** — File processing and indexing
- **[[Help/CLI/stats]]** — Kiln statistics and analysis

### Configuration
- **[[Help/Config/llm]]** — LLM provider setup
- **[[Help/Config/embedding]]** — Embedding configuration
- **[[Help/Config/storage]]** — Database and storage options

### Terminal UI
- **[[Help/TUI/Index]]** — TUI overview and shortcuts
- **[[Help/TUI/Component Architecture]]** — Widget system design

### Extending Crucible
- **[[Help/Extending/Creating Plugins]]** — Create plugins in any language
- **[[Help/Extending/Event Hooks]]** — React to kiln events
- **[[Help/Extending/MCP Gateway]]** — Connect external tools
- **[[Help/Extending/Custom Tools]]** — Add MCP tools

### Advanced Features
- **[[Help/Workflows/Index]]** — Automated multi-step processes
- **[[Help/Query/Index]]** — Advanced query language

## Example Agents

See AI agent cards in action:

- **[[Agents/Researcher]]** — Deep exploration and context gathering
- **[[Agents/Coder]]** — Code-focused analysis and generation
- **[[Agents/Reviewer]]** — Quality review and feedback

## Organization Styles

Not sure how to structure your notes?

- **[[Organization Styles/Index]]** — Overview and comparison
- **[[Organization Styles/PARA]]** — Projects, Areas, Resources, Archive
- **[[Organization Styles/Zettelkasten]]** — Atomic linked notes
- **[[Organization Styles/Johnny Decimal]]** — Numbered hierarchy

## About This Kiln

This dev-kiln is maintained as part of the Crucible project. See [[Meta/Dev Kiln Architecture]] for technical details.

---

**Ready to start?** Run `cru chat` and start a conversation.
