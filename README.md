# Crucible

[![CI](https://github.com/Mootikins/crucible/actions/workflows/ci.yml/badge.svg)](https://github.com/Mootikins/crucible/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

> An AI agent that remembers in plaintext

Crucible is a local-first AI assistant where **every conversation becomes a searchable note you own**. Chat with AI, build a knowledge graph, extend with plugins in multiple languages — all backed by markdown files in your git repo.

> **Early Development**: APIs and storage formats may change. Contributions welcome!

## What Makes Crucible Different

**Sessions as Markdown**: Conversations aren't ephemeral. Every chat session is saved as a markdown file, organized by workspace. Search across sessions. Link them together. Your AI memory lives in git.

**Knowledge as Context**: Your notes become agent memory. Use `/search` to inject relevant context, or let precognition (coming soon) find it automatically. Wikilinks define relationships. Block-level embeddings enable semantic search at paragraph granularity.

**Polyglot Plugins**: Write extensions in the language that fits:
- **Rune** — Native Rust integration, sandboxed, fastest
- **Lua** — LLM-friendly syntax, Fennel support, gradual types

**Plaintext First**: No proprietary formats. No cloud lock-in. Files are always the source of truth. The database is optional acceleration.

## Quick Start

```bash
# Build from source
git clone https://github.com/mootikins/crucible.git
cd crucible && cargo build --release

# Start a chat session
./target/release/cru chat

# Or start the MCP server for Claude/GPT integration
./target/release/cru mcp
```

**In a chat session:**
- Type naturally — the agent responds with tool access to your knowledge base
- `/search query` — Inject relevant notes into context
- `/tasks` — View current task list
- `:command` — Run REPL commands
- `!shell` — Execute shell commands

## Core Features

### Agent Chat (TUI & Web)

Interactive AI conversations with session persistence:

```
~/code/my-project $ cru chat

> Help me understand the authentication flow

[Agent searches your notes, finds relevant context...]

I found several notes about auth. Based on [[Auth Design]] and
[[API Security]], your flow uses JWT tokens issued by...
```

Sessions are saved to `~/your-kiln/sessions/my-project/2025-01-01_1430.md`.

### Knowledge Graph

Wikilinks (`[[Note Name]]`) define your knowledge graph — no extraction needed:

```markdown
# Project Architecture

The [[API Gateway]] handles auth via [[JWT Tokens]].
See [[Security Audit 2024]] for vulnerability review.
```

Query by graph traversal, semantic similarity, tags, or full-text search.

### MCP Server

Expose your knowledge base to any MCP-compatible AI:

```bash
cru mcp
```

Works with Claude Desktop, Claude Code, GPT via plugins, and local models. Tools include:
- `semantic_search` — Find notes by meaning
- `create_note` — Add to your knowledge base
- `get_outlinks` / `get_inlinks` — Traverse relationships

### Multi-Language Plugins

Define tools and hooks in your preferred language. Place plugin files in `~/.config/crucible/plugins/` or `KILN/plugins/`:

| Language | Extension | Strengths |
|----------|-----------|-----------|
| Rune | `.rn` | Native Rust integration, fastest, sandboxed |
| Lua | `.lua`, `.fnl` | Simple syntax, LLM-friendly, Fennel support |

See the [docs](./docs/Help/Concepts/Scripting%20Languages.md) for language guides.

## Architecture

```
crucible-cli        Terminal UI, REPL, commands
crucible-web        Browser chat interface (SolidJS + Axum)
crucible-tools      MCP server, tool implementations
crucible-core       Domain logic, traits, parser types
crucible-surrealdb  Storage with EAV graph schema
crucible-rune       Rune scripting runtime
crucible-lua        Lua/Luau with Fennel support
crucible-llm        Embedding backends (FastEmbed, Burn, LlamaCpp)
crucible-rig        LLM chat via Rig (Ollama, OpenAI, Anthropic)
```

LLM providers implement capability traits (`CanEmbed`, `CanChat`), letting you swap backends freely.

## Documentation

- **[docs/](./docs/)** — User guides and reference (also a working example kiln)
- **[AGENTS.md](./AGENTS.md)** — Guide for AI agents working on this codebase
- **[openspec/](./openspec/)** — Change proposals and specifications

## Roadmap

- [x] TUI chat with session persistence
- [x] MCP server for external agents
- [x] Multi-language plugin system
- [x] Block-level semantic search
- [ ] Web chat interface
- [ ] Precognition (auto-RAG before each turn)
- [ ] Session compaction and resume
- [ ] Python plugin support

## License

MIT or Apache-2.0, at your option.
