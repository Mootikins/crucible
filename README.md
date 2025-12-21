# Crucible

[![CI](https://github.com/Mootikins/crucible/actions/workflows/ci.yml/badge.svg)](https://github.com/Mootikins/crucible/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

> Extensible AI infrastructure for knowledge work — your data, your tools, your workflow

Crucible is an AI-powered knowledge system built on **plaintext as source of truth, infinite extensibility, and complete user control.** Your markdown files stay readable forever. AI capabilities layer on top without lock-in. Extend everything through plugins, scripts, and agents.

> **Early Development**: APIs and storage formats may change. Contributions welcome!

## Why Crucible?

**Data Sovereignty**: Your knowledge lives in markdown files you can read, edit, and move anywhere. No proprietary formats. No cloud dependency. The database is optional — files are always the source of truth.

**Extensible by Design**: Crucible exposes primitives you compose into workflows. Scripts, plugins, and AI agents all use the same extension points.

**AI as Infrastructure**: Instead of AI as a black box, Crucible provides semantic search, embeddings, and agent tools as building blocks. Use what you need. Swap providers freely. Run everything locally or connect to APIs.

## Core Capabilities

### Knowledge Graph
Wikilinks (`[[Note Name]]`) define your knowledge graph — no extraction or configuration needed. Block-level embeddings enable semantic search at paragraph granularity. Combine graph traversal, tags, and fuzzy matching for precise discovery.

### Agent Integration
Built-in MCP (Model Context Protocol) server exposes your knowledge base to AI agents. Create notes, search semantically, traverse relationships — all through a standard protocol that works with Claude, GPT, and local models.

### Extension System
Rune scripting for sandboxed automation. Pluggable LLM providers (Ollama, OpenAI, FastEmbed, LlamaCpp). Query language (tq) for structured data manipulation. Every layer designed for composition.

## Quick Start

```bash
git clone https://github.com/mootikins/crucible.git
cd crucible
cargo build --release

# Start the CLI
cru

# Or start the MCP server for agent integration
cru mcp
```

**Windows**: See [Windows Configuration Guide](docs/WINDOWS-CONFIGURATION.md) for platform-specific setup.

## Architecture

Crucible separates concerns into composable crates:

- **Core** — Domain logic, traits, parser types
- **Storage** — SurrealDB with EAV graph schema (optional)
- **LLM** — Unified provider system for embeddings and chat
- **Tools** — MCP server and tool implementations
- **CLI/Web** — User interfaces

LLM providers implement capability traits (`CanEmbed`, `CanChat`, `CanConstrainGeneration`), letting you swap backends without changing application code.

## Documentation

- [AGENTS.md](./AGENTS.md) — Guide for AI agents working on the codebase
- [OpenSpec](./openspec/) — Change proposals and system specifications
- [Dev Kiln](./examples/dev-kiln/) — Documentation vault and test fixture

## License

MIT or Apache-2.0, at your option.
