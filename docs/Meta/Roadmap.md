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

### Wave 0 — Phase 0 finishers ✅ fully shipped 2026-05-07

> Mostly wiring: types and RPC plumbing already existed; calling code now lives. All five items plus the two follow-ups (cache_stats Lua/statusline and LLM-driven Summarize) shipped same day.

- ✅ **Validate-Retry Loop** — `validate_output()` runs after every turn in `execute_agent_stream`; failures inject a regenerate-prompt and re-enter the stream up to `validation_retries`; exhaustion emits `ended("error: output validation exhausted retries")`.
- ✅ **`Summarize` Context Strategy** — `enforce_context_budget` returns a `BudgetAction::NeedsSummarize` carrying the drained messages; `stream_chat_from_messages` calls `summarize_via_backend` (same genai client as the agent) and replaces the placeholder with the LLM-generated recap. On error or empty response, the static `[summary placeholder]` survives as fallback.
- ✅ **Token Budget Tracking** — `estimate_tokens` / `estimate_messages_tokens` helpers in `crucible_core::traits::context_ops`; auto-compact threshold configurable via `:set autocompact_threshold` (default 0.95, range 0.0–1.0; `0` or `off` disables); `should_autocompact` triggers `SessionManager::request_compaction` from `run_reactor_handlers`.
- ✅ **Cache Stats** — `CacheStats` aggregate + `session.cache_stats` RPC; `cru.sessions.cache_stats(id)` Lua binding via `DaemonSessionApi`; statusline `cache_hit_rate` component fed from `message_complete` cache token fields.
- ✅ **CLI Help & Discoverability** — `infer_subcommands = true`; clap 4 typo suggestions; insta snapshots lock `cru --help` and the two most-used subcommands.

### Wave 1 — Lua API closure ✅ fully shipped 2026-05-08

> Lua surface for context manipulation, output validation, and turn undo. Fork and LuaCATS landed earlier; the remaining three closed out this wave.

- ✅ **Lua Context Operations** — `cru.context.{usage, compact, messages, remove, estimate_tokens}` bind to `crucible-core::traits::context_ops` via `DaemonSessionApi`. `remove` mirrors `undo_turns` semantics on the branchable `ConversationTree` (cursor rewind, not slice deletion).
- ✅ **Lua Validators** — `OutputValidation::Lua { name }` variant; daemon plugin loader exposes `cru.context.register_validator(name, fn)`; stream loop calls `LuaValidatorRegistry::run` against the loader's `Arc<Lua>` (mlua `send` feature). Lua-side ergonomic: `cru.sessions.set_output_validation(id, { type = "lua", name = "..." })`.
- ✅ **Turn Undo + Undo Lua API** — `cru.sessions.{undo, can_undo, undo_depth, undo_history}` bridge to `AgentManager`. File rollback via `WorkspaceSnapshot` (git: `write-tree`+`commit-tree` to capture untracked files; non-git: in-memory journal capped at 5MiB). Side-map keyed by `(session_id, NodeId)` keeps `crucible-core` types untouched. `redo` deferred — no `redo_turns` analogue exists yet.
- ✅ **`session.fork()`** — already shipped in `cru.sessions.fork(id, opts)` before this wave.
- ✅ **LuaCATS Type Stubs** — auto-generates on daemon start to `~/.config/crucible/luals/`; `cru.context` added to `UNIVERSAL_MODULES`.

### Wave 2 — Agent learning & teams (shipped 2026-05-11)

> 5 roadmap items shipped as 4 features: `entity-memory` + `session-digest` collapsed into one plugin running ONE LLM pass at session end (per-turn extraction rejected as 10-30% generation overhead). Memory Scoping reframed as a security boundary enforced at the storage query layer, not a tag filter.

- ✅ **`session-digest` runtime plugin** — completed sessions → linked notes (digests + atomic entity notes) via `on_session_end` hook; one LLM call per session, grammar-constrained JSON output, dedupes entities via `cru.kiln.search`. Absorbs `entity-memory` (which would have been a redundant second pass).
- ✅ **Memory Scoping** — `Scope::User/Workspace/Global` enforced at `SqliteNoteStore` and Lance post-filter; write-side validation prevents privilege escalation; raw-SQL escape hatch gated to `#[cfg(test)]`. Scope elevation deferred to a later wave.
- ✅ **Team Patterns** (core Rust) — `cru.team.{supervisor, router, broadcast}` on top of existing subagent infrastructure; supervisor sequential, broadcast parallel via `tokio::join_all`.
- ✅ **Grammar + Lua Integration** — `cru.grammar.{new, presets, set/clear/get_session_grammar}` with GBNF; hard-error on backends that don't support grammar (no silent fallback). Wired through `SessionAgent` config + RPC.

> Prerequisites that landed alongside the wave: `cru.kiln.create_note` (writes notes + reindexes), `cru.kiln.search` (semantic search wired to daemon vectors), and enriched `Session` userdata (`kiln_path`, `agent_name`, `end_reason`) on the `on_session_end` hook.

Code-review findings landed after the initial flip (4 critical fixes, 4 commits): `SqliteClientHandle` now binds the kiln path so `KnowledgeRepository` reads enforce workspace scope (precognition was bypassing the boundary at `Scope::Global`); `cru.team.*` is wired through `DaemonTeamServerBridge` from `Server::run` (previously stubbed); daemon-side idempotency guard ensures `on_session_end` fires exactly once per session (was firing twice — once from `session.end`, once from CLI shutdown — doubling extraction cost); `cru.kiln.search` over-fetch math corrected.

Remaining follow-ups (tracked separately, not blocking Wave 3):
- Wire llama-cpp backend to consume `SessionAgent.grammar` — until then `supports_grammar()` returns `false` everywhere and `set_session_grammar` hard-errors. The session-digest plugin gracefully degrades to prompt-only JSON discipline.
- Tighten `Scope::workspace()` canonicalization — currently falls back to non-canonical path when `canonicalize()` fails, creating asymmetric scope-equality paths.
- Legacy `DaemonStorageClient` read methods use unscoped RPC variants; user-scoped notes can leak through CLI / MCP `semantic_search` callers within a kiln.
- Per-session `digest: false` opt-out — needs session frontmatter on Session userdata to be implemented.

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

Wave 0 shipped 2026-05-07. The shortest route through remaining P0/P1 work:

```
Wave 1  →  Wave 2
(Lua       (agent
 surface)   learning)
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
| 2026-05-07 | Wave 0 shipped | 5 commits land CLI help discoverability, cache-stats RPC, configurable autocompact, validate-retry loop, and a `Summarize` strategy variant. Two follow-ups deferred: cache-stats Lua/statusline (trait surface) and LLM-driven Summarize recap (requires async backend access from `enforce_context_budget`). |
| 2026-05-07 | Wave 0 follow-ups shipped | Two follow-ups landed same day: (1) `cru.sessions.cache_stats` Lua binding via the `DaemonSessionApi` trait + a statusline `cache_hit_rate` component fed from `message_complete` cache fields; (2) Summarize now hoists the drain into an async wrapper inside `stream_chat_from_messages`, calls `summarize_via_backend` on the same genai client the agent uses, and replaces the placeholder with the LLM recap (static placeholder remains as the error-fallback). |

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
