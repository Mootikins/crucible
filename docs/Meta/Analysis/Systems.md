---
title: Crucible Systems
description: Orthogonal systems that make up Crucible architecture
tags:
  - meta
  - architecture
  - systems
---

# Crucible Systems

This document defines the orthogonal systems that make up Crucible. Each system has clear boundaries and responsibilities. Checked against the code at commit 7053bcfe7 (2026-08-22).

## System Boundaries

| System | Scope | Code |
|--------|-------|------|
| **parser** | Markdown to structured data (extensions, frontmatter, blocks) | `crucible-core/src/parser` |
| **storage** | Persistence: SQLite (metadata, FTS, links, embeddings) | `crucible-daemon/src/storage/sqlite` |
| **agents** | Agent cards, handles, LLM providers, tool registry | `crucible-core/src/agent`, `crucible-daemon/src/llm`, `crucible-daemon/src/provider`, `crucible-daemon/src/tools`, `crucible-daemon/src/acp` |
| **workflows** | Definitions (markup), engine, gates, RPC | `crucible-core/src/workflow`, `crucible-daemon/src/rpc/workflow_handlers.rs` |
| **plugins** | Extension points, hooks, scripts (Luau) | `crucible-lua`, `crucible-daemon/src/daemon_plugins` |
| **apis** | HTTP REST, SSE, WebSocket | `crucible-web/src` |
| **cli** | Commands, REPL, TUI, configuration | `crucible-cli`, `crucible-oil`, `crucible-core/src/config` |
| **daemon** | Multi-session server, RPC, agent management | `crucible-daemon` |
| **observe** | Session logs, JSONL event streams, markdown export | `crucible-daemon/src/observe` |

No sync system exists. No crate holds Merkle, CRDT or Loro code. The only Merkle
helper is a pair-hash function in `crucible-core/src/hashing/algorithm.rs`. Sync
is an idea, not a system.

## System Descriptions

### parser

The input layer. It transforms markdown notes into structured data.

- Frontmatter extraction (YAML properties)
- Block extraction (headings, paragraphs, code, and so on)
- Syntax extensions (wikilinks, tags, callouts)
- Block hashes for change detection

See: [[Help/Concepts/The Knowledge Graph]]

### storage

The persistence layer. It stores and retrieves structured data.

- SQLite note store, keyed by path, with one content hash per note (`storage/sqlite/note_store.rs`)
- Full-text search (`storage/sqlite/fts.rs`)
- Wikilink resolution and backlinks (`storage/sqlite/link_index.rs`)
- Properties (`storage/sqlite/property_store.rs`)
- Kiln management

There is no block store, and no Merkle verification.

See: [[Help/Concepts/Kilns]], and [[Storage Schema]] for the kiln database's
migration ladder, which of its tables are rebuildable, and how to add a column.

### agents

The AI agent infrastructure. It manages agent definitions and execution.

- Agent cards (system prompts, metadata)
- Agent handles (the interface for communication)
- LLM providers: Ollama, OpenAI-compatible, Copilot (`crucible-daemon/src/provider/`)
- Context management (sliding window, compaction)
- Tool registry and MCP integration
- Delegation: agents delegate tasks with the `delegate_session` tool. Children run as hidden, parent-linked sessions through the main scheduler loop (`DelegationService`, `delegation.rs:93`). A target resolves to an agent card or an ACP profile. The model chain is: card-explicit, then specialty via `[llm.models]`, then inherit-from-parent (`crucible-core/src/session/types/agent.rs:195`). Policy comes from `DelegationConfig` (`config/components/acp.rs:38`): enabled, max_depth, allowed_targets, timeout_secs.

See: [[Help/Concepts/Agents & Protocols]], [[Help/Extending/Internal Agent]]

### workflows

Workflow definitions and execution. Implemented.

- Workflow markup (a DAG in markdown prose)
- Engine (`crucible-core/src/workflow/engine.rs`) and registry
- Four RPC methods: `workflow.start`, `workflow.status`, `workflow.approve_gate`, `workflow.cancel`

See: [[Help/Workflows/Workflow Syntax]]

### plugins

The extension layer.

- Hook points (stages and events)
- Scripting runtime (Lua, with Fennel support)
- Runtime modules under the `cru.*` namespace: `cru.timer`, `cru.ratelimit`, `cru.retry`, `cru.emitter`, `cru.check`, `cru.fs`, `cru.http`, `cru.session` (`daemon_plugins/mod.rs:173`)
- Daemon-side plugins, for example the Discord integration (`runtime/plugins/discord`)

See: [[Help/Extending/Event Hooks]], [[Help/Extending/Custom Handlers]]

