# Crucible

[![CI](https://github.com/Mootikins/crucible/actions/workflows/ci.yml/badge.svg)](https://github.com/Mootikins/crucible/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

> An AI agent that remembers in plaintext

Crucible is a local-first AI assistant where **every conversation becomes a searchable note you own**. Chat with AI, build a knowledge graph, extend with plugins in multiple languages — all backed by markdown files in your git repo.

> **Early Development**: APIs and storage formats may change. Contributions welcome!

## What Makes Crucible Different

**Sessions as Markdown**: Conversations aren't ephemeral. Every chat session is saved as a markdown file, organized by workspace. Search across sessions. Link them together. Your AI memory lives in git.

**Knowledge as Context**: Your notes become agent memory. Precognition automatically injects relevant vault context before each agent turn, or use `/search` for manual control. Wikilinks define relationships. Block-level embeddings enable semantic search at paragraph granularity.

**Extensible Plugins**: Write extensions in Lua with Fennel support:
- **Lua** — LLM-friendly syntax, simple semantics, gradual types
- **Fennel** — Lisp syntax compiling to Lua, macros included

**Plaintext First**: No proprietary formats. No cloud lock-in. Files are always the source of truth. The database is optional acceleration.

## Quick Start

```bash
# Install from source
git clone https://github.com/mootikins/crucible.git
cd crucible && cargo build --release

# Or install directly via Cargo
cargo install --git https://github.com/Mootikins/crucible.git crucible-cli

# Start a chat session (first run triggers setup wizard)
cru chat

# Or start the MCP server for Claude/GPT integration
cru mcp
```

On first run, `cru chat` launches a setup wizard that walks you through kiln path selection, provider detection, and model configuration. A background daemon auto-spawns to handle session persistence and file watching.

**In a chat session:**
- Type naturally — the agent responds with tool access to your knowledge base
- `/search query` — Inject relevant notes into context
- `/tasks` — View current task list
- `:command` — Run REPL commands (`:model`, `:set`, `:export`, `:help`)
- `!shell` — Execute shell commands
- `BackTab` — Cycle modes: Normal → Plan → Auto

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

### Lua Plugins

Define tools and event handlers in Lua. Place plugin files in `~/.config/crucible/plugins/` or `KILN/plugins/`:

| Extension | Language | Use Case |
|-----------|----------|----------|
| `.lua` | Lua | Tools, handlers, automation |
| `.fnl` | Fennel | Lisp syntax, macros, DSLs |

```lua
--- Search and summarize notes
-- @tool name="summarize" description="Summarize notes matching query"
-- @param query string "Search query"
function summarize(args)
    local results = crucible.search(args.query)
    return { summary = "Found " .. #results .. " notes" }
end
```

See the [docs](./docs/Help/Concepts/Scripting%20Languages.md) for the full plugin guide.

## Architecture

```
crucible-cli             Terminal UI, REPL, commands
crucible-daemon          Background server (auto-spawned, Unix socket)
crucible-rpc             RPC client library with auto-reconnect
crucible-tools           MCP server, tool implementations
crucible-core            Domain logic, traits, parser types
crucible-surrealdb       Storage with EAV graph schema
crucible-lua             Lua/Luau with Fennel support
crucible-llm             Embedding backends (FastEmbed, Burn, LlamaCpp)
crucible-rig             LLM chat via Rig (Ollama, OpenAI, Anthropic)
crucible-acp             Agent Context Protocol (host + gateway)
```

The daemon (`cru-server`) auto-spawns on first use and handles session persistence, file watching, event streaming, and multi-client coordination over a Unix socket. LLM providers implement capability traits (`CanEmbed`, `CanChat`), letting you swap backends freely.

## Documentation

- **[docs/](./docs/)** — User guides and reference (also a working example kiln)
- **[AGENTS.md](./AGENTS.md)** — Guide for AI agents working on this codebase
- **[openspec/](./openspec/)** — Change proposals and specifications

## Roadmap

- [x] TUI chat with session persistence and resume
- [x] MCP server for external agents
- [x] Lua/Fennel plugin system with 17+ API modules
- [x] Block-level semantic search with reranking
- [x] Precognition (auto-RAG before each turn)
- [x] ACP host for spawning external AI agents
- [x] Permission system with pattern whitelisting
- [x] Daemon with auto-spawn, file watching, and multi-session support
- [ ] Web chat interface
- [ ] Lua session primitives (fork, inject, collect)
- [ ] ACP agent mode (embeddable in Zed, JetBrains, Neovim)

## License

MIT or Apache-2.0, at your option.
