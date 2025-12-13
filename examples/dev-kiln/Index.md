---
title: Welcome to Crucible
description: The dev-kiln - your guide to Crucible's knowledge management system
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

## Core Concepts

Crucible is built on simple principles:

- **Wikilinks Define the Graph** - `[[Note Name]]` links create your knowledge structure
- **Plaintext is Source of Truth** - Works with any text editor, no vendor lock-in
- **Block-Level Granularity** - Search and embed at paragraph/heading level
- **Local-First** - Everything stays on your machine
- **Agent-Ready** - Built for AI collaboration via MCP and ACP protocols

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

### Extending Crucible
- **[[Help/Extending/Writing Plugins]]** - Create plugins with Rune
- **[[Help/Extending/Custom Tools]]** - Add MCP tools
- **[[Help/Extending/Agent Cards]]** - Configure AI agents
- **[[Help/Extending/Workflow Authoring]]** - Build workflows

## Example Agents

See AI agent cards in action:

- **[[Agents/Researcher]]** - Deep exploration and context gathering
- **[[Agents/Coder]]** - Code-focused analysis and generation
- **[[Agents/Reviewer]]** - Quality review and feedback

## Quick Start Commands

Get productive immediately:

```bash
# Process your notes and start exploring
cru

# Search for content
cru search "knowledge graph"

# Find semantically similar notes
cru semantic "AI agents"

# Start an AI chat session
cru chat

# View kiln statistics
cru stats
```

## About This Kiln

This dev-kiln is maintained as part of the Crucible project. See [[Meta/Dev Kiln Architecture]] for technical details about how this kiln is structured and tested.

---

**Ready to start?** Head to **[[Guides/Getting Started]]** and build your first query.