### apis

External interfaces for programmatic access.

- HTTP REST (query data, trigger actions)
- Server-Sent Events (streamed responses)
- MCP server for external tools

**Client-local state.** A view may persist its own presentation state
(`web-layout.json`, `web-layout.recents.json`, `crucible-web/src/routes/layout.rs:69`) and its own transport credentials
(`sessions.json`, written 0600, `middleware/auth/session.rs:51`). The test is whether *another client or an agent*
would need to read it. Model, temperature and mode would, so they are
daemon-side. Pane geometry and browser tokens would not. Recents are the
borderline case: client-local until a second surface wants them. Then they
move to the daemon. Nobody copies them.

This is not an exception to "the daemon owns all business logic". The daemon has
no concept of a browser login and must not acquire one. Pane geometry is a
blob the server stores without interpretation. Recents live server-side rather
than in `localStorage` because per-origin storage vanished across ports and
browsers. That reason is about *where the bytes go*, not about who owns the
rule.

See: [[Help/Extending/MCP Gateway]]

### cli

The command-line user interface.

- Subcommands (search, process, chat, agents, and so on)
- TUI chat interface with the Oil renderer
- Configuration management
- Output formats (table, JSON)

See: [[Help/CLI/Index]], [[Help/TUI/Index]]

### daemon

A multi-session server for concurrent agent access. It owns all business logic that the views (CLI, TUI, Web) consume over RPC.

- Unix socket RPC (`cru daemon serve`). The `rpc_methods!` table in `crucible-daemon/src/rpc/dispatch.rs:83` is the one list of methods. It has 156 rows. The largest groups are `session.*` (75), `plugin.*` (11), `lua.*` (8), `note.*` (6), `review.*` (5), `kiln.*` (5), and 11 top-level methods such as `search_vectors`, `search_text`, `search_grep`, `list_notes`, `get_note_by_name`, `get_backlinks`. Do not copy the list here. Read the table.
- Event streaming via subscriptions (subscribe/unsubscribe with wildcard support)
- Tool dispatch via `DaemonToolDispatcher` (`tool_dispatch.rs:117`). It routes tool calls to the correct executor (built-in Rust tools, Lua plugin tools, or external MCP server tools) through a provider chain with lazy name hydration.
- Tool dispatch enforces a 30 second timeout per tool call. `delegate_session` gets the delegation timeout plus 30 seconds (`messaging/tool_call.rs:647-660`). A timed-out call returns an error to the LLM, so it can retry or adjust.
- The auto-archive sweep runs every 30 minutes. It archives sessions idle beyond a configurable threshold, default 72 hours (`server/mod.rs:664-667`).

See: [[Help/Core/Sessions]], AGENTS.md Daemon Architecture section

### observe

Session logs and observability. It captures session events as append-only streams.

- Append-only JSONL event logs per session
- Human-readable markdown export on demand
- `observe/indexer.rs` builds a `NoteRecord` from the JSONL log, so the kiln note store can index a session. It does not open SQLite itself.
- Event types: user messages, assistant responses, tool calls, thinking blocks, errors

See: [[Help/Core/Sessions]]

## Rust/Lua Boundary

Crucible follows a "scriptable surfaces, not a scripted runtime" model. Lua owns presentation and policy. Rust owns structure and correctness.

### What Stays in Rust

| Area | Reason |
|------|--------|
| Rendering engine (Oil) | Node tree, layout, ANSI output — correctness-critical |
| Input FSM | Key events, mode transitions, focus management |
| Component framework | `Component` trait, `ViewContext`, lifecycle |
| Session/agent protocol | RPC, event streaming, message types |
| Parser | Markdown → AST — deterministic, perf-sensitive |
| Storage | Database operations, indexing, embedding |

### What Lua Controls

| Surface | How |
|---------|-----|
| Colour palette | `cru.colorscheme.setup()` — semantic colours, terminal slots, adaptive pairs |
| Highlight groups | `cru.hl.set/link` — open, linkable namespace |
| Surface geometry | `cru.geometry.setup()` — borders, padding, prompt glyphs, layout |
| Statusline layout | `cru.statusline.setup()` — item trees, multiple bars, anchors |
| Code highlighting | `cru.syntax.setup()` — derived from the colorscheme by default |
| Keybinding remaps | *(not implemented)* — user-defined key to action mapping |
| Event handlers | Hooks on session events (turn complete, tool call, etc.) |

### Decision Filter

> Would a user reasonably want to change this without changing Crucible's behavior?
>
> **YES** → Lua surface (statusline layout, colors, key bindings)
> **NO** → Rust (rendering correctness, protocol, input handling)

