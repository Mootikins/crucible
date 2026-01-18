---
description: Vision, milestones, and feature planning for Crucible development
type: roadmap
status: active
updated: 2024-12-13
tags:
  - meta
  - planning
  - vision
---

# Crucible Roadmap

## Vision

**"Neovim for agents+notes"** - extensible, open, documented

A knowledge management system where:
- AI agents have perfect context from your vault
- Workflows are defined in markdown and executed by agents
- Everything is extensible via Rune scripting and hooks
- Power users get CLI, everyone else gets web/desktop UI eventually

## Killer Workflows

1. **AI agent with perfect vault context** - Claude/agents find relevant notes automatically, help you think with full context
2. **Workflow automation in markdown** - Define workflows as prose, agents execute them, progress logged as reviewable notes

## User Progression

| Phase | Users | Interface |
|-------|-------|-----------|
| Now | Power users, developers | CLI (chat-focused) |
| Next | Plugin creators, agent developers | CLI + Rune scripting |
| Later | Non-technical users | Web UI, Tauri desktop |

---

## Phase 0: Core Foundation (Current)

*Must work before anything else*

### Finish In-Progress

- [ ] **MCP Tool System** (95%) - Permission prompts, ACP integration
- [ ] **MCP Bridge/Gateway** (85%) - Integration tests, documentation
- [x] **Rune programmatic tool calling** - Tool discovery (`discover_tools`, `get_tool_schema`) complete
- [x] **Event/Hook System** - Note lifecycle hooks (`note:created`, `note:modified`, `note:deleted`) complete

### Polish & Stability (Dogfooding Focus)

- [ ] **Error handling UX** - Clear error messages, graceful degradation when services unavailable
- [ ] **Session persistence** - Reliable save/resume, handle crashes gracefully
- [ ] **Plugin loading errors** - Clear feedback when Rune/Lua plugins fail to load
- [ ] **MCP connection stability** - Reconnect logic, timeout handling, status indicators
- [ ] **CLI help & discoverability** - `--help` completeness, command suggestions

### Maintain

- [x] Parser + Storage - Stable
- [x] ACP Integration - Working (enables Cursor, external agents)
- [x] Chat CLI - Primary interface
- [x] Embeddings - Semantic search working
- [x] Query System - Context enrichment for agents (see [[Help/Query/Query System]])
- [x] TUI E2E Testing - expectrl-based test harness for PTY testing (see [[Help/TUI/E2E Testing]])
- [x] **Ink TUI Migration** - Removed ratatui, ink is now the sole TUI renderer

---

## Phase 1: Extensibility Complete

*The "Neovim-like" extension story*

### High Priority

- [ ] **Rune Integration (full)** - Complete scripting API for custom workflows, agent behaviors, callout handlers
- [ ] **Internal Agent System** - Direct LLM usage (Ollama, OpenAI) without ACP dependency
- [ ] **Grammar + Rune integration** - Constrained generation for specific flows

### Medium Priority

- [ ] **TUI Redesign** - Streaming UX, splash screen, bottom-anchored chat (see [[TUI User Stories]])
- [ ] **Chat Improvements** - File references (`@file`), command history, session stats
- [ ] **Hook documentation** - How to extend Crucible guide

---

## Phase 2: Workflow Automation

*Killer workflow #2: workflows defined in markdown*

### High Priority

- [ ] **Markdown Handlers** - Event handlers defined in pure markdown, inject context into agents (see [[Help/Extending/Markdown Handlers]])
- [ ] **Workflow Markup** - DAG workflows in markdown prose (`@agent`, `->` data flow, `> [!gate]` approvals) (see [[Help/Workflows/Workflow Syntax]])
- [ ] **Workflow Sessions** - Log execution as markdown, resume interrupted work

### Medium Priority

- [ ] **Session learning** - Codify successful sessions into reusable workflows
- [ ] **Parallel execution** - `(parallel)` suffix or `&` prefix for concurrent steps (deferred)

---

## Phase 3: Polish & Rich Features

*Better UX, preparing for non-technical users*

### Web/Desktop UI (Tauri + Web)

- [ ] **Browser UI** - Svelte-based chat interface via `cru serve`
- [ ] **Tauri Desktop** - Native app wrapping web UI
- [ ] **Canvas/Flowcharts** - WebGL-based visual workflows
- [ ] **Rich rendering** - Mermaid diagrams, LaTeX, image OCR
- [ ] **Document preview** - PDF, image rendering in notes

### Note Features

- [ ] **Note Types** - Templates and typed notes (book, meeting, movie)

---

## Phase 4: Scale & Collaborate

*Multi-device, multi-user, federation*

### Deferred (revisit when core is solid)

- [ ] **Sync System** - Merkle diff + CRDT for multi-device
- [ ] **Session Daemon** - Concurrent agent access to kiln
- [ ] **Shared Memory** - Worlds/Rooms for collaborative cognition
- [ ] **Federation** - A2A protocol for cross-vault agents

---

## Backlog

| Item | Notes |
|------|-------|
| Rune/Core event unification | Merge rune EventBus with core SessionEvent system into single event architecture |
| Remove remaining unused deps | `cargo machete` shows unused deps in other crates (core, surrealdb, tools, etc.) |

---

## Archived / Cut

| Item | Reason |
|------|--------|
| `crucible-desktop` (GPUI) | Cut - using Tauri + web instead |
| `add-desktop-ui` OpenSpec | Archived - GPUI approach abandoned |
| `add-meta-systems` | Too ambitious (365 tasks), overlaps with focused Rune approach |
| `add-advanced-tool-architecture` | Overlaps with working MCP bridge |
| `add-quick-prompt-features` | Nice UX, not core - revisit in Phase 3 |
| `refactor-clustering-plugins` | Nice feature, not core |
| Ratatui TUI | Removed - migrated to ink-only TUI (2025-01-17) |

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2024-12-13 | Cut GPUI desktop, keep Tauri | Web tech enables rich features (canvas, mermaid, latex) at low cost |
| 2024-12-13 | Keep ACP | Working, enables Cursor-specific models |
| 2024-12-13 | Event hooks = Tool + Note lifecycle | Focused scope vs. 317-task proposal |
| 2024-12-13 | Keep grammar crate | Integrate with Rune for constrained generation |
| 2024-12-13 | CLI is chat-focused | Other commands for testing, primary UX is conversation |

---

## Links

- [[Dev Kiln Architecture]] - System architecture
- [[TUI User Stories]] - Chat interface requirements
- [[Plugin User Stories]] - Extension system requirements
- [[Meta/Systems]] - System boundaries
- [[Help/Workflows/Workflow Syntax]] - Workflow syntax reference
- [[Help/Extending/Markdown Handlers]] - Handler syntax reference
- [[Help/Query/Query System]] - Query system reference
