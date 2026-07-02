---
title: Roadmap
description: Phase-based development timeline and dependency-ordered execution plan
type: roadmap
status: active
updated: 2026-06-28
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

> The web UI (SolidJS + Axum, `crucible-cli/src/web`) already provides an early web chat interface; HTTP→RPC bridge, SSE, chat/search/webhook APIs, and auth all shipped. Phase 1+2 work continues iteratively rather than waiting for a single big-bang release.

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

### Wave 2 — Agent learning & teams (shipped 2026-05-11, pruned 2026-05-12)

> Shipped as 1 feature after two consolidation passes. The first prune (2026-05-12) removed `session-digest`, `entity-memory`, and `cru.grammar.*` — digest's LLM-judged dedupe risked wrong merges and kiln pollution (user preference: prompted refinement over automatic digests), and the grammar plumbing had no working backend. A second pass (2026-05-12) pulled `cru.team.*` after recognising that supervisor/router/broadcast are delegation *patterns* users want to script with primitives, not hardcoded Rust orchestrators — see [[Help/Delegation Patterns]]. Memory Scoping kept as a per-kiln security boundary; `Scope` collapsed to a single workspace-only variant.

- ✅ **Memory Scoping** — single-variant `Scope::Workspace { path }` enforced at `SqliteNoteStore` and Lance post-filter; write-side validation prevents writes that name a different workspace; raw-SQL escape hatch gated to `#[cfg(test)]`. `Scope::workspace()` is fallible — silent canonicalization fallback removed.

Removed in the prune:
- `cru.team.{supervisor, router, broadcast}` — supervisor, router, broadcast are 5–20 line Lua recipes against `cru.sessions.{create, configure_agent, send_and_collect, end_session, collect_subagents}`; the hardcoded Rust shapes shut out variants (Lua decider vs LLM, regex classifier vs LLM, etc.) and shipped without consumers. Documented as [[Help/Delegation Patterns]]. ~1984 LOC removed (`crucible-daemon/src/team/` + `team_bridge.rs` + `crucible-lua/src/team.rs`).
- `runtime/plugins/session-digest/` (10 files, ~2098 LOC) — speculative LLM dedupe consumer.
- `cru.grammar.*` Lua module + `SessionAgent.grammar` + `BackendType::supports_grammar` + RPC + `AgentManager::{set,clear,get}_grammar` + `DaemonGrammarBridge` (~1500 LOC) — no working backend, every call path hard-errored.
- `cru.kiln.create_note` write path + `DaemonVaultApi::search` real impl + `DaemonVaultBridge` (~843 LOC) — only consumer was session-digest.
- `EndReason` enum + read-only `Session` userdata fields `kiln_path`/`agent_name`/`end_reason` — populated daemon-side for session-digest's `on_session_end` handler.
- Pre-prune `Scope::Global` / `Scope::User { id }` variants and the `can_read`/`can_write` matrix — replaced by `same_workspace(other) -> bool`.
- Unscoped `*_scoped` client RPC suffix dropped now that there's only one shape.

Daemon-side idempotency guard (`LuaSessionState.end_hooks_fired`) for `on_session_end` stays — independent correctness fix.

Remaining follow-ups (not blocking Wave 3):
- Legacy frontmatter `scope: global` / `scope: user:*` are now refused with `ScopeError::Unsupported` — notes carrying those values become invisible and will need a one-time migration if any user data has them.

### Wave 3 — Workflows Phase 2 (parser + engine landed; finish out)

> `WorkflowDoc` parser, execution engine, daemon RPC, CLI, inline LLM, completion assessment, and resumability all shipped per recent commits. Remaining work is the surrounding ergonomics.

- **Workflow Sessions** — markdown execution log + resume from interruption
- **Markdown Handlers** — event handlers in pure markdown, inject context into agents
- ✅ **Parallel Execution** (2026-06-10) — `&` heading prefix and case-insensitive `(parallel)` section suffix set `WorkflowStep.parallel`; consecutive parallel siblings collapse into one engine `ParallelGroup` slot run via `join_all` (branch = member + descendants, sequential within; scope snapshot at group start; document-order event/output merge). Join-all-then-fail aggregates every branch failure; gates inside a group fail their branch. Inline LLM turns serialize per session (turn guard + bounded `ConcurrentRequest` retry) — per-branch agent dispatch is `fan` territory.
- **Session Learning** — codify successful sessions → reusable workflows. Depends on `session-digest` (Wave 2) and Markdown Handlers.
- **Workflow Authoring guide** — docs only

### Wave 4 — Web/HTTP foundation primitives (gateway shipped; rendering primitives next)

> HTTP→RPC bridge, SSE, chat/search/webhook APIs, and auth are in. The remaining foundation is rendering.

- **Static File Serving** — SolidJS bundle + PWA manifest + service worker
- ✅ **Oil Node Serialization** — `Serialize` derives across the full `Node`/`Style` type graph behind the `serde` cargo feature on `crucible-oil` (TUI builds pay nothing). Externally-tagged snake_case JSON; default-valued fields omitted (missing key ⇒ default). Contract locked by insta snapshots in `crucible-oil/tests/serialize_json.rs`.
- **SolidJS Oil Renderer** — `<OilNode>` component tree (depends on Oil Node Serialization)
- **Plugin Panel Hosting** — iframe sandbox + message-passing protocol (depends on Oil Node Serialization)
- **PWA Support** — ✅ shipped 2026-06-10 (manifest + workbox service worker via vite-plugin-pwa; app-shell precache only, `/api/*` incl. SSE never intercepted)

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

### Agent learning & tooling follow-ups (post-Hermes-Agent review, 2026-06-28)

> A repo review against Nous Research's Hermes Agent surfaced four items where Crucible's intent wasn't yet reflected in the build. Status detail lives in [[Meta/Product]]; ordering below. These are mostly independent and can slot in opportunistically.

- ✅ **Skill Context Injection** — shipped: `format_skills_for_context` renders the tier-1 catalog into the daemon's enriched prompt (kiln-gated); full `SKILL.md` loads on demand via the `skill_view` tool.
- ✅ **Progressive Tool Disclosure** — shipped: automatic budget-based deferral of gateway/user MCP tool schemas behind the `discover_tools`/`get_tool_schema`/`invoke_tool` bridge; kiln and workspace tools never deferred; `invoke_tool` unwrapped before hooks/permissions and cannot escape plan mode.
- **Reflection Pass** (second self-improvement avenue) — forked post-session reviewer that *proposes* notes/skills with provenance. Depends on delegation primitives (shipped) + skill self-creation. Relates to Wave 8 proactive work. Must not repeat the auto-merge `session-digest` mistake — propose, don't dispose.
- **Execution Backends as Plugins** — ✅ already shipped via the `oci` runtime plugin + `pre_tool_call` interception; recorded in Product so the "backends are plugins, not core" stance is explicit.

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
| 2026-06-28 | Hermes Agent review → 4 follow-ups recorded | Compared Crucible against Nous Research's Hermes Agent runtime. Convergent design on most axes (native function-calling, file-based memory, agentskills.io skills, delegation, cache-preserving non-every-turn injection). Four intent-vs-build gaps captured: skill context injection (dead `format_skills_for_context`), progressive tool disclosure, a second self-improvement avenue (reflection pass), and execution-backends-as-plugins (already shipped via `oci`). Crucible's semantic knowledge graph remains the differentiator Hermes lacks in core (it's lexical FTS5 + optional vector plugins). |
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