### Embedded Defaults

Lua surfaces ship with embedded Rust defaults in `crucible-lua` (`statusline_items::builtin_default()` at `statusline_items.rs:386`, `ThemeConfig::default_dark()` at `theme.rs:430`), so a client renders correctly before, or without, any daemon config. This ensures:

1. The TUI works without any Lua initialization (tests, emergency fallback)
2. User's `init.lua` overrides the default — not required for basic functionality
3. One rendering path (config-driven) for both default and custom configs

## Presentation Parity Boundary

Crucible runs turns through two kinds of agent — the internal one
(`GenaiAgentHandle`) and a delegated ACP agent (`AcpAgentHandle`) — and both must
reach the user as the same picture. **`SessionEventMessage` is the boundary where
that becomes true.** Downstream of it there is exactly one renderer per surface:
`chat_runner/commands.rs::session_event_to_chat_msgs()` → `ChatAppMsg` →
`ContainerList` for the TUI, `crucible-web/src/events.rs` for the web. Nothing in
`crucible-cli/src/tui/oil/` branches on which agent produced the turn.

The contract that follows: **a new `AgentHandle` gets correct presentation for
free if and only if it emits `TurnPayload` values with the same fields
populated.** That contract used to be stated here as a list of event names and
fields; it is now a type —
`crucible-core/src/protocol/session_events/turn.rs` — whose variant list *is*
the vocabulary and whose fields *are* the fields. Both renderers match on it
exhaustively, so an event added to the group fails their builds rather than
falling through to a trace log. A field a handle leaves `None` is a card the
renderer draws with less information, not an error anyone sees.

One correction the type made: `tool_result`'s `data.result` is not the two-key
`{"result"|"error": …}` envelope this section used to describe. It is
`ToolResultBody` — `result` **or** `error`, plus an optional `spill_path` (set
when a ≥10KB output was written to `$CRU_SESSION_DIR/tools/`) and an optional
`summary` (from a `tool:display_complete` Lua hook). Four keys, and the
disjointness of `result`/`error` is what makes the untagged decode unambiguous.

**`TurnEvent`-level cross-agent equality is structurally impossible and must
never be asserted.** The two handles differ there *by design*: the internal agent
yields `ToolCall` + `ToolBatchEnd` and lets the daemon dispatch the tool,
receiving the result back **inbound**; an ACP agent (`owns_history`) runs its own
tool loop in its own process and yields `ToolCall` + `ToolResult` **outbound**.
`GenaiAgentHandle` never yields a `ToolResult` at all — it only ever matches one
as inbound. So `assert_eq!(turn_events(acp), turn_events(internal))` compares two
things that are *supposed* to be different, and any test written that way is
either failing for the wrong reason or passing by accident. `TurnEvent` tests are
**per-agent contract expectations** ("does this handle emit what its own contract
requires"), never cross-agent comparisons.

**`acp_integration/display_parity.rs` sits above the boundary and cannot prove
parity on its own.** Despite the name it stops at `StreamingChunk`, which is
upstream of `TurnEvent` and two layers upstream of anything the user sees; a green
run there says the ACP client parsed the wire, not that the turn renders. Real
parity evidence is a pair of `SessionEvent` recordings of the same behaviour — one
per agent — pumped through the shared renderer and compared as frames
(`user_story_tests/acp_parity_tests.rs`, fixtures in `assets/fixtures/acp_parity_*`).
Those fixtures are re-derived from the daemon's own broadcast channel on every test
run by `agent_manager/tests/parity_capture.rs`, so they cannot quietly outlive the
shape they pin — a recorded payload nothing regenerates is the same trap as a
unit-tested code path production never reaches.

The equality such a pair proves is **per-behaviour, not general**: it covers the
tools and event shapes those two recordings contain. Divergences outside them stay
open until a pair exercises them.

See: [[Meta/TUI User Stories]] (US-307), [[Help/Concepts/Agent Client Protocol]]

## Cross-Cutting Concerns

Some changes span multiple systems:

- **Security**: Authentication, authorization, sandboxing (touches apis, agents, plugins)
- **Observability**: Logging, metrics, tracing (touches all systems)
- **Configuration**: Unified config format (touches cli, storage, agents)

## Relationship to Crates

Systems are conceptual groupings. Crates are implementation units.

- One system may span multiple crates (e.g., `agents` → `crucible-daemon/llm`, `crucible-daemon/tools`, `crucible-daemon/acp`)
- One crate may implement parts of multiple systems (e.g., `crucible-core` has parser types, agent types and the workflow engine)

The system boundary is about **what** (requirements), crates are about **how** (implementation).

## Related
