# Crucible Systems

This document defines the orthogonal systems that make up Crucible. Each system has its own spec folder and archived changes are organized by system.

## System Boundaries

| System | Scope | Crates |
|--------|-------|--------|
| **parser** | Markdown → structured data (extensions, frontmatter, blocks) | `crucible-parser`, `crucible-core/parser` |
| **storage** | Persistence: SurrealDB, EAV graph, content-addressed blocks, Merkle trees | `crucible-surrealdb`, `crucible-merkle` |
| **agents** | Agent cards, handles, LLM providers, tool registry | `crucible-core/agents`, `crucible-llm`, `crucible-tools`, `crucible-acp` |
| **workflows** | Definitions (markup) + sessions (logging, resumption, codification) | `crucible-core/workflow` (future) |
| **plugins** | Extension points, hooks, scripting (Rune on CLI, WASM on desktop) | `crucible-plugins` (future) |
| **apis** | HTTP REST, WebSocket, events, A2A protocol | `crucible-api` (future) |
| **cli** | Commands, REPL, TUI, configuration | `crucible-cli`, `crucible-config` |
| **desktop** | Tauri app, GUI | `packages/crucible-desktop` (future) |

## System Descriptions

### parser
Input processing layer. Transforms markdown notes into structured data.
- Frontmatter extraction (YAML properties)
- Block extraction (headings, paragraphs, code, etc.)
- Syntax extensions (wikilinks, tags, callouts)
- Content hashing for deduplication

### storage
Persistence layer. Stores and retrieves structured data.
- SurrealDB embedded database
- EAV (Entity-Attribute-Value) graph model
- Content-addressed block storage
- Merkle tree integrity verification
- Kiln (vault) management

### agents
AI agent infrastructure. Manages agent definitions and execution.
- Agent cards (system prompts, metadata)
- Agent handles (interface for communication)
- LLM providers (Ollama, OpenAI-compatible)
- Context management (sliding window, compaction)
- Tool registry and MCP integration

### workflows
Workflow definitions and execution logging.
- Workflow markup (DAG in markdown prose)
- Session logging (readable markdown format)
- Session resumption (continue from checkpoint)
- Workflow codification (session → definition)
- RL case generation (learn from failures)

### plugins
Extension and customization layer.
- Hook points (pre/post processing)
- Scripting runtime (Rune for CLI/server)
- Desktop plugins (WASM or native)
- Federation (bytecode over A2A)

### apis
External interfaces for programmatic access.
- HTTP REST (trigger flows, query data)
- WebSocket (real-time sync, A2A protocol)
- Event system (webhooks, triggers)
- Authentication and authorization

### cli
Command-line user interface.
- Subcommands (search, process, chat, agents, etc.)
- REPL mode (interactive queries)
- Configuration management
- Output formatting (table, JSON)

### desktop
Graphical user interface (future).
- Tauri-based desktop app
- Note editing and navigation
- Workflow visualization
- Agent chat interface

## Directory Structure

```
openspec/
├── SYSTEMS.md          # This file
├── AGENTS.md           # AI agent instructions
├── project.md          # Project-wide context
├── changes/            # Active proposals
│   └── <change-id>/
├── specs/              # Current requirements BY SYSTEM
│   ├── parser/
│   ├── storage/
│   ├── agents/
│   ├── workflows/
│   └── ...
└── archive/            # Completed changes BY SYSTEM
    ├── parser/
    │   └── YYYY-MM-DD-<change>/
    ├── agents/
    │   └── YYYY-MM-DD-<change>/
    └── ...
```

## Cross-Cutting Concerns

Some changes span multiple systems:
- **Security**: Authentication, authorization, sandboxing (touches apis, agents, plugins)
- **Observability**: Logging, metrics, tracing (touches all systems)
- **Configuration**: Unified config format (touches cli, storage, agents)

Cross-cutting changes go in `archive/cross-system/` or reference multiple specs.

## Adding New Systems

When adding a new system:
1. Add entry to this table with scope and crates
2. Create `specs/<system>/` folder
3. Create initial spec with requirements
4. Update relevant crates in Cargo.toml

## Relationship to Crates

Systems are conceptual groupings. Crates are implementation units.
- One system may span multiple crates (e.g., `agents` → `crucible-llm`, `crucible-tools`, `crucible-acp`)
- One crate may implement parts of multiple systems (e.g., `crucible-core` has parser types and agent traits)

The system boundary is about **what** (requirements), crates are about **how** (implementation).
