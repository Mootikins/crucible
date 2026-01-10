---
title: Crucible Systems
description: Orthogonal systems that make up Crucible architecture
tags:
  - meta
  - architecture
  - systems
---

# Crucible Systems

This document defines the orthogonal systems that make up Crucible. Each system has clear boundaries and responsibilities.

## System Boundaries

| System | Scope | Crates |
|--------|-------|--------|
| **parser** | Markdown → structured data (extensions, frontmatter, blocks) | `crucible-parser`, `crucible-core/parser` |
| **storage** | Persistence: SQLite (default), SurrealDB (advanced) | `crucible-sqlite`, `crucible-surrealdb` |
| **sync** | Merkle-CRDT sync across devices, collaborators, and federated agents | `crucible-sync` (future) |
| **agents** | Agent cards, handles, LLM providers, tool registry | `crucible-core/agents`, `crucible-llm`, `crucible-tools`, `crucible-acp` |
| **workflows** | Definitions (markup) + sessions (logging, resumption) | `crucible-core/workflow` (future) |
| **plugins** | Extension points, hooks, scripting (Rune) | `crucible-rune`, `crucible-lua` |
| **apis** | HTTP REST, WebSocket, events | `crucible-web` |
| **cli** | Commands, REPL, TUI, configuration | `crucible-cli`, `crucible-config` |

## System Descriptions

### parser

Input processing layer. Transforms markdown notes into structured data.

- Frontmatter extraction (YAML properties)
- Block extraction (headings, paragraphs, code, etc.)
- Syntax extensions (wikilinks, tags, callouts)
- Content hashing for deduplication

See: [[Help/Concepts/The Knowledge Graph]]

### storage

Persistence layer. Stores and retrieves structured data.

- SurrealDB embedded database
- EAV (Entity-Attribute-Value) graph model
- Content-addressed block storage
- Merkle tree integrity verification
- Kiln (vault) management

See: [[Help/Concepts/Kilns]]

### sync

Synchronization across boundaries. Enables conflict-free collaboration.

- Merkle-CRDT protocol (compare roots, sync divergent blocks)
- Three localities: local (multi-device), coordinated (collaboration), federated
- CRDT types: Loro for text, LWW for metadata, OR-Set for tags

*Status: Planned (Phase 4)*

### agents

AI agent infrastructure. Manages agent definitions and execution.

- Agent cards (system prompts, metadata)
- Agent handles (interface for communication)
- LLM providers (Ollama, OpenAI-compatible)
- Context management (sliding window, compaction)
- Tool registry and MCP integration

See: [[Help/Concepts/Agents & Protocols]], [[Help/Extending/Internal Agent]]

### workflows

Workflow definitions and execution logging.

- Workflow markup (DAG in markdown prose)
- Session logging (readable markdown format)
- Session resumption (continue from checkpoint)

See: [[Help/Workflows/Workflow Syntax]]

*Status: Planned (Phase 2)*

### plugins

Extension and customization layer.

- Hook points (pre/post processing)
- Scripting runtime (Rune, Lua)
- Event handlers in markdown

See: [[Help/Extending/Event Hooks]], [[Help/Extending/Markdown Handlers]]

### apis

External interfaces for programmatic access.

- HTTP REST (query data, trigger actions)
- Server-Sent Events (streaming responses)
- MCP server for external tools

See: [[Help/Extending/MCP Gateway]]

### cli

Command-line user interface.

- Subcommands (search, process, chat, agents, etc.)
- TUI chat interface
- Configuration management
- Output formatting (table, JSON)

See: [[Help/CLI/Commands]]

## Cross-Cutting Concerns

Some changes span multiple systems:

- **Security**: Authentication, authorization, sandboxing (touches apis, agents, plugins)
- **Observability**: Logging, metrics, tracing (touches all systems)
- **Configuration**: Unified config format (touches cli, storage, agents)

## Relationship to Crates

Systems are conceptual groupings. Crates are implementation units.

- One system may span multiple crates (e.g., `agents` → `crucible-llm`, `crucible-tools`, `crucible-acp`)
- One crate may implement parts of multiple systems (e.g., `crucible-core` has parser types and agent traits)

The system boundary is about **what** (requirements), crates are about **how** (implementation).

## Related

- [[Meta/Roadmap]] - Development timeline
- [[Dev Kiln Architecture]] - Technical architecture
