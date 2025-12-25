---
description: The dev-kiln - your guide to Crucible's knowledge management system
status: implemented
tags:
  - index
  - guide
  - welcome
order: 0
---

# Welcome to Crucible

**Crucible** is a plaintext-first knowledge management system that turns your markdown files into a powerful knowledge graph. By combining wikilinks, semantic search, and AI agent integration, Crucible helps you discover connections and context across your notes.

## What is This?

This **dev-kiln** serves as:

- **Learning Environment** - Hands-on tutorials and guides to get you started
- **Reference System** - Queryable documentation for all Crucible features
- **Working Example** - A real kiln demonstrating best practices
- **Test Fixture** - Integration tests verify this content works correctly

Think of it as Crucible documenting itself, using itself.

## Maps of Content

Navigate by topic:

- **[[Extending Crucible]]** - Plugins, hooks, tools, and customization
- **[[Search & Discovery]]** - Finding and navigating your notes
- **[[AI Features]]** - Agents, chat, and AI-powered capabilities
- **[[Configuration]]** - Setting up providers and backends

## Core Concepts

Understand the fundamentals:

- **[[Help/Concepts/Kilns]]** - What makes a kiln
- **[[Help/Concepts/The Knowledge Graph]]** - How wikilinks connect ideas
- **[[Help/Concepts/Semantic Search]]** - Finding by meaning
- **[[Help/Concepts/Plaintext First]]** - Why markdown matters
- **[[Help/Concepts/Agents & Protocols]]** - MCP and ACP explained

## Getting Started

New to Crucible? Start here:

1. **[[Guides/Getting Started]]** - Your first steps with Crucible
2. **[[Guides/Your First Kiln]]** - Create and configure a new kiln
3. **[[Guides/Basic Commands]]** - Essential CLI commands to know
4. **[[Help/Wikilinks]]** - Understanding the link syntax that powers everything

## Organization Styles

Not sure how to structure your notes? Explore proven approaches:

- **[[Organization Styles/Index]]** - Overview and comparison of methods
- **[[Organization Styles/PARA]]** - Projects, Areas, Resources, Archive
- **[[Organization Styles/Zettelkasten]]** - Atomic linked notes for deep thinking
- **[[Organization Styles/Johnny Decimal]]** - Numbered hierarchical organization
- **[[Organization Styles/Choosing Your Structure]]** - Find what works for you

## Reference Documentation

Quick help on specific features:

### Core Features
- **[[Help/Wikilinks]]** - `[[link]]` syntax and behavior
- **[[Help/Frontmatter]]** - YAML metadata for notes
- **[[Help/Tags]]** - Organizing with `#tags`
- **[[Help/Block References]]** - Linking to specific paragraphs with `^block-id`

### Commands
- **[[Help/CLI/search]]** - Text and semantic search
- **[[Help/CLI/process]]** - File processing and indexing
- **[[Help/CLI/chat]]** - AI agent integration
- **[[Help/CLI/stats]]** - Kiln statistics and analysis

### Configuration
- **[[Help/Config/llm]]** - LLM provider setup
- **[[Help/Config/embedding]]** - Embedding configuration
- **[[Help/Config/storage]]** - Database and storage options
- **[[Help/Config/agents]]** - Agent card configuration

### Terminal UI
- **[[Help/TUI/Index]]** - TUI overview and shortcuts
- **[[Help/TUI/Component Architecture]]** - Widget system design
- **[[Help/TUI/Rune API]]** - Scripting the TUI (planned)

### Extending Crucible
- **[[Help/Extending/Creating Plugins]]** - Create plugins with Rune
- **[[Help/Extending/Event Hooks]]** - React to kiln events
- **[[Help/Extending/MCP Gateway]]** - Connect external tools
- **[[Help/Extending/Custom Tools]]** - Add MCP tools
- **[[Help/Extending/Agent Cards]]** - Configure AI agents
- **[[Help/Extending/Workflow Authoring]]** - Build workflows

### Scripting with Rune
- **[[Help/Rune/Language Basics]]** - Rune syntax fundamentals
- **[[Help/Rune/Crucible API]]** - Available functions
- **[[Help/Rune/Best Practices]]** - Writing good plugins

### Advanced Features
- **[[Help/Workflows/Index]]** - Automated multi-step processes
- **[[Help/Query/Index]]** - Advanced query language

## Example Agents

See AI agent cards in action:

- **[[Agents/Researcher]]** - Deep exploration and context gathering
- **[[Agents/Coder]]** - Code-focused analysis and generation
- **[[Agents/Reviewer]]** - Quality review and feedback

## Quick Start Commands

Get productive immediately:

```bash
# Process your notes
cru process

# Start an AI chat session
cru chat

# View kiln statistics
cru stats

# Check storage status
cru status

# Start MCP server for external tools
cru mcp
```

## About This Kiln

This dev-kiln is maintained as part of the Crucible project. See [[Meta/Dev Kiln Architecture]] for technical details about how this kiln is structured and tested.

---

**Ready to start?** Head to **[[Guides/Getting Started]]** and build your first query.
