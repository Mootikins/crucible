---
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

- [x] **Semantic Search** `P0` — Vector similarity search with reranking · [[Help/Concepts/Semantic Search]] · `crucible-surrealdb`, `crucible-llm`
- [x] **Full-text Search** `P0` — Fast text search across all notes · [[Help/CLI/search]] · `crucible-surrealdb`
- [x] **Knowledge Graph** `P0` — Wikilink-based graph structure and traversal · [[Help/Concepts/The Knowledge Graph]] · `crucible-surrealdb`
- [x] **Query System** `P0` — Structured note queries with composable pipeline · [[Help/Query/Query System]] · `crucible-query`
- [x] **Property Search** `P0` — Search notes by frontmatter properties and tags · `crucible-tools`
- [x] **Document Clustering** `P0` — Heuristic clustering and MoC detection · `crucible-surrealdb`
- [ ] **K-Means Clustering** `P2` — K-means implementation (placeholder stub, needs ndarray or similar) · `crucible-surrealdb`
- [x] **Block-level Embeddings** `P0` — Paragraph-granularity semantic indexing · `crucible-llm`, `crucible-surrealdb`
- [x] **Precognition** `P0` — Auto-RAG: inject relevant kiln/session context before each agent turn; should default to on (`:set precognition`); the core differentiator — every conversation is knowledge-graph-aware · `crucible-cli`, `crucible-acp`
- [x] **Session Search** `P0` — Past conversations indexed and searchable via session indexing pipeline; `cru session reindex` for batch processing · `crucible-observe`, `crucible-cli`

## AI Chat & Agents

### Conversation & Sessions
- [x] **Interactive Chat** `P0` — Conversational AI with streaming text, thinking, tool calls, and subagent events · [[Help/CLI/chat]] · `crucible-cli`, `crucible-rig`
- [x] **Agent Cards** `P0` — Configurable agent personas with system prompts, model, temperature, tools · [[Help/Extending/Agent Cards]] · [[Help/Config/agents]] · `crucible-config`
- [x] **Session Persistence** `P0` — Conversations saved as markdown + JSONL in kiln · [[Help/Core/Sessions]] · `crucible-daemon`
- [x] **Session Resume** `P0` — Load and continue previous sessions with full history · [[Help/Core/Sessions]] · `crucible-daemon`, `crucible-daemon-client`
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
- [x] **Context Enrichment** `P0` — Inject kiln context into agent conversations · `crucible-context`, `crucible-enrichment`
- [x] **File Attachment** `P0` — `@file` context attachment in chat · `crucible-cli`
- [x] **Rules Files** `P0` — Project-level AI instructions (`.crucible/rules`) · [[Help/Rules Files]] · `crucible-config`

### Lua Session API
- [x] **Scripted Agent Control** `P0` — Lua API for temperature, max_tokens, thinking_budget, model, mode; daemon getters use local cache · `crucible-lua`
- [x] **Session Event Handlers** `P0` — Lua hooks on `turn:complete` can inject follow-up messages · `crucible-lua`, `crucible-daemon`

### Lua Session Primitives (Planned)

> These fill gaps so autonomous loops, fan-out, and context control are trivial plugins — not bespoke features.

- [ ] **`session.messages()`** `P1` — Read conversation history from Lua; enables context windowing, summarization, checkpoint detection · `crucible-lua`
- [ ] **`session.inject(role, content)`** `P1` — Insert messages mid-conversation; enables fan-out result collection, context injection at checkpoints · `crucible-lua`
- [ ] **`session.fork()`** `P1` — Branch conversation state; enables parallel exploration, A/B approach testing · `crucible-lua`
- [ ] **`subagent.collect(ids)`** `P1` — Await multiple subagents and collect results; enables fan-out patterns · `crucible-lua`

### In Progress / Planned
- [x] **MCP Tool System** `P0` — Permission prompts via `PermissionGate` trait, ACP integration, `McpProxyTool` injection · `crucible-tools`, `crucible-acp`
- [x] **Error Handling UX** `P0` — Toast notifications, contextual messages, graceful degradation for DB lock/search/kiln fallback, `BackendError::is_retryable()` + `retry_delay_secs()`, RPC `call_with_retry()` for idempotent daemon ops, recovery suggestions in error messages · `crucible-cli`, `crucible-core`, `crucible-daemon-client`
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
- [x] **Kiln Tools** `P0` — `get_kiln_info`, `get_kiln_roots`, `get_kiln_stats` · `crucible-tools`
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

- [ ] **One-Line Install** `P0` — Pre-built binaries via GitHub Releases (linux x86_64/aarch64, macOS Intel/Apple Silicon, Windows); `curl|sh`, `brew install crucible`, `cargo binstall crucible`, AUR, Nix flake; target: working `cru` binary in <60 seconds · `crucible-cli`
- [ ] **Precognition Default-On** `P0` — Change default from opt-in to on; the knowledge-graph-aware context is the core differentiator and should not be hidden behind `:set precognition on` · `crucible-cli`, `crucible-config`

### HTTP Gateway (P1 — platform layer for everything external)

> The daemon is Unix-socket-only (JSON-RPC 2.0). Messaging bots, webhook triggers, web UI, and any external client all need HTTP access. This is the shared foundation — wire `crucible-web` to `DaemonClient` and expose the daemon's 55 RPC methods over HTTP + SSE/WebSocket for events.

