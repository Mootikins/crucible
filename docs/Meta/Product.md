---
title: Product
description: Product feature map — capabilities, status, documentation, and dependencies
type: product
status: active
updated: 2026-02-03
tags:
  - meta
  - product
  - moc
---

# Crucible Product Map

> A living inventory of every capability, organized by what users get.
>
> **Legend**: `[x]` shipped · `[-]` in progress · `[ ]` planned
> **Phases**: `P0` core · `P1` extensibility · `P2` workflows · `P3` polish · `P4` scale

## Vision

**"Neovim for agents+notes"** — extensible, open, documented

A knowledge management system where:
- AI agents have perfect context from your kiln
- Workflows are defined in markdown and executed by agents
- Everything is extensible via Lua scripting and hooks
- Power users get CLI, everyone else gets web/desktop UI eventually

## User Progression

| Phase | Users | Interface |
|-------|-------|-----------|
| Now | Power users, developers | CLI (chat-focused) |
| Next | Plugin creators, agent developers | CLI + Lua scripting + messaging integrations |
| Later | Broader audience, mobile users | Web PWA (self-hosted via Tailscale/Cloudflare Tunnel) |

---

## Note-Taking & Authoring

- [x] **Wikilinks** `P0` — `[[note]]` linking with aliases, headings, and block refs · [[Help/Wikilinks]] · `crucible-parser`
- [x] **Tags** `P0` — `#tag` and `#nested/tag` taxonomy · [[Help/Tags]] · `crucible-parser`
- [x] **Frontmatter** `P0` — YAML/TOML metadata in note headers · [[Help/Frontmatter]] · `crucible-parser`
- [x] **Block References** `P0` — `^block-id` paragraph-level linking · [[Help/Block References]] · `crucible-parser`
- [x] **Callouts** `P0` — `> [!type]` admonition blocks · `crucible-parser`
- [x] **LaTeX** `P0` — `$inline$` and `$$block$$` math notation · `crucible-parser`
- [x] **Footnotes** `P0` — Reference-style footnotes · `crucible-parser`
- [x] **Tables** `P0` — Markdown tables with alignment · `crucible-parser`
- [x] **Task Lists** `P0` — `- [ ]` / `- [x]` checkbox items · [[Help/Task Management]] · `crucible-parser`
- [x] **Kilns** `P0` — Vault-like note collections with `.crucible/` config · [[Help/Concepts/Kilns]] · `crucible-core`
- [x] **Plaintext First** `P0` — Markdown files are always the source of truth · [[Help/Concepts/Plaintext First]]
- [ ] **Note Types** `P3` — Templates and typed notes (book, meeting, movie) · `crucible-core`

## Knowledge Discovery

- [x] **Semantic Search** `P0` — Vector similarity search with reranking · [[Help/Concepts/Semantic Search]] · `crucible-sqlite`, `crucible-llm`
- [x] **Full-text Search** `P0` — Fast text search across all notes · [[Help/CLI/search]] · `crucible-sqlite`
- [x] **Knowledge Graph** `P0` — Wikilink-based graph structure and traversal · [[Help/Concepts/The Knowledge Graph]] · `crucible-sqlite`
- [x] **Query System** `P0` — Structured note queries with composable pipeline · [[Help/Query/Query System]] · `crucible-query`
- [x] **Property Search** `P0` — Search notes by frontmatter properties and tags · `crucible-tools`
- [x] **Document Clustering** `P0` — Heuristic clustering and MoC detection · `crucible-sqlite`
- [ ] **K-Means Clustering** `P2` — K-means implementation (placeholder stub, needs ndarray or similar) · `crucible-sqlite`
- [x] **Block-level Embeddings** `P0` — Paragraph-granularity semantic indexing · `crucible-llm`, `crucible-sqlite`
- [x] **Session Search** `P0` — Past conversations indexed and searchable via session indexing pipeline; `cru session reindex` for batch processing · `crucible-observe`, `crucible-cli`

## Agent Learning & Memory

