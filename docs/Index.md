---
title: Index
description: The dev-kiln - documentation and working example for Crucible
status: implemented
tags:
  - index
  - guide
  - welcome
order: 0
---

# Welcome to Crucible

**Crucible** is a knowledge-grounded agent runtime. Agents that draw from a knowledge graph make better decisions — memory and knowledge are too fundamental to be an afterthought. Your notes, conversations, and wikilinks form that graph. Everything beyond the knowledge core is extensible via Lua scripting and plugins.

## How It Works

### Knowledge enters

Notes, wikilinks, tags, and sessions-as-notes are how knowledge gets into the system. Write markdown, link ideas with `[[wikilinks]]`, and every conversation you have becomes a searchable, linkable note in your [[Help/Concepts/Kilns|kiln]].

- **[[Help/Concepts/Kilns]]** — What makes a kiln
- **[[Help/Concepts/The Knowledge Graph]]** — How wikilinks connect ideas
- **[[Help/Concepts/Plaintext First]]** — Why markdown matters

### Knowledge activates

[[Help/Concepts/Precognition|Precognition]] is the core expression of this philosophy: before every message reaches the LLM, Crucible searches your knowledge graph and silently injects relevant context. Your notes whisper to the agent. The more you write and link, the smarter your agents get.

- **[[Help/Concepts/Precognition]]** — Auto-RAG from your knowledge graph
- **[[Help/Concepts/Semantic Search]]** — Finding by meaning, not just keywords
- **[[Help/Concepts/Agents & Protocols]]** — MCP, ACP, and how agents connect

### You extend it

Neovim-like architecture: Lua/Fennel plugins, TUI-first, headless daemon with RPC. Most behaviors beyond the knowledge core can be scripted.

- **[[Help/Concepts/Scripting Languages]]** — Lua with Fennel support
- **[[Help/Extending/Creating Plugins]]** — Create plugins in any language
- **[[Help/Extending/MCP Gateway]]** — Connect external tools

## Getting Started

```bash
# Start an AI chat session
cru chat

# Process your notes for search
cru process

# Start MCP server for external agents
cru mcp
```

New to Crucible? Start here:

1. **[[Guides/Getting Started]]** — Your first steps with Crucible
2. **[[Guides/Your First Kiln]]** — Create and configure a new kiln
3. **[[Help/CLI/chat]]** — Start chatting with AI
4. **[[Help/Wikilinks]]** — The link syntax that powers everything

## Maps of Content

Navigate by topic:

- **[[Meta/Product]]** — Feature map: capabilities, status, and documentation
- **[[AI Features]]** — Agents, chat, sessions, and AI-powered capabilities
- **[[Extending Crucible]]** — Plugins, hooks, tools, and scripting
- **[[Search & Discovery]]** — Finding and navigating your notes
- **[[Configuration]]** — Setting up providers and backends

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

This **dev-kiln** serves as a learning environment, reference system, working example, and test fixture — Crucible documenting itself, using itself. See [[Meta/Dev Kiln Architecture]] for technical details.

---

**Ready to start?** Run `cru chat` and start a conversation.