```
HTTP Gateway (crucible-web wired to daemon)
    ├── Messaging bots (Telegram, Discord)
    ├── Webhook endpoints (POST /api/webhook/:name)
    └── Web UI (SolidJS frontend on same server)
         └── Remote access (Tailscale / Cloudflare Tunnel)
```

- [ ] **HTTP-to-RPC Bridge** `P1` — Wire `DaemonClient` into `crucible-web` Axum routes; translate HTTP requests to daemon JSON-RPC calls; ~1,000-1,500 lines of Rust · `crucible-web`, `crucible-daemon-client`
- [ ] **SSE/WebSocket Event Bridge** `P1` — Subscribe to daemon session events, stream to HTTP clients via SSE or WebSocket; the only non-trivial part of the gateway · `crucible-web`
- [ ] **Chat HTTP API** `P1` — `POST /api/chat/send` + SSE stream for responses; `POST /api/session/create`, `/resume`, `/list`, `/end` · `crucible-web`
- [ ] **Search HTTP API** `P1` — `POST /api/search` (semantic + full-text + property); `GET /api/notes`, `GET /api/notes/:name` · `crucible-web`
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

### Ecosystem & Shareability (P1-P2)

- [ ] **Example Plugin Pack** `P1` — Ship 3-5 reference plugins demonstrating key patterns: autonomous loops, scheduled tasks, fan-out, auto-linking · `crucible-lua`
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

- [x] **SurrealDB Backend** `P0` — Primary storage with RocksDB engine · [[Help/Config/storage]] · `crucible-surrealdb`
- [x] **SQLite Backend** `P0` — Lightweight alternative storage · [[Help/Config/storage]] · `crucible-sqlite`
- [x] **Vector Embeddings** `P0` — FastEmbed (ONNX) local embedding generation · [[Help/Config/embedding]] · `crucible-llm`
- [x] **Embedding Reranking** `P0` — Search result reranking for relevance · `crucible-surrealdb`
- [x] **File Processing** `P0` — Parse, enrich, and index notes via pipeline · [[Help/CLI/process]] · `crucible-pipeline`, `crucible-enrichment`
- [x] **Transaction Queue** `P0` — Batched database operations with consistency · `crucible-surrealdb`
- [x] **Hash-based Change Detection** `P0` — Content-addressable block hashing · `crucible-core`
- [x] **Task Storage** `P0` — Task records, history, dependencies, file associations · `crucible-surrealdb`
- [x] **Kiln Statistics** `P0` — Note counts, link analysis, storage metrics · [[Help/CLI/stats]] · `crucible-cli`
- [x] **Daemon Server** `P0` — Unix socket server with 35 RPC methods · `crucible-daemon`
- [x] **Daemon Client** `P0` — Auto-spawn, reconnect, RPC client library · `crucible-daemon-client`
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

> Builds on the HTTP gateway (P1). Messaging integrations are the daily-driver interface; web UI adds richer interactions (graph visualization, multi-panel layouts, file previews, workflow configuration). Serve over Tailscale/Cloudflare Tunnel for self-hosted remote access; PWA for mobile without app store friction.

- [-] **Web Chat UI** `P3` — SolidJS frontend on the same `crucible-web` server as the HTTP gateway; chat + search + kiln browsing · `crucible-web` · depends: [[#HTTP Gateway|HTTP-to-RPC Bridge]]
- [ ] **PWA Support** `P3` — Progressive Web App manifest + service worker; enables mobile access without app store, installable from browser · `crucible-web`
- [ ] **Oil Node Serialization** `P3` — Oil Node → JSON for web rendering · `crucible-oil`
- [ ] **SolidJS Renderer** `P3` — `<OilNode>` component for browser rendering · `crucible-web`
- [ ] **Shared Component Model** `P3` — Unified TUI/Web rendering from same Oil nodes · `crucible-oil`, `crucible-web`
- [ ] **Tauri Desktop** `P3` — Native desktop app wrapping web UI · depends: [[#Web & Desktop|Web Chat UI]]
- [ ] **Canvas / Flowcharts** `P3` — WebGL-based visual workflow editor · depends: [[#Web & Desktop|Web Chat UI]]
- [ ] **Rich Rendering** `P3` — Mermaid diagrams, LaTeX rendering, image OCR · `crucible-web`
- [ ] **Document Preview** `P3` — PDF and image rendering in notes · `crucible-web`

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
| 2026-02-03 | Web UI deprioritized to P3 | Messaging covers daily interaction; web serves richer interactions later, via Tailscale for privacy, PWA for mobile |
| 2026-02-03 | Precognition should default to on | Core differentiator shouldn't be opt-in; knowledge-graph-aware context is the product's value proposition |
| 2026-02-03 | Proactive kiln digest at P2 | Matches OpenClaw's most viral feature (heartbeat) using Crucible's strength (knowledge graph); delivered via messaging integrations |
| 2026-02-03 | Discord as daemon-side plugin, not separate crate | Direct REST API + Gateway integration via Lua plugin with `config`, `network`, `websocket` capabilities; avoids HTTP gateway dependency; validates plugin system for real integrations |

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