> Agents that get smarter over time. Learning is implemented as **notes in the kiln** — not opaque database stores. Entity facts, session summaries, and accumulated knowledge are all atomic zettelkasten-style markdown notes with wikilinks, tags, and frontmatter. This means agent memory is human-readable, editable, searchable via the existing knowledge graph, and available to precognition for future context injection.
>
> **Two-tier model**: Core Rust features (precognition, auto-linking) handle the fast path. Default runtime Lua plugins (entity-memory, session-digest) handle higher-level knowledge extraction. Both are toggleable and overridable. See [[#Core Agent Features]] and [[#Default Runtime Plugins]].
>
> **Informed by**: Agno framework analysis (2026-02). Agno uses six opaque DB-backed learning stores. Crucible's approach is strictly better — same learning capabilities but with human-readable, editable, wikilinked notes as the storage layer. Local embeddings, metadata (wikilinks, tags, frontmatter), and potential future LSP integration provide rich fetchable context without custom storage.

- [x] **Precognition** `P0` — Auto-RAG: inject relevant kiln/session context before each agent turn; default on; `:set precognition`; the core differentiator — every conversation is knowledge-graph-aware · `crucible-cli`, `crucible-acp`
- [x] **Session Persistence** `P0` — Conversations saved as markdown + JSONL in kiln; indexed for semantic search · `crucible-daemon`, `crucible-observe`
- [ ] **Entity Memory Plugin** `P1` — Default runtime Lua plugin; extracts entities and structured facts from conversations → atomic notes in `Entities/` folder with wikilinks to source sessions; zettelkasten-style: one note per entity, updated across sessions; uses `turn:complete` hook + `cru.tools.call("semantic_search", ...)` to deduplicate · `runtime/entity-memory/`
- [ ] **Session Digest Plugin** `P1` — Default runtime Lua plugin; summarizes completed sessions → linked notes in `Sessions/` folder; captures key decisions, topics discussed, entities mentioned; wikilinks to entity notes and source notes; builds the "what did we talk about?" knowledge layer · `runtime/session-digest/`
- [ ] **Memory Scoping** `P2` — Namespace agent memory: per-user, per-workspace, or global; entity notes tagged with scope; precognition filters by active scope · `crucible-core`, `crucible-lua`

## AI Chat & Agents

### Conversation & Sessions
- [x] **Interactive Chat** `P0` — Conversational AI with streaming text, thinking, tool calls, and subagent events · [[Help/CLI/chat]] · `crucible-cli`, `crucible-rig`
- [x] **Agent Cards** `P0` — Configurable agent personas with system prompts, model, temperature, tools · [[Help/Extending/Agent Cards]] · [[Help/Config/agents]] · `crucible-config`
- [x] **Session Persistence** `P0` — Conversations saved as markdown + JSONL in kiln · [[Help/Core/Sessions]] · `crucible-daemon`
- [x] **Session Resume** `P0` — Load and continue previous sessions with full history · [[Help/Core/Sessions]] · `crucible-daemon`, `crucible-rpc`
- [x] **Conversation History** `P0` — Clear history (`:clear`), resume with prior messages; TUI viewport hydrated from daemon session events · `crucible-rig`
- [x] **Message Queueing** `P0` — Type and queue messages during streaming; Ctrl+Enter force-sends · `crucible-cli`

### Agent Runtime
- [x] **Internal Agent** `P0` — Built-in agent with session memory and tool access · [[Help/Extending/Internal Agent]] · `crucible-rig`
- [x] **Multiple LLM Providers** `P0` — Ollama, OpenAI, Anthropic via unified interface · [[Help/Config/llm]] · `crucible-rig`
- [x] **Model Switching** `P0` — Runtime `:model <name>` with autocomplete; model listing from Ollama · `crucible-daemon`, `crucible-cli`
- [x] **Temperature Control** `P0` — Runtime `:set temperature=0.5`; range 0.0–2.0; persisted per-session · `crucible-daemon`, `crucible-cli`
- [x] **Max Tokens Control** `P0` — Runtime `:set max_tokens=4096` or `none` for provider default; persisted · `crucible-daemon`, `crucible-cli`
- [x] **Extended Thinking** `P0` — Budget presets (off/minimal/low/medium/high/max) via `:set thinkingbudget`; Ctrl+T toggles display · `crucible-daemon`, `crucible-cli`
- [x] **System Prompt** `P0` — Loaded from agent card at session creation; layered prompt composition · `crucible-rig`, `crucible-config`
- [x] **Environment Overrides** `P0` — `--env KEY=VALUE` flag for per-session env vars (e.g., API keys) · `crucible-cli`
- [x] **Agent Cancellation** `P0` — Ctrl+C/Esc cancels local stream and propagates to daemon via `session.cancel` RPC · `crucible-daemon`

### Tools & Permissions
- [x] **Tool Calls** `P0` — Inline tool execution with streaming results; parallel calls tracked by call_id · `crucible-rig`, `crucible-tools`
- [x] **Permission System** `P0` — Multi-layer: safe-tool whitelist → pattern matching → Lua hooks → user prompt · `crucible-daemon`
- [x] **Pattern Whitelisting** `P0` — "Always allow" saves project-scoped patterns for future sessions · `crucible-daemon`
- [x] **Permission Hooks (Lua)** `P0` — Custom Lua hooks can Allow/Deny/Prompt with 1s timeout · `crucible-lua`, `crucible-daemon`
- [x] **Interaction System** `P0` — Agent can ask questions (single/multi-select, free-text) and request permission · `crucible-core`, `crucible-daemon`
- [x] **Subagent Spawning** `P0` — Background job manager for parallel subagent tasks with cancellation · `crucible-daemon`

### Context & Knowledge
- [x] **Context Enrichment** `P0` — Inject kiln context into agent conversations · `crucible-daemon`
- [x] **File Attachment** `P0` — `@file` context attachment in chat · `crucible-cli`
- [x] **Rules Files** `P0` — Project-level AI instructions (`.crucible/rules`) · [[Help/Rules Files]] · `crucible-config`

### Core Agent Features (Toggleable via `:set`)

> Core capabilities implemented in Rust for performance and reliability, toggleable like precognition (`:set autolink`, `:set noprecognition`). These expose hook points that Lua plugins can intercept and override. `:plugins` shows both core and Lua plugins in a unified view.

- [x] **Precognition** `P0` — Auto-RAG: inject relevant kiln context before each agent turn; `:set precognition`; the core differentiator — every conversation is knowledge-graph-aware · `crucible-cli`, `crucible-acp`
- [ ] **Auto-Linking** `P1` — Detect unlinked mentions of existing notes in agent output and conversation; suggest or apply wikilinks; emits `autolink:suggest` events for Lua override; `:set autolink` · `crucible-core`, `crucible-daemon`
- [ ] **Team Patterns** `P1` — Multi-agent orchestration primitives in core Rust; supervisor (decompose → delegate → synthesize), router (route to specialist by topic), broadcast (parallel, gather perspectives); builds on existing subagent spawning infrastructure; `:set team.default_pattern=supervisor` · `crucible-daemon`

### Lua Session API
- [x] **Scripted Agent Control** `P0` — Lua API for temperature, max_tokens, thinking_budget, model, mode; daemon getters use local cache · `crucible-lua`
- [x] **Session Event Handlers** `P0` — Lua hooks on `turn:complete` can inject follow-up messages · `crucible-lua`, `crucible-daemon`

### Lua Session & Tool Primitives (Planned)

> These fill gaps so autonomous loops, fan-out, and context control are trivial plugins — not bespoke features.

- [x] **`cru.tools.call(name, args)`** `P1` — Programmatic tool calling from Lua; returns results synchronously; respects session permission scope; the bridge between "plugins that react" and "plugins that do intelligent work" · `crucible-lua`, `crucible-tools`
- [x] **`cru.tools.batch({...})`** `P1` — Concurrent multi-tool calls; `batch({{"semantic_search", {query="X"}}, {"list_notes", {tag="Y"}}})` runs in parallel via async runtime; essential for digest/summarization plugins · `crucible-lua`, `crucible-tools`
- [ ] **`session.messages()`** `P1` — Read conversation history from Lua; enables context windowing, summarization, checkpoint detection · `crucible-lua`
- [ ] **`session.inject(role, content)`** `P1` — Insert messages mid-conversation; enables fan-out result collection, context injection at checkpoints · `crucible-lua`
- [ ] **`session.fork()`** `P1` — Branch conversation state; enables parallel exploration, A/B approach testing · `crucible-lua`
- [ ] **`subagent.collect(ids)`** `P1` — Await multiple subagents and collect results; enables fan-out patterns · `crucible-lua`

### In Progress / Planned
- [x] **MCP Tool System** `P0` — Permission prompts via `PermissionGate` trait, ACP integration, `McpProxyTool` injection · `crucible-tools`, `crucible-acp`
- [x] **Error Handling UX** `P0` — Toast notifications, contextual messages, graceful degradation for DB lock/search/kiln fallback, `BackendError::is_retryable()` + `retry_delay_secs()`, RPC `call_with_retry()` for idempotent daemon ops, recovery suggestions in error messages · `crucible-cli`, `crucible-core`, `crucible-rpc`
- [x] **Per-session MCP Servers** `P0` — Agent cards define MCP servers; `mcp_servers` propagated to `SessionAgent` and wired in daemon · `crucible-acp`
- [ ] **Grammar + Lua Integration** `P1` — Constrained generation for structured agent outputs · `crucible-core`

## Terminal Interface (TUI)

### Modes & Input
- [x] **Chat Modes** `P0` — Normal, Plan (read-only), Auto (auto-approve); cycle with BackTab; syncs to daemon agent · [[Help/TUI/Modes]] · `crucible-cli`
- [x] **Input Modes** `P0` — Normal (`>`), Command (`:`), Shell (`!`) input · [[Help/TUI/Commands]] · `crucible-cli`
- [x] **Slash Commands** `P0` — `/quit`, `/mode`, `/plan`, `/auto`, `/normal`, `/help` handled locally; registry commands (`/search`, `/commit`, `/agent`, etc.) forwarded to agent · `crucible-cli`
- [x] **REPL Commands** `P0` — `:quit`, `:help`, `:clear`, `:model`, `:set`, `:export`, `:messages`, `:mcp`, `:config`, `:palette` · [[Help/TUI/Commands]] · `crucible-cli`
- [x] **Runtime Config** `P0` — Vim-style `:set` with enable/disable/toggle/reset/query/history (`?`, `??`, `&`, `<`) · [[Help/TUI/Commands]] · `crucible-cli`
- [x] **Double Ctrl+C Quit** `P0` — First clears input or shows warning; second within 300ms quits · `crucible-cli`

### Streaming & Display
- [x] **Streaming Display** `P0` — Real-time token streaming with cancel (Esc/Ctrl+C) · `crucible-cli`
- [x] **Streaming Graduation** `P0` — Dual-zone: viewport (live) → stdout (permanent); XOR placement, monotonic, atomic · `crucible-cli`
- [x] **Thinking Display** `P0` — Streaming thinking blocks with token count; Ctrl+T toggles; `:set thinking`; note: token count is inaccurate (counts delta messages, not actual tokens) · `crucible-cli`
- [x] **Markdown Rendering** `P0` — Full markdown-to-node rendering with styled output · `crucible-cli`, `crucible-oil`
- [x] **Context Usage Display** `P0` — Token usage (used/total) in statusline; daemon pipes prompt/completion tokens via `message_complete` event · `crucible-cli`
- [x] **Lua Statusline Bridge** `P0` — `crucible.statusline.setup()` config drives TUI `StatusBar` rendering; falls back to hardcoded layout · [[Help/Lua/Configuration]] · `crucible-cli`

### Tool & Agent Display
- [x] **Tool Call Display** `P0` — Spinner while running, smart summarization (line/file/match counts), MCP prefix stripping · `crucible-cli`
- [x] **Tool Output Handling** `P0` — Tail display (50 lines), spill to file at >10KB, parallel call tracking by call_id · `crucible-cli`
- [x] **Subagent Display** `P0` — Spawned/completed/failed tracking with elapsed time and truncated prompt · `crucible-cli`
- [x] **MCP Server Display** `P0` — `:mcp` lists servers with live connection status; `ChatAppMsg::McpStatusLoaded` updates display at runtime · `crucible-cli`

### Interaction Modals
- [x] **Permission Modal** `P0` — Allow (y), Deny (n), Allowlist (a); diff toggle (d); queued permissions auto-open · `crucible-cli`
- [x] **Ask Modal** `P0` — Single-select, multi-select (Space), free-text "other" option · `crucible-cli`
- [x] **Diff Preview** `P0` — Syntax-highlighted line/word-level diffs for file operations; collapsible · `crucible-cli`
- [x] **Permission Session Settings** `P0` — `:set perm.show_diff` controls initial diff visibility, `:set perm.autoconfirm_session` auto-approves permissions · `crucible-cli`
- [x] **Batch Ask / Edit / Show / Panel** `P0` — All 7 InteractionRequest variants (Ask, AskBatch, Edit, Show, Permission, Popup, Panel) fully implemented with key handlers, renderers, and tests · `crucible-cli`

### Autocomplete & Popups
- [x] **Autocomplete** `P0` — 9 trigger kinds: `@files`, `[[notes]]`, `/commands`, `:repl`, `:model`, `:set`, command args, F1 palette · `crucible-cli`
- [x] **Command Palette** `P0` — F1 toggle for full command discovery; selecting items executes slash/REPL commands · `crucible-cli`
- [x] **Model Lazy-Fetch** `P0` — Models loaded on first `:model` access (NotLoaded → Loading → Loaded) · `crucible-cli`

### Shell
- [x] **Shell Modal** `P0` — `!command` full-screen execution; scrollable (j/k/g/G/PgUp/PgDn); `i` inserts output · [[Help/TUI/Shell Execution]] · `crucible-cli`
- [x] **Shell History** `P0` — Last 100 commands recalled with `!` prefix · `crucible-cli`

### Notifications
- [x] **Toast Notifications** `P0` — Auto-dismiss after 3s; badge in status bar (INFO/WARN/ERROR) · `crucible-cli`
- [x] **Messages Drawer** `P0` — `:messages` toggles full notification history panel · `crucible-cli`
- [x] **Warning Badges** `P0` — Persistent count badge when warnings exist · `crucible-cli`

### Rendering Engine
- [x] **Oil Renderer** `P0` — Custom terminal rendering engine (replaced ratatui) · [[Help/TUI/Component Architecture]] · `crucible-oil`
- [x] **Taffy Layout** `P0` — Flexbox-based terminal layout engine · `crucible-oil`
- [x] **Theme System** `P0` — Token-based theming with configurable colors · [[Meta/TUI-Style-Guide]] · `crucible-oil`
- [x] **Viewport Caching** `P0` — Cached messages, tool calls, shell executions, subagents with lazy line-wrapping · `crucible-cli`
- [x] **Drawer Component** `P0` — Bordered expandable panels with title/footer badges · `crucible-cli`

### Session & Export
- [x] **Session Export** `P0` — `:export <path>` saves session as markdown via observe renderer; tilde expansion, frontmatter, thinking blocks, tool calls · `crucible-cli`
- [x] **Keybindings** `P0` — Full keybinding table (Enter, Esc, Ctrl+C, Ctrl+T, BackTab, F1, y/n/a/d in modals) · [[Help/TUI/Keybindings]] · `crucible-cli`

### In Progress / Planned
- [ ] **TUI Redesign** `P1` — Splash screen, bottom-anchored chat · [[Meta/TUI User Stories]] · `crucible-cli`
- [ ] **Chat Improvements** `P1` — Command history, session stats · `crucible-cli`

## Extensibility & Plugins

- [x] **Lua Scripting** `P0` — Lua 5.4 runtime for plugins · [[Help/Lua/Language Basics]] · [[Help/Concepts/Scripting Languages]] · `crucible-lua`
- [x] **Fennel Support** `P0` — Lisp-to-Lua compiler with macros · [[Help/Concepts/Scripting Languages]] · `crucible-lua`
- [x] **Plugin System** `P0` — Discovery, lifecycle, manifests · [[Help/Extending/Creating Plugins]] · [[Help/Extending/Plugin Manifest]] · `crucible-lua`
- [x] **Tool Annotations** `P0` — `@tool`, `@hook`, `@param` annotations for Lua functions · [[Help/Extending/Custom Tools]] · `crucible-lua`
- [x] **Event Hooks** `P0` — Note lifecycle hooks (`note:created`, `note:modified`, etc.) · [[Help/Extending/Event Hooks]] · `crucible-lua`
- [x] **Custom Handlers** `P0` — Event handler chains with priority ordering · [[Help/Extending/Custom Handlers]] · `crucible-lua`
- [x] **Oil UI DSL** `P1` — Lua/Fennel API for interaction modals (ask, popup, panel); predefined modal types, not a general component model · [[Help/Extending/Scripted UI]] · [[Help/Plugins/Oil-Lua-API]] · `crucible-lua`, `crucible-oil`
- [x] **Lua API Modules** `P0` — 20+ modules under unified `cru.*` namespace: `cru.fs`, `cru.graph`, `cru.http`, `cru.session`, `cru.shell`, `cru.timer`, `cru.ratelimit`, `cru.retry`, `cru.emitter`, `cru.check`, etc. (`crucible.*` retained as long-form alias) · `crucible-lua`
- [x] **Timer/Sleep Primitives** `P1` — `cru.timer.sleep(secs)` async sleep, `cru.timer.timeout(secs, fn)` deadline wrapper; backed by `tokio::time` · `crucible-lua`
- [x] **Rate Limiting** `P1` — `cru.ratelimit.new({ capacity, interval })` token bucket; `:acquire()` (async), `:try_acquire()` (sync), `:remaining()` · `crucible-lua`
- [x] **Retry with Backoff** `P1` — `cru.retry(fn, opts)` exponential backoff with jitter; configurable max retries, base/max delay · `crucible-lua`
- [x] **Event Emitter** `P1` — `cru.emitter.new()` minimal pub/sub; `:on(event, handler)`, `:off()`, `:emit()`, `:once()` · `crucible-lua`
- [x] **Argument Validation** `P1` — `cru.check.string()`, `.number()`, `.table()`, `.one_of()` with optional/range constraints · `crucible-lua`
- [x] **Plugin Config** `P0` — Per-plugin configuration schemas · [[Help/Lua/Configuration]] · `crucible-lua`
- [x] **Script Agent Queries** `P0` — Lua-based agent queries · [[Help/Extending/Script Agent Queries]] · `crucible-lua`
- [x] **HTTP Module** `P0` — HTTP client for plugins · [[Help/Extending/HTTP Module]] · `crucible-lua`
- [-] **Lua Integration (full)** `P1` — Complete scripting API for custom workflows and callout handlers · `crucible-lua`
- [ ] **Hook Documentation** `P1` — Comprehensive guide on extending Crucible · [[Help/Extending/Event Hooks]]

### Plugin Developer Experience

> The Discord plugin proved Crucible's plugin system works for real integrations (376-line multi-module plugin with WebSocket, REST, streaming, permissions). But the dev loop has friction: no hot reload, no IDE hints, no REPL, no scaffolding. These items close the gap between "works" and "easy to write."
>
> **Guiding insight**: Neovim's plugin ecosystem exploded when LuaLS type stubs + lazy.nvim hot reload made Lua plugins as ergonomic as TypeScript. Crucible needs the same inflection point.

- [ ] **LuaCATS Type Stubs** `P1` — Generate `---@meta` files from Rust API surface (`cru.fs`, `cru.session`, `cru.http`, etc.); ship with binary, write to `~/.config/crucible/luals/`; enables IDE autocomplete and type checking via LuaLS · `crucible-lua`
- [x] **Plugin Hot Reload** `P1` — `:reload <plugin>` command; invalidate `package.loaded`, re-require, re-extract services; `plugin.reload` RPC + `plugin.list` RPC · `crucible-lua`, `crucible-daemon`, `crucible-cli`
- [ ] **`:lua` REPL** `P1` — Evaluate Lua expressions in running daemon context; `=expr` prints result (Neovim pattern); inspect plugin state, test API calls, debug interactively · `crucible-cli`, `crucible-lua`
- [ ] **`cru plugin new`** `P1` — Scaffold plugin from template: `plugin.yaml`, `init.lua` with annotated example tool, `.luarc.json` for LuaLS, optional `tests/` directory; symlinks into plugin path · `crucible-cli`
- [ ] **Clean Error Messages** `P1` — `xpcall` wrapper strips Rust FFI frames from stack traces; errors include plugin name + file path + line number; user sees `Error in 'discord' at responder.lua:42` not raw mlua backtrace · `crucible-lua`
- [ ] **Plugin Test Harness** `P2` — `cru plugin test <name>` runs plugin tests with mock `cru.*` API; busted-compatible test runner; enables CI for plugins · `crucible-lua`, `crucible-cli`
- [ ] **`.luarc.json` Generation** `P2` — `cru plugin new` and `cru plugin init` emit LuaLS config pointing to type stubs; zero-config IDE setup · `crucible-cli`

### Plugin Abstractions (Planned)

> Extracted from building the Discord plugin (376 lines, 7 modules). These abstractions target the plugin types we expect to be most common: messaging bots, autonomous loops, content transformers, and long-running services.

- [x] **`cru.service`** `P1` — Service lifecycle for long-running plugins; declarative descriptor with `start`, `stop`, `health` hooks; config schema with automatic validation and secret resolution (`secret=true` → check `CRUCIBLE_<PLUGIN>_<KEY>` env var first); supervised restart with backoff via `cru.retry`; `cru.service.status(name)`, `cru.service.list()`, `cru.service.stop(name)` · `crucible-lua`
- [ ] **`cru.messaging`** `P2` — Adapter trait for chat platform integrations; normalizes the receive → should_respond → session → send_and_collect → format → reply loop that is identical across Discord/Telegram/Slack/Matrix; platform provides `connect()`, `normalize(raw)`, `send(channel, text)`, `typing(channel)`; framework handles session-per-channel lifecycle, chunking for platform message limits, typing indicator cadence, rate limiting; builds on `cru.service`; **extract from two concrete implementations** (Discord + one more), don't speculate the shape · `crucible-lua`
- [ ] **`cru.transform`** `P2` — Content transform pipeline; `register(name, fn)` + `pipeline({name1, name2, ...})` composes pure text→text functions; convention wrapper for table formatting, mermaid rendering, citation insertion, import normalization, platform-specific markdown cleanup; pipeline is the unit messaging adapters plug into for `format_response` · `crucible-lua`

### Fennel for Plugins

> Crucible ships both Lua and Fennel (`FennelCompiler` in `crucible-lua`). Fennel compiles to Lua with zero runtime overhead. The question is whether to **actively promote** Fennel for plugins or keep it as an opt-in power tool.

**Strengths for plugin authors:**

| Feature | Benefit | Example |
|---------|---------|---------|
| **Macros** | DSLs that eliminate boilerplate; a `defservice` or `deftool` macro could reduce a plugin to its essential logic | `(defservice :discord {:token (secret)} (fn [ctx] ...))` |
| **Pattern matching** | Cleaner event dispatch than if/elseif chains; natural fit for `MESSAGE_CREATE` / `INTERACTION_CREATE` routing | `(match event.t :MESSAGE_CREATE (handle-msg event.d) :READY (on-ready event.d))` |
| **Destructuring** | Concise argument extraction; Lua plugins repeat `local x = args.x` lines | `(fn [{: query : limit}] ...)` |
| **Immutable locals** | Fewer mutation bugs in stateful plugins (services, session managers) | `(local config (validate schema opts))` — can't accidentally reassign |
| **Data literal syntax** | Tables-as-data read naturally; good for config, schemas, API payloads | `{:name "discord" :capabilities [:network :websocket]}` |
| **Lisp composition** | Threading macros (`->`, `->>`) make transform pipelines readable | `(->> text (strip-mentions) (transform-tables) (chunk 2000))` |

**Weaknesses for plugin authors:**

| Issue | Impact | Mitigation |
|-------|--------|------------|
| **LuaLS doesn't understand Fennel** | Type stubs, autocomplete, diagnostics — all DX investments are Lua-only; Fennel devs get no IDE support | Fennel LSP (`fennel-ls`) exists but immature; alternatively, generate Fennel type stubs alongside Lua ones |
| **Smaller community** | Fewer examples, less Stack Overflow help, harder to onboard contributors | Good docs + example plugins can compensate; Fennel community is small but high-quality |
| **Compilation indirection** | Error line numbers reference compiled Lua, not source Fennel; debugging is harder | Fennel has source maps; `FennelCompiler` could propagate them |
| **Parenthetical syntax** | Polarizing; barrier for developers without Lisp experience | Keep Lua as default; Fennel is opt-in for those who prefer it |
| **Hot reload complexity** | Fennel files need recompilation before reload; adds a step vs. pure Lua | `FennelCompiler` already handles this; `:reload` command should compile-then-load transparently |
| **Macro debugging** | Macros can produce opaque errors; `macrodebug` helps but adds friction | Document macro patterns; keep macros simple |

**Recommendation:** Keep Fennel as an **opt-in power tool**, not the default path. Lua examples first in all docs, Fennel alternatives shown alongside. Invest in Fennel-specific DX only after Lua DX is solid (type stubs, hot reload, REPL all working). The macro system is genuinely valuable for reducing plugin boilerplate — a `defservice` macro alone could justify Fennel for service plugin authors. But the LuaLS gap means Fennel developers trade IDE ergonomics for language ergonomics; that's an informed choice, not a default.

## Agent Protocols (ACP & MCP)

### ACP Host (Crucible → External Agents)

Crucible acts as an **ACP host**, spawning and controlling external AI agents (Claude Code, Codex, Cursor, Gemini CLI, OpenCode) with Crucible's memory, context, and permission system.

- [x] **ACP Host** `P0` — Spawn and control ACP agents with JSON-RPC 2.0 over stdio; capability negotiation, plan/act modes · [[Help/Concepts/Agents & Protocols]] · `crucible-acp`
- [x] **Context Injection** `P0` — `PromptEnricher` performs semantic search, wraps results in `<precognition>` blocks; configurable via `ContextConfig::inject_context` · `crucible-acp`
- [x] **In-Process MCP Host** `P0` — SSE transport MCP server running in-process; agents discover Crucible tools without external server · `crucible-acp`
- [x] **Agent Discovery** `P0` — Parallel probing of known agents (`claude-code`, `opencode`, `cursor-acp`, `gemini`); env var overrides · `crucible-acp`
- [x] **Sandboxed Filesystem** `P0` — Path validation, traversal prevention, mode-based permissions (plan=read-only), configurable size limits · `crucible-acp`
- [x] **Permission Gate** `P0` — `PermissionGate` trait for pluggable permission decisions; `list_directory` in `FileSystemHandler` · `crucible-acp`
- [x] **Streaming Responses** `P0` — Chunk processing, tool call parsing from stream, diff handling · `crucible-acp`
- [x] **Session Management** `P0` — UUID sessions with config (cwd, mode, context size), history with ACP roles, persistence across reconnections · `crucible-acp`

### ACP Agent (Future — Crucible as Embeddable Agent)

- [ ] **ACP Agent Mode** `P1` — Crucible as an embeddable ACP agent; any ACP host (Zed, JetBrains, Neovim) spawns Crucible to get knowledge graph + memory · `crucible-acp`
- [ ] **ACP Registry Submission** `P1` — Agent manifest for [ACP Registry](https://github.com/agentclientprotocol/registry); one PR → available in all ACP clients · `crucible-acp`
- [ ] **ACP Schema Bump** `P1` — Bump `agent-client-protocol-schema` from 0.10.6 → 0.10.7; SDK crate (`agent-client-protocol` 0.9.3) and wire protocol (v1) are already current · `crucible-acp`, `crucible-core`

### MCP Server (External Agents → Crucible Tools)

- [x] **MCP Server** `P0` — Expose kiln as MCP tools for external AI agents · [[Help/Concepts/Agents & Protocols]] · `crucible-tools`
- [x] **Note Tools** `P0` — `create_note`, `read_note`, `read_metadata`, `update_note`, `delete_note`, `list_notes` · `crucible-tools`
- [x] **Search Tools** `P0` — `semantic_search`, `text_search`, `property_search` · `crucible-tools`
- [x] **Kiln Tools** `P0` — `get_kiln_info` · `crucible-tools`
- [x] **Workspace Tools** `P0` — `read_file`, `edit_file`, `write_file`, `bash`, `glob`, `grep` · `crucible-tools`
- [x] **TOON Formatting** `P0` — Token-efficient response formatting · `crucible-tools`

### MCP Gateway (Crucible → Upstream MCP Servers)

- [x] **MCP Gateway** `P0` — Connect upstream MCP servers with prefixed tool names · [[Help/Extending/MCP Gateway]] · [[Help/Config/mcp]] · `crucible-tools`
- [x] **Lua Plugin Tools** `P0` — Dynamic tool discovery from Lua plugins · `crucible-tools`, `crucible-lua`
- [x] **MCP Bridge/Gateway** `P0` — `McpGatewayManager` shared in daemon, `McpProxyTool` dynamic injection, live status display · `crucible-tools`
- [x] **MCP Connection Stability** `P0` — Auto-reconnect loop, 30s SSE keepalive, live status indicators in TUI · `crucible-acp`

## Distribution & Growth

> How Crucible reaches users and spreads. Ordered by growth impact.
>
> **Insight from OpenClaw analysis (2026-02):** Viral growth came from instant install, meeting users in apps they already use, and proactive behavior. Crucible's counter-position: "Your AI should live in your notes, not a chat app you don't control."

### Install & Onboarding (P0 — #1 adoption blocker)

- [x] **One-Line Install** `P0` — Pre-built binaries via GitHub Releases (linux x86_64/aarch64, macOS Intel/Apple Silicon); `curl|sh`, `brew install mootikins/crucible/crucible`, `cargo binstall crucible-cli`; target: working `cru` binary in <60 seconds · `crucible-cli`
- [x] **Precognition Default-On** `P0` — Changed default from opt-in to on; the knowledge-graph-aware context is the core differentiator · `crucible-cli`

### HTTP Gateway (P1 — platform layer for everything external)

> The daemon is Unix-socket-only (JSON-RPC 2.0). Messaging bots, webhook triggers, web UI, and any external client all need HTTP access. This is the shared foundation — wire `crucible-web` to `DaemonClient` and expose the daemon's 55 RPC methods over HTTP + SSE/WebSocket for events.

```
HTTP Gateway (crucible-web wired to daemon)
    ├── Messaging bots (Telegram, Discord)
    ├── Webhook endpoints (POST /api/webhook/:name)
    └── Web UI (SolidJS frontend on same server)
         └── Remote access (Tailscale / Cloudflare Tunnel)
```

- [x] **HTTP-to-RPC Bridge** `P1` — Wire `DaemonClient` into `crucible-web` Axum routes; translate HTTP requests to daemon JSON-RPC calls · `crucible-web`, `crucible-rpc`
- [x] **SSE/WebSocket Event Bridge** `P1` — Subscribe to daemon session events, stream to HTTP clients via SSE; `EventBroker` fans out per-session events · `crucible-web`
- [x] **Chat HTTP API** `P1` — `POST /api/chat/send` + `GET /api/chat/events/:session_id` SSE stream; `POST /api/session`, `/list`, `/:id/pause`, `/:id/resume`, `/:id/end` · `crucible-web`
- [x] **Search HTTP API** `P1` — `POST /api/search/vectors`; `GET /api/notes`, `GET /api/notes/:name`; `GET /api/kilns` · `crucible-web`
- [ ] **API Auth** `P1` — Bearer token or API key auth; required the moment the gateway is exposed beyond localhost · `crucible-web`
- [ ] **Webhook API** `P1` — `POST /api/webhook/:name` triggers named Lua handlers; enables GitHub webhooks, calendar events, IFTTT/Zapier/n8n integration; enriches payloads with kiln context (uniquely Crucible) · `crucible-web`, `crucible-lua`

### Messaging Integrations (P1 — meet users where they are)

> 1-2 good messaging integrations reduce the need for a web UI substantially. Users interact daily in messaging apps; Crucible meets them there and delivers proactive kiln insights. Integrations can be daemon-side Lua plugins (Discord) or thin adapters over the HTTP gateway (Telegram, Matrix).

- [ ] **Telegram Bot** `P1` — Bot API adapter over HTTP gateway; lowest friction (HTTP API, no app store approval, huge dev audience); enables proactive digest delivery · `crucible-telegram` (new crate) · depends: [[#HTTP Gateway|HTTP-to-RPC Bridge]]
- [x] **Discord Plugin** `P1` — Discord integration (REST API + Gateway) as daemon-side Lua plugin; tools: `discord_send`, `discord_read`, `discord_channels`, `discord_register_commands`; commands: `:discord connect/disconnect/status` · `plugins/discord/`
- [ ] **Matrix Bridge** `P2` — Matrix protocol integration; strong overlap with self-host/privacy audience · `crucible-matrix` (new crate) · depends: [[#HTTP Gateway|HTTP-to-RPC Bridge]]

### Remote Access (P2 — self-hosting for everyone)

> Agents can't be on every device. Self-hosting with easy remote access is more aligned with "local-first, your notes, your control" than paid cloud hosting. Cloudflare Tunnel / Tailscale Funnel provide zero-config encrypted remote access to a locally-running daemon.

- [ ] **`cru tunnel`** `P2` — One-command remote access setup; wraps `cloudflared tunnel` or `tailscale funnel`; exposes HTTP gateway with auth to user's devices · `crucible-cli`
- [ ] **Cloudflare Tunnel Integration** `P2` — `cru tunnel --cloudflare`; auto-configures `cloudflared` with API auth; free tier for personal use · `crucible-cli`
- [ ] **Tailscale Funnel Integration** `P2` — `cru tunnel --tailscale`; WireGuard encrypted, ACL-gated; zero-config for Tailscale users · `crucible-cli`
- [ ] **Paid Hosting** `P?` — Multi-tenant hosted option (OpenClaw model); needs daemon isolation, user management, billing; defer until clear demand · future

### Proactive Behavior (P2 — viral feature)

> OpenClaw's most praised feature was the heartbeat — the agent reaching out unprompted. Crucible can do this better because it has a knowledge graph, not flat memory. Heartbeat is time-based; webhook triggers are event-driven — Crucible can do both.

- [ ] **Kiln Digest** `P2` — Periodic scan of recent kiln changes; surface missed connections ("You wrote about X in two notes this week — want me to link them?"); delivered via messaging integration or TUI notification · `crucible-daemon`, `crucible-lua`
- [ ] **Scheduled Lua Hooks** `P2` — Cron-style callbacks for Lua plugins; enables daily briefings, orphan note detection, task reminders from `- [ ]` items · `crucible-lua`, `crucible-daemon`
- [ ] **Daily Briefing Plugin** `P2` — Reference plugin: summarize recent kiln changes, pending tasks, orphaned notes; delivered via messaging or shown on TUI startup · `crucible-lua`

### Default Runtime Plugins (P1 — Neovim-style bundled plugins)

> Crucible ships a `runtime/` directory of Lua plugins alongside the binary, analogous to Neovim's `$VIMRUNTIME/plugin/`. These load automatically, are overridable, and their source code *is* the documentation for how to build plugins.
>
> **Plugin search path (priority order):**
> 1. `CRUCIBLE_PLUGIN_PATH` — env override (highest priority)
> 2. `~/.config/crucible/plugins/` — user global
> 3. `KILN/.crucible/plugins/` — kiln personal (gitignored)
> 4. `KILN/plugins/` — kiln shared (version-controlled)
> 5. `$CRUCIBLE_RUNTIME/plugins/` — bundled default (lowest priority)
>
> Same-named user plugin at any higher path **shadows** the runtime version. `:plugins` shows provenance: `[core]`, `[runtime]`, `[user]`, `[kiln]`.

- [ ] **Runtime Plugin Infrastructure** `P1` — `$CRUCIBLE_RUNTIME/plugins/` path added to plugin discovery; provenance tracking in `:plugins` display; shadow-by-name semantics · `crucible-lua`, `crucible-cli`
- [ ] **`entity-memory` Runtime Plugin** `P1` — Extract entities/facts from conversations → atomic zettelkasten notes; wikilinks to source sessions; deduplicates against existing entity notes · `runtime/entity-memory/`
- [ ] **`session-digest` Runtime Plugin** `P1` — Summarize completed sessions → linked notes; captures decisions, topics, entities; wikilinks to entity and source notes · `runtime/session-digest/`

### Ecosystem & Shareability (P1-P2)

- [ ] **Plugin Install** `P1` — `cru plugin add <git-url>` or `cru plugin add <name>`; Git-native distribution (lazy.nvim model, not centralized marketplace) · `crucible-lua`, `crucible-cli`
- [ ] **Agent Memory Branding** `P1` — Rename "Precognition" to "Agent Memory" in user-facing docs; communicates the value proposition directly · docs
- [ ] **`cru share`** `P2` — Export sessions as self-contained HTML or shareable artifacts; `:export` exists for local markdown, this adds sharable formats · `crucible-cli`
- [ ] **Graph Visualization** `P2` — Shareable knowledge graph renders (SVG/HTML); creates viral demo moments ("look at my AI-connected notes") · `crucible-cli` or `crucible-web`

## Workflow Automation

- [ ] **Markdown Handlers** `P2` — Event handlers in pure markdown, inject context into agents · [[Help/Extending/Markdown Handlers]] · depends: [[#Extensibility & Plugins]]
- [ ] **Workflow Markup** `P2` — DAG workflows in markdown (`@agent`, `->` data flow, `> [!gate]`) · [[Help/Workflows/Workflow Syntax]] · [[Help/Workflows/Markup]]
- [ ] **Workflow Sessions** `P2` — Log execution as markdown, resume interrupted work · [[Help/Workflows/Index]]
- [ ] **Session Learning** `P2` — Codify successful sessions into reusable workflows
- [ ] **Parallel Execution** `P2` — `(parallel)` suffix or `&` prefix for concurrent steps
- [ ] **Workflow Authoring** `P2` — Guide for creating workflows · [[Help/Extending/Workflow Authoring]]

## Storage & Processing

- [x] **SQLite Backend** `P0` — Default storage; fast, lightweight, recommended for most users · [[Help/Config/storage]] · `crucible-sqlite`
- [x] **SurrealDB Backend** `P0` — Advanced storage with EAV graph and RocksDB engine; richer queries, higher memory · [[Help/Config/storage]] · _(removed)_
- [x] **Vector Embeddings** `P0` — FastEmbed (ONNX) local embedding generation · [[Help/Config/embedding]] · `crucible-llm`
- [x] **Embedding Reranking** `P0` — Search result reranking for relevance · `crucible-sqlite`
- [x] **File Processing** `P0` — Parse, enrich, and index notes via pipeline · [[Help/CLI/process]] · `crucible-daemon`
- [x] **Transaction Queue** `P0` — Batched database operations with consistency · `crucible-sqlite`
- [x] **Hash-based Change Detection** `P0` — Content-addressable block hashing · `crucible-core`
- [x] **Task Storage** `P0` — Task records, history, dependencies, file associations · `crucible-sqlite`
- [x] **Kiln Statistics** `P0` — Note counts, link analysis, storage metrics · [[Help/CLI/stats]] · `crucible-cli`
- [x] **Daemon Server** `P0` — Unix socket server with 35 RPC methods · `crucible-daemon`
- [x] **Daemon Client** `P0` — Auto-spawn, reconnect, RPC client library · `crucible-rpc`
- [x] **Event Subscriptions** `P0` — Per-session and wildcard event streaming via daemon · `crucible-daemon`
- [x] **Notification RPC** `P0` — Add, list, dismiss notifications via daemon · `crucible-daemon`
- [x] **File Watching** `P0` — File change detection (notify/polling, debouncing, daemon bridge) with auto-reprocessing: `file_changed` events trigger `pipeline.process()` via daemon reprocess task; enrichment disabled for now (parsing + storage only) · `crucible-watch`, `crucible-daemon`
- [ ] **Burn Embeddings** `P?` — Burn ML framework for local embeddings (stubbed) · `crucible-llm`
- [ ] **LlamaCpp Embeddings** `P?` — GGUF model inference for embeddings (stubbed) · `crucible-llm`
- [ ] **Session Compaction** `P?` — Compact sessions with cache purge for memory efficiency · `crucible-daemon`

## Configuration & Setup

- [x] **Config System** `P0` — TOML config with profiles, includes, environment overrides · [[Help/Configuration]] · `crucible-config`
- [x] **Provider Config** `P0` — Unified provider configuration (Ollama, OpenAI, Anthropic) · [[Help/Config/llm]] · `crucible-config`
- [x] **Embedding Config** `P0` — Provider, model, batch size, concurrency settings · [[Help/Config/embedding]] · `crucible-config`
- [x] **Storage Config** `P0` — Backend selection, embedded vs daemon mode · [[Help/Config/storage]] · `crucible-config`
- [x] **MCP Config** `P0` — Upstream MCP server connections · [[Help/Config/mcp]] · `crucible-config`
- [x] **Workspace Config** `P0` — Multi-workspace kiln associations · [[Help/Config/workspaces]] · `crucible-config`
- [x] **Agent Config** `P0` — Default agent, temperature, max_tokens, thinking budget · [[Help/Config/agents]] · `crucible-config`
- [x] **CLI Commands** `P0` — 16 command modules: chat, session, process, search, stats, config, etc. · [[Help/CLI/Index]] · `crucible-cli`
- [x] **Init Command** `P0` — Project initialization (`cru init`) with path validation · `crucible-cli`
- [x] **Setup Wizard** `P0` — Oil TUI first-run wizard: auto-triggers on `cru chat` when no kiln exists; guides through kiln path, provider detection, model selection · `crucible-cli`
- [x] **Kiln Discovery** `P0` — Git-like upward `.crucible/` search with priority: CLI flag → ancestor walk → env var → global config · `crucible-cli`
- [x] **Kiln Path Validation** `P0` — Shared validation layer: hard blocks (root, nested kiln), strong warnings (git repo, source project, home dir, tmp), mild warnings (cloud sync) · `crucible-cli`
- [x] **Getting Started** `P0` — Installation and first steps guide · [[Guides/Getting Started]] · [[Guides/Your First Kiln]]
- [x] **Platform Guides** `P0` — Windows setup, GitHub Copilot integration · [[Guides/Windows Setup]] · [[Guides/GitHub Copilot Setup]]
- [-] **CLI Help & Discoverability** `P0` — `--help` completeness, command suggestions · `crucible-cli`
- [x] **Plugin Loading Errors** `P0` — `:plugins` command shows load status; failures surfaced as toast notifications with error details · `crucible-lua`, `crucible-cli`

## Web & Desktop

> Builds on the HTTP gateway (P1). The web UI is a **thin client to the daemon** (same as TUI — just a different renderer). Serve over Tailscale/Cloudflare Tunnel for self-hosted remote access; PWA for mobile without app store friction.
>
> **Design principles** (informed by OpenClaw, Codex Desktop, Claude Artifacts, Obsidian):
> 1. **Gateway-centric** — all state lives in daemon; web is stateless view layer (matches OpenClaw's architecture)
> 2. **Agent inbox first** — single place to see all running agents, approve permissions, review results (Codex's command center pattern)
> 3. **Knowledge graph is the differentiator** — visual graph exploration that no competitor has in-browser
> 4. **Easy primitives** — Lua plugins can define UI panels, not just tools; agent-driven UI (like OpenClaw's A2UI / Claude's Artifacts)
> 5. **Good API docs** — interactive playground for the HTTP API; self-documenting

### Rust Primitives (P1 — the 5 things core must provide)

> Neovim insight: core provides primitives (buffers, windows, events, highlights). Plugins compose them. These are Crucible's web equivalents.

- [-] **HTTP → RPC Bridge** `P1` — Proxy daemon JSON-RPC methods to REST + SSE endpoints; thin translation layer, no domain logic · **Core Rust** · `crucible-web` · depends: [[#HTTP Gateway|HTTP-to-RPC Bridge]]
- [ ] **SSE Event Streaming** `P1` — Stream chat tokens, log lines, and daemon events to browser via Server-Sent Events; backpressure handling · **Core Rust** · `crucible-web`
- [ ] **Oil Node Serialization** `P1` — `impl Serialize for Node` — Oil nodes to JSON for browser rendering; foundational primitive for all rich display · **Core Rust** · `crucible-oil`
- [ ] **Plugin Panel Hosting** `P1` — iframe sandbox + message-passing protocol for Lua-registered web panels; the "floating window" primitive that P2 features compose on · **Core Rust** · `crucible-web`, `crucible-lua`
- [ ] **Static File Serving** `P1` — Serve SolidJS bundle, PWA manifest, service worker; infrastructure · **Core Rust** · `crucible-web`

### Foundation UI (P1 — ships with HTTP gateway)

- [-] **Web Chat UI** `P1` — SolidJS frontend: streaming chat, markdown rendering, tool output cards, permission modals; Rust owns SSE streaming + message framing, SolidJS owns rendering, Lua can extend tool card definitions · **Hybrid** · `crucible-web`
- [-] **Flexible Panel System** `P1` — 4-edge dockable layout (left, right, bottom, center) with collapsible panels; drag-and-drop positioning; layout persistence to localStorage · **SolidJS** · `crucible-web`
- [-] **Breadcrumb Navigation** `P1` — Project dropdown with selection, Session dropdown with search, New Session button; dark theme header bar · **SolidJS** · `crucible-web`
- [-] **File Tree** `P1` — Ark UI Collapsible-based tree with Workspace files + Kiln notes sections; extension-based file icons; loading/error states · **SolidJS** · `crucible-web`
- [-] **CodeMirror 6 Editor** `P1` — Multi-file tabs with dirty indicator; language detection (markdown, rust, typescript, javascript); one-dark theme; load/save via API · **SolidJS** · `crucible-web`
- [-] **Model Picker** `P1` — Cursor-style dropdown below textarea; shows available models; switch model during conversation · **SolidJS** · `crucible-web`
- [-] **Session Auto-Naming** `P1` — Auto-generates title from first user message; never overwrites existing titles · **SolidJS** · `crucible-web`
- [ ] **Agent Inbox / Overview** `P1` — Dashboard of active sessions, pending permissions, recent completions; the landing page; composes existing `session.list` + event subscription RPCs · **Lua extension** · `crucible-web`
- [ ] **Permission Approval UI** `P1` — Approve/deny from browser with diff preview; Rust owns the approval RPC (security-critical), Lua owns diff formatting + approval policy hooks · **Hybrid** · `crucible-web`
- [-] **Session Management** `P1` — List, switch, resume, end sessions; pure HTTP→RPC proxy, already exists as daemon methods · **Core Rust** · `crucible-web`
- [ ] **PWA Support** `P1` — Manifest + service worker; installable from browser, mobile access without app store · **Core Rust** · `crucible-web`
- [ ] **SolidJS Oil Renderer** `P1` — `<OilNode>` component tree for browser; foundational rendering like Neovim's terminal grid — everything else depends on it · **Core Rust** (frontend) · `crucible-web`, `crucible-oil`

### Knowledge & Search (P2 — Crucible's unique strength)

- [ ] **Knowledge Graph Visualization** `P2` — Interactive force-directed wikilink graph; Lua provides data query, SolidJS + d3 renders; no new Rust primitives needed · **Lua extension** · `crucible-web`
- [ ] **Note Browser** `P2` — Browse notes with frontmatter, wikilinks, backlinks; users want custom columns/sort/filters — classic plugin territory · **Lua extension** · `crucible-web`
- [ ] **Search UI** `P2` — Unified semantic + full-text + property search; Rust owns the search RPC (perf-critical), Lua owns result formatting + custom scopes · **Hybrid** · `crucible-web`
- [ ] **Structured Data Views** `P3` — Obsidian Bases-style tables/kanban from frontmatter; the canonical "plugin not core" feature; depends on mature plugin system · **Lua extension** · `crucible-web`, `crucible-query`

### Plugin UI & Artifacts (P2 — easy primitives to _make_ things)

- [ ] **Agent Artifacts** `P2` — Agent responses produce rendered outputs (code, documents, diagrams) in side panel; artifact extraction is domain logic, rendering uses panel system · **Lua extension** · `crucible-web`
- [ ] **Skills Browser** `P2` — Browse/enable/disable plugins with documentation; CRUD over plugin registry · **Lua extension** · `crucible-web`, `crucible-lua`
- [ ] **Rich Content Renderers** `P2` — Mermaid diagrams, LaTeX, syntax highlighting; each renderer is a plugin registering a content-type handler · **Lua extension** · `crucible-web`

### Configuration & System (P2)

- [ ] **Config Editor** `P2` — Schema-driven form for `config.toml`; form UI is plugin work, schema generated from config types · **Lua extension** · `crucible-web`, `crucible-config`
- [ ] **OpenAPI Spec** `P2` — Machine-readable API spec file (generated from routes); ship the spec, let users use Swagger UI / curl / httpie — zero maintenance vs custom playground · **Core Rust** · `crucible-web`
- [ ] **System Info** `P2` — Daemon health, kilns, MCP status, plugin status, embedding stats; health checks over existing RPCs · **Lua extension** · `crucible-web`
- [ ] **Log Viewer** `P2` — Real-time daemon log streaming; Rust owns the SSE log endpoint (backpressure), Lua owns filtering/formatting · **Hybrid** · `crucible-web`

### Canvas & Desktop (P3)

- [ ] **Canvas** `P3` — Infinite spatial workspace for notes + agent sessions; massive scope, low adoption in note apps; if built, it's a plugin with a custom panel · **Lua extension** · `crucible-web`
- [ ] **Workflow Visual Editor** `P3` — DAG editor for workflow markup; domain logic over workflow system · **Lua extension** · `crucible-web` · depends: [[#Workflow Automation]]
- [ ] **Tauri Desktop** `P3` — Native desktop app wrapping web UI; menu bar agent status, system notifications · **Core Rust** · depends: [[#Web & Desktop|Web Chat UI]]

## Collaboration & Scale

- [ ] **Sync System** `P4` — Merkle diff + CRDT for multi-device synchronization
- [ ] **Concurrent Agent Access** `P4` — Multiple agents accessing a kiln simultaneously · `crucible-daemon`
- [ ] **Shared Memory** `P4` — Worlds/Rooms for collaborative cognition
- [ ] **Federation** `P4` — A2A protocol for cross-kiln agent communication

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
| 2026-02-02 | ACP-first distribution | ACP enables context injection that MCP cannot; MCP = demo entry point, ACP host = current value, ACP agent = registry distribution |
| 2026-02-02 | Lua primitives over bespoke features | Autonomous loops, fan-out, automations are Lua plugins — not built-in features; matches Neovim philosophy |
| 2026-02-02 | ACP direction clarified | Crucible is ACP host (controls agents), not ACP agent; embeddable agent mode is future P1 work for registry distribution |
| 2026-02-03 | One-line install promoted to P0 | #1 adoption blocker; OpenClaw's `npm install -g` is the bar to beat |
| 2026-02-03 | HTTP gateway as P1 platform layer | Daemon is Unix-socket-only; messaging bots, webhooks, web UI all need HTTP; wire crucible-web to DaemonClient as shared foundation |
| 2026-02-03 | Webhook API at P1 | `POST /api/webhook/:name` triggers Lua handlers; enables GitHub/calendar/IFTTT integration; enriches with kiln context — uniquely Crucible vs OpenClaw's time-based heartbeat |
| 2026-02-03 | Messaging integrations (Telegram, Discord) at P1 | Meet users where they are; thin adapters over HTTP gateway; 1-2 channels reduce need for web UI substantially |
| 2026-02-03 | Messaging → web progression | Messaging is precursor work to web; HTTP gateway serves both; web adds richer interactions (graph viz, config, workflow) later |
| 2026-02-03 | Remote access via Cloudflare/Tailscale at P2 | Agents can't be on every device; `cru tunnel` wraps cloudflared/tailscale funnel; self-host > paid hosting for positioning |
| 2026-02-03 | Paid hosting deferred | Multi-tenant needs daemon isolation, billing, ops; defer until demand is clear; free self-host + tunnel is more aligned with local-first positioning |
| 2026-02-03 | Obsidian plugin dropped | HTTP gateway + messaging + web covers the progression; Obsidian plugin is a separate TypeScript project with maintenance burden for a subset of users |
| 2026-02-03 | Web UI core promoted to P1, rich features at P2 | Chat + agent inbox + permission approval are foundational (ship with HTTP gateway); knowledge graph viz, config editor, plugin panels at P2; canvas/desktop at P3. Informed by OpenClaw (gateway-centric thin client), Codex Desktop (multi-agent command center), Claude Artifacts (side-panel rendered output), Obsidian Bases/Canvas (structured data + spatial workspace) |
| 2026-02-03 | Agent inbox is web UI landing page | Codex Desktop's key insight: users need a command center to supervise multiple agents, not just a chat window. The inbox shows all sessions, pending permissions, recent completions |
| 2026-02-03 | Plugin-defined web panels at P2 | Lua plugins should be able to register browser panels (HTML/JS), not just TUI modals. This is the "easy primitives to make UI" principle — inspired by OpenClaw's A2UI and Claude Artifacts |
| 2026-02-03 | Knowledge graph viz is the web "wow" feature | No competitor renders a knowledge graph in-browser. Crucible's wikilink graph + semantic search is the differentiator; visual exploration is the demo moment |
| 2026-02-03 | ~~Web UI deprioritized to P3~~ | ~~Messaging covers daily interaction; web serves richer interactions later, via Tailscale for privacy, PWA for mobile~~ (superseded: core web UI promoted to P1) |
| 2026-02-03 | Precognition should default to on | Core differentiator shouldn't be opt-in; knowledge-graph-aware context is the product's value proposition |
| 2026-02-03 | Proactive kiln digest at P2 | Matches OpenClaw's most viral feature (heartbeat) using Crucible's strength (knowledge graph); delivered via messaging integrations |
| 2026-02-03 | Discord as daemon-side plugin, not separate crate | Direct REST API + Gateway integration via Lua plugin with `config`, `network`, `websocket` capabilities; avoids HTTP gateway dependency; validates plugin system for real integrations |
| 2026-02-03 | Plugin DX (type stubs, hot reload, REPL, scaffolding) at P1 | Discord plugin proved the system works but exposed dev loop friction: daemon restart per change, no IDE hints, no interactive debugging. Neovim's plugin ecosystem inflection point was LuaLS stubs + lazy.nvim hot reload; Crucible needs the same. Type stubs and hot reload are highest priority; test harness at P2 |
| 2026-02-03 | `cru.tools.call` is highest-priority plugin primitive | Low effort (tools exist on Rust side, this is Lua binding + permission check) but unlocks the most new plugin categories: autonomous loops, smart importers, digest generators. Without it, plugins can only react to events — not do intelligent work |
| 2026-02-03 | `cru.service` lifecycle before `cru.messaging` adapter | Service lifecycle (start/stop/health, supervised restart, config validation) is the foundation messaging adapters compose on. Discord, Telegram, calendar pollers, auto-linkers are all services. Build the general pattern first |
| 2026-02-03 | `cru.messaging` adapter: extract from two implementations | Don't speculate the adapter shape from Discord alone. Build Telegram or Matrix adapter, then extract the common trait. The receive→respond→format loop is identical; platform variance is in transport and API shape |
| 2026-02-03 | Fennel is opt-in power tool, not default path | Lua examples first in all docs; Fennel shown alongside. Macros are genuinely valuable (`defservice`, `deftool` could halve plugin boilerplate) but LuaLS doesn't understand Fennel — devs trade IDE ergonomics for language ergonomics. Invest in Fennel DX only after Lua DX is solid |
| 2026-02-05 | Agent learning as notes, not opaque DB | Informed by Agno framework analysis. Agno uses six opaque DB-backed learning stores. Crucible's approach: agent memory stored as wikilinked atomic notes (zettelkasten-style) in kiln. Human-readable, editable, searchable via existing graph. Strictly better — same capabilities plus human review/edit. Local embeddings + metadata (wikilinks, tags, frontmatter) provide rich fetchable context without custom storage |
| 2026-02-05 | Two-tier extensibility: core Rust + runtime Lua | Core features (precognition, auto-linking, team patterns) in Rust for performance, toggleable via `:set`. Higher-level behaviors (entity-memory, session-digest) as default runtime Lua plugins. Both show in `:plugins` with provenance. Matches Neovim's architecture: C core + bundled Lua plugins |
| 2026-02-05 | Default runtime plugins (Neovim-style) | Ship bundled Lua plugins at `$CRUCIBLE_RUNTIME/plugins/`; lowest-priority in discovery path; user plugins shadow by name. Source code serves as reference documentation. Plugins: `entity-memory` (facts → atomic notes), `session-digest` (session summaries → linked notes) |
| 2026-02-05 | Team patterns as core Rust, not Lua plugins | Multi-agent orchestration (supervisor, router, broadcast) is fundamental infrastructure, not optional behavior. Implemented in Rust, builds on existing subagent spawning. Configurable via `:set team.default_pattern`. Lua hooks can intercept delegation decisions |
| 2026-02-05 | SQLite is default storage | Product map updated to reflect SQLite as default, SurrealDB as advanced option. Docs (CLAUDE.md, Systems.md) corrected. SQLite is fast, lightweight, recommended for single-user local-first usage |

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

## Links

- [[Meta/Roadmap]] — Phase-based development timeline
- [[Meta/Tasks]] — Active operational tasks
- [[Meta/backlog]] — Development backlog
- [[Meta/Systems]] — System architecture and boundaries
- [[Meta/TUI User Stories]] — TUI requirements
- [[Meta/Plugin User Stories]] — Plugin requirements
- [[Meta/Plugin API Sketches]] — Plugin API designs
