---
title: Roadmap
description: Phase-based development timeline and dependency-ordered execution plan
type: roadmap
status: active
updated: 2026-05-07
tags:
  - meta
  - planning
  - vision
---

# Crucible Roadmap

> Two views of the same plan. **Phases** are how we communicate the strategic arc. **Waves** are how we sequence the actual work — a topological sort of remaining items by dependency.
>
> Feature status (`[x]` shipped · `[-]` in progress · `[ ]` planned) lives in [[Meta/Product]]. This document tracks the *order* and *dependencies*.

## Vision

A knowledge-grounded agent runtime — agents that draw from a knowledge graph make better decisions. Neovim-like architecture: Lua extensibility, TUI-first, headless daemon with RPC, plugin-driven. See [[Meta/Product#Vision]].

## User Progression

| Phase | Users | Interface |
|-------|-------|-----------|
| Now | Power users, developers | CLI (chat-focused) |
| Next | Plugin creators, agent developers | CLI + Lua scripting + messaging integrations |
| Later | Broader audience, mobile users | Web PWA (self-hosted via Tailscale/Cloudflare Tunnel) |

---

## Phase View (strategic)

| Phase | Focus | Maps to waves |
|-------|-------|---------------|
| **0 — Core Foundation** | Stability, error handling, session persistence, MCP completion | Wave 0 |
| **1 — Extensibility Complete** | Lua scripting API, agent learning, TUI redesign, ACP agent mode, HTTP gateway, web foundation | Waves 1–7 |
| **2 — Workflow Automation** | Markdown-defined workflows, DAG execution, session learning, rich web | Waves 3, 8, 9 |
| **3 — Polish & Rich Features** | Web UI polish, Tauri desktop, rich rendering, note types | Wave 10 |
| **4 — Scale & Collaborate** | Multi-device sync, federation, collaborative cognition | Wave 11 |

> The `crucible-web` crate (SolidJS + Axum) already provides an early web chat interface; HTTP→RPC bridge, SSE, chat/search/webhook APIs, and auth all shipped. Phase 1+2 work continues iteratively rather than waiting for a single big-bang release.

---

## Wave View (execution order)

Waves are dependency-ordered. **Items inside a wave can be parallelized**; later waves depend on earlier ones. Wave numbers are not phase numbers — they reflect the topological sort of what's currently incomplete.

### Wave 0 — Phase 0 finishers (immediate next)

> Mostly wiring: types and RPC plumbing already exist; we need to call them from the right places.

- **Validate-Retry Loop** — `validate_output()` exists in core but is never called; wire into `messaging.rs` agent loop with `validation_retries` honored on failure
- **`Summarize` Context Strategy** — `enforce_context_budget()` only handles `Truncate` and `SlidingWindow`; add LLM-summarize-older-turns variant
- **Token Budget Tracking** — char/4 heuristic estimate; auto-compact threshold configurable via `:set autocompact_threshold` (default 0.95, range 0.0–1.0; `0` or `off` disables)
- **Cache Stats** — `cru.session.cache_stats()` Lua binding + statusline hit-rate display (event payload already carries `cache_read_tokens` / `cache_creation_tokens`)
- **CLI Help & Discoverability** — `--help` audit, command suggestions on typos

### Wave 1 — Lua API closure (depends on Wave 0 types)

- **Lua Context Operations** — bind `crucible-core/src/traits/context_ops` to `cru.context.{usage, compact, messages, remove, estimate_tokens}`
- **Lua Validators** — depends on Wave 0 validate-retry; `cru.session.set("output_validation", { type = "lua", fn = ... })`
- **Turn Undo + Undo Lua API** — wire existing `UndoTree<T>` + git stash; `/undo` slash command + `cru.session.{undo,redo,can_undo,undo_history}`
- **`session.fork()`** — branching state for A/B exploration
- **LuaCATS Type Stubs** — generate `---@meta` from Rust API surface; ship to `~/.config/crucible/luals/`. Land last in this wave once the Lua surface is stable.

### Wave 2 — Agent learning & teams (depends on Wave 1)

> `cru.tools.call`, `cru.tools.batch`, and `cru.sessions.messages/inject/collect_subagents` all shipped — these enable runtime plugins to do real work.

- **`entity-memory` runtime plugin** — facts → atomic notes via `turn:complete` hook; deduplicates against existing entity notes via `semantic_search`
- **`session-digest` runtime plugin** — completed sessions → linked notes; captures decisions, topics, entities
- **Memory Scoping** — per-user / workspace / global tagging; precognition filters by active scope. Depends on entity-memory tagging convention.
- **Team Patterns** (core Rust) — supervisor / router / broadcast on existing subagent infrastructure
- **Grammar + Lua Integration** — constrained generation for structured agent outputs (parallel; weak dep on type stubs)

### Wave 3 — Workflows Phase 2 (parser + engine landed; finish out)

> `WorkflowDoc` parser, execution engine, daemon RPC, CLI, inline LLM, completion assessment, and resumability all shipped per recent commits. Remaining work is the surrounding ergonomics.

- **Workflow Sessions** — markdown execution log + resume from interruption
- **Markdown Handlers** — event handlers in pure markdown, inject context into agents
- **Parallel Execution** — `(parallel)` suffix / `&` prefix semantics
- **Session Learning** — codify successful sessions → reusable workflows. Depends on `session-digest` (Wave 2) and Markdown Handlers.
- **Workflow Authoring guide** — docs only

### Wave 4 — Web/HTTP foundation primitives (gateway shipped; rendering primitives next)

> HTTP→RPC bridge, SSE, chat/search/webhook APIs, and auth are in. The remaining foundation is rendering.

- **Static File Serving** — SolidJS bundle + PWA manifest + service worker
- **Oil Node Serialization** — `impl Serialize for Node`; foundational primitive everything else composes on
- **SolidJS Oil Renderer** — `<OilNode>` component tree (depends on Oil Node Serialization)
- **Plugin Panel Hosting** — iframe sandbox + message-passing protocol (depends on Oil Node Serialization)
- **PWA Support** — depends on Static File Serving

### Wave 5 — Web UI core (depends on Wave 4)

> Several items already `[-]` in progress (Web Chat UI, panels, breadcrumbs, file tree, CodeMirror, model picker, auto-naming).

- **Finish Web Chat UI in-progress items**
- **Agent Inbox / Overview** — landing page composing `session.list` + event subscriptions; the multi-agent command center
- **Permission Approval UI** — Rust-owned approval RPC, Lua-owned diff/policy hooks

### Wave 6 — Distribution edges (depends on Wave 4 HTTP gateway)

- **Telegram Bot** — Bot API adapter over HTTP gateway (lowest friction)
- **Matrix Bridge** — second adapter to validate trait shape
- **`cru.messaging` adapter** — extract trait from Telegram + Discord (already shipped) + Matrix
- **`cru tunnel`** + **Cloudflare/Tailscale Funnel** integrations
- **Plugin Install** — `cru plugin add <git-url>` (lazy.nvim model)
- **Agent Memory Branding** — rename Precognition; pure docs

### Wave 7 — ACP agent mode (independent track)

- **ACP Schema Bump** — 0.10.6 → 0.10.7
- **ACP Agent Mode** — Crucible as embeddable ACP agent (depends on schema bump)
- **ACP Registry Submission** — depends on agent mode

### Wave 8 — Proactive features (depends on Waves 2 + 6)

- **Kiln Digest** — periodic scan, surface missed connections; uses entity-memory + messaging
- **Daily Briefing Plugin** — reference plugin building on Kiln Digest

### Wave 9 — P2 web rich features (depends on Wave 5)

Knowledge Graph Visualization · Note Browser · Search UI · Agent Artifacts · Skills Browser · Rich Content Renderers · Config Editor · OpenAPI Spec · System Info · Log Viewer · `cru share` · K-Means Clustering

### Wave 10 — P3 polish

Note Types · Structured Data Views · Canvas · Workflow Visual Editor · Tauri Desktop

### Wave 11 — P4 scale

Sync (Merkle/CRDT) · Concurrent Agent Access · Shared Memory · Federation

---

## Critical Path

The shortest route through unfinished P0/P1 work:

```
Wave 0  →  Wave 1  →  Wave 2
(finish    (Lua       (agent
 Phase 0)   surface)   learning)
```

After Wave 2, three independent tracks unlock and can be worked in parallel:

```
                           ┌─→ Wave 3 (workflows)  ─→ Wave 8 (proactive)
Wave 2 (agent learning) ───┼─→ Wave 4 → 5 (web)    ─→ Wave 9 (rich web)
                           ├─→ Wave 6 (distribution) ┘
                           └─→ Wave 7 (ACP agent — fully independent, slot in opportunistically)
```

Wave 7 (ACP agent mode) has no dependencies on the others — it's a small lift good for unlocking registry distribution.

---

## Backlog

| Item | Notes |
|------|-------|
| Session compaction with cache purge | When compacting, purge ViewportCache `graduated_ids` for pre-compaction content. Memory scales with model context length, not full session history. |
| Remove remaining unused deps | `cargo machete` shows unused deps in core, surrealdb, tools, etc. |

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2024-12-13 | Cut GPUI desktop, keep Tauri | Web tech enables rich features (canvas, mermaid, latex) at low cost |
| 2024-12-13 | Keep ACP | Working, enables Cursor-specific models |
| 2024-12-13 | Event hooks = Tool + Note lifecycle | Focused scope vs. 317-task proposal |
| 2024-12-13 | Keep grammar crate | Integrate with Lua for constrained generation |
| 2024-12-13 | CLI is chat-focused | Other commands for testing, primary UX is conversation |
| 2025-01-23 | Oil UI DSL: Lua + Fennel | React-like functional UI, web deferred to Phase 3 |
| 2025-01-23 | Web is SolidJS, not Svelte | Corrected docs; SolidJS already in use |
| 2025-01-24 | Vim-style :set command | Runtime config overlay with modification tracking |
| 2025-01-24 | Model switching | Runtime model changes via :model command and daemon RPC |
| 2026-05-07 | Roadmap split into Phase + Wave views | Phases communicate strategy; waves sequence execution. Topological sort of remaining work resolves dependency confusion when multiple tracks proceed in parallel. |

---

## Archived / Cut

| Item | Reason |
|------|--------|
| `crucible-desktop` (GPUI) | Cut — using Tauri + web instead |
| `add-desktop-ui` OpenSpec | Archived — GPUI approach abandoned |
| `add-meta-systems` | Too ambitious (365 tasks), overlaps with focused Lua approach |
| `add-advanced-tool-architecture` | Overlaps with working MCP bridge |
| `add-quick-prompt-features` | Nice UX, not core — revisit in Phase 3 |
| `refactor-clustering-plugins` | Nice feature, not core |
| Ratatui TUI | Removed — migrated to oil-only TUI (2025-01-17) |
| SurrealDB Backend | Removed — SQLite is default and only backend |
| hermit plugin | Removed (2026-03-29) — capabilities belong in chat/messaging integration plugins |

---

## Links

- [[Meta/Product]] — Feature inventory with status and documentation links
- [[Meta/Systems]] — System architecture and boundaries
- [[Meta/TUI User Stories]] — Chat interface requirements
- [[Meta/Plugin User Stories]] — Plugin requirements
- [[Help/Workflows/Workflow Syntax]] — Workflow syntax reference
- [[Help/Extending/Markdown Handlers]] — Handler syntax reference
- [[Help/Query/Query System]] — Query system reference
