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
| **parser** | Markdown → structured data (extensions, frontmatter, blocks) | `crucible-core/parser` |
| **storage** | Persistence: SQLite (default), LanceDB (vector) | `crucible-daemon/storage/{sqlite,lance}` |
| **sync** | Merkle-CRDT sync across devices, collaborators, and federated agents | `crucible-sync` (future) |
| **agents** | Agent cards, handles, LLM providers, tool registry | `crucible-core/agents`, `crucible-daemon/llm`, `crucible-daemon/tools`, `crucible-daemon/acp` |
| **workflows** | Definitions (markup) + sessions (logging, resumption) | `crucible-core/workflow` |
| **plugins** | Extension points, hooks, scripting (Lua) | `crucible-lua` |
| **apis** | HTTP REST, WebSocket, events | `crucible-web/src` |
| **cli** | Commands, REPL, TUI, configuration | `crucible-cli`, `crucible-oil`, `crucible-core/config` |
| **daemon** | Multi-session server, RPC, agent management | `crucible-daemon` |
| **observe** | Session logging, JSONL event streams, markdown export | `crucible-daemon/observe` |


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

- SQLite (default) — fast, lightweight, recommended for most users
- Content-addressed block storage
- Merkle tree integrity verification
- Kiln management

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
- Delegation: agents delegate tasks via the `delegate_session` tool; children run as real (hidden, parent-linked) sessions through the main scheduler loop (`DelegationService`). Targets resolve to agent cards (model chain: card-explicit > specialty via `[llm.models]` > inherit-from-parent) or ACP profiles; policy via `DelegationConfig` (enabled, max_depth incl. real nesting, allowed_targets, timeout_secs)

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
- Scripting runtime (Lua with Fennel support)
- Runtime modules under unified `cru.*` namespace (`cru.timer`, `cru.ratelimit`, `cru.retry`, `cru.emitter`, `cru.check`, `cru.fs`, `cru.http`, `cru.session`, etc.)
- Daemon-side plugins (e.g., Discord integration as a Lua plugin)

See: [[Help/Extending/Event Hooks]], [[Help/Extending/Custom Handlers]]

### apis

External interfaces for programmatic access.

- HTTP REST (query data, trigger actions)
- Server-Sent Events (streaming responses)
- MCP server for external tools

**Client-local state.** A view may persist its own presentation state
(`web-layout.json`, `web-layout.recents.json`) and its own transport credentials
(`sessions.json`, written 0600). The test is whether *another client or an agent*
would need to read it: model, temperature and mode would, so they are
daemon-side; pane geometry and browser tokens would not. Recents are the
borderline case — client-local until a second surface wants them, and then they
move to the daemon rather than being copied.

This is not an exception to "the daemon owns all business logic". The daemon has
no concept of a browser login and must not acquire one, and pane geometry is a
blob the server stores without interpreting. Recents live server-side rather
than in `localStorage` because per-origin storage vanished across ports and
browsers — that reason is about *where the bytes go*, not about who owns the
rule.

See: [[Help/Extending/MCP Gateway]]

### cli

Command-line user interface.

- Subcommands (search, process, chat, agents, etc.)
- TUI chat interface with Oil renderer
- Configuration management
- Output formatting (table, JSON)

See: [[Help/CLI/Index]], [[Help/TUI/Index]]

### daemon

Multi-session server for concurrent agent access. Owns all business logic that views (CLI, TUI, Web) consume over RPC.

- Unix socket RPC (`cru daemon serve`) with 82+ registered methods
- Session lifecycle: create, pause, resume, resume_from_storage, end, archive, unarchive, delete, compact, replay
- Agent management: configure, send_message, cancel, switch_model, list_models, interaction_respond
- Session config: thinking_budget, temperature, max_tokens, precognition (get/set pairs)
- Kiln CRUD: open, close, list, set_classification, search_vectors, list_notes, get_note_by_name
- Note CRUD: upsert, get, delete, list
- Processing: process_file, process_batch
- Notifications: add, list, dismiss per session
- Event streaming via subscriptions (subscribe/unsubscribe with wildcard support)
- Lua runtime: init_session, register_hooks, execute_hook, shutdown_session, discover_plugins, plugin_health, generate_stubs, run_plugin_tests, register_commands
- Plugin management: reload, list
- Project management: register, unregister, list, get
- Storage operations: verify, cleanup, backup, restore
- MCP server control: start, stop, status
- Skills discovery: list, get, search
- Agent profiles: list_profiles, resolve_profile
- Tool dispatch via `DaemonToolDispatcher`: routes tool calls to the correct executor (built-in Rust tools, Lua plugin tools, or external MCP server tools) using a provider chain with lazy name hydration
- Tool dispatch enforces a 30-second timeout per tool call; timed-out calls return an error to the LLM so it can retry or adjust
- Auto-archive sweep runs every 30 minutes, archiving sessions idle beyond a configurable threshold (default 72 hours)

See: [[Help/Core/Sessions]], AGENTS.md Daemon Architecture section

### observe

Session logging and observability. Captures session events as append-only streams.

- Append-only JSONL event logs per session
- Human-readable markdown export on demand
- Optional SQLite indexing for fast session queries
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
| Colour palette | `crucible.colorscheme.setup()` — semantic colours, terminal slots, adaptive pairs |
| Highlight groups | `crucible.hl.set/link` — open, linkable namespace |
| Surface geometry | `crucible.ui.setup()` — borders, padding, prompt glyphs, layout |
| Statusline layout | `crucible.statusline.setup()` — item trees, multiple bars, anchors |
| Code highlighting | `crucible.syntax.setup()` — derived from the colorscheme by default |
| Theme tokens | *(planned)* — color palette, style overrides |
| Keybinding remaps | *(planned)* — user-defined key → action mapping |
| Event handlers | Hooks on session events (turn complete, tool call, etc.) |

### Decision Filter

> Would a user reasonably want to change this without changing Crucible's behavior?
>
> **YES** → Lua surface (statusline layout, colors, key bindings)
> **NO** → Rust (rendering correctness, protocol, input handling)

### Embedded Defaults

Lua surfaces ship with embedded Rust defaults (`statusline_items::builtin_default()`, `ThemeConfig::default_dark()`) so a client renders correctly before — or without — any daemon config. This ensures:

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
free if and only if it emits the same `SessionEventMessage` vocabulary with the
same fields populated** — `tool_call` with its `source`, `display` and `diffs`,
`tool_result` with the `{"result"|"error": …}` envelope, `thinking`,
`text_delta`, `segment_complete`, `message_complete`. A field a handle leaves
`None` is a card the renderer draws with less information, not an error anyone
sees.

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
- One crate may implement parts of multiple systems (e.g., `crucible-core` has parser types and agent traits)

The system boundary is about **what** (requirements), crates are about **how** (implementation).

## Related

