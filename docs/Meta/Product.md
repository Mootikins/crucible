---
title: Product
description: Product feature map — capabilities, status, documentation, and dependencies
type: product
status: active
updated: 2026-06-28
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

A **knowledge-grounded agent runtime**. Agents that draw from a knowledge graph make better decisions — memory and knowledge are too fundamental to be an afterthought. Your notes, sessions, and wikilinks form that graph. Everything beyond the knowledge core is extensible.

- **Knowledge + Agents** — the core. Agents draw from and contribute to a knowledge graph. [[Help/Concepts/Precognition|Precognition]] injects relevant context before every turn. Sessions persist as linked notes. The more you use it, the smarter it gets.
- **PKM as input** — notes, wikilinks, tags, and sessions-as-notes are how knowledge enters the system. Not an add-on; essential infrastructure.
- **Neovim-like architecture** — Lua extensibility, TUI-first, headless daemon with RPC, plugin-driven. Most behaviors beyond the knowledge core can be scripted.
- **Plaintext-first** — you own everything as markdown files. The daemon is an implementation detail. Simple at rest, powerful when running.

## User Progression

| Phase | Users | Interface |
|-------|-------|-----------|
| Now | Power users, developers | CLI (chat-focused) |
| Next | Plugin creators, agent developers | CLI + Lua scripting + messaging integrations |
| Later | Broader audience, mobile users | Web PWA (self-hosted via Tailscale/Cloudflare Tunnel) |

---

## Note-Taking & Authoring

- [x] **Wikilinks** `P0` — `[[note]]` linking with aliases, headings, and block refs · [[Help/Wikilinks]] · `crucible-core` (parser)
- [x] **Tags** `P0` — `#tag` and `#nested/tag` taxonomy · [[Help/Tags]] · `crucible-core` (parser)
- [x] **Frontmatter** `P0` — YAML/TOML metadata in note headers · [[Help/Frontmatter]] · `crucible-core` (parser)
- [x] **Block References** `P0` — `^block-id` paragraph-level linking · [[Help/Block References]] · `crucible-core` (parser)
- [x] **Callouts** `P0` — `> [!type]` admonition blocks · `crucible-core` (parser)
- [x] **LaTeX** `P0` — `$inline$` and `$$block$$` math notation · `crucible-core` (parser)
- [x] **Footnotes** `P0` — Reference-style footnotes · `crucible-core` (parser)
- [x] **Tables** `P0` — Markdown tables with alignment · `crucible-core` (parser)
- [x] **Task Lists** `P0` — `- [ ]` / `- [x]` checkbox items · [[Help/Task Management]] · `crucible-core` (parser)
- [x] **Kilns** `P0` — Vault-like note collections with `.crucible/` config · [[Help/Concepts/Kilns]] · `crucible-core`
- [x] **Plaintext First** `P0` — Markdown files are always the source of truth · [[Help/Concepts/Plaintext First]]
- [ ] **Note Types** `P3` — Templates and typed notes (book, meeting, movie) · `crucible-core`

## Knowledge Discovery

- [x] **Semantic Search** `P0` — Vector similarity search with reranking · [[Help/Concepts/Semantic Search]] · `crucible-daemon` (storage), `crucible-daemon` (llm)
- [x] **Full-text Search** `P0` — Fast text search across all notes · [[Help/CLI/search]] · `crucible-daemon` (storage)
- [x] **Knowledge Graph** `P0` — Wikilink-based graph structure and traversal · [[Help/Concepts/The Knowledge Graph]] · `crucible-daemon` (storage)
- [x] **Query System** `P0` — Structured note queries with composable pipeline · [[Help/Query/Query System]] · `crucible-daemon` (storage) (query)
- [x] **Property Search** `P0` — Search notes by frontmatter properties and tags · `crucible-daemon` (tools)
- [x] **Document Clustering** `P0` — Heuristic clustering and MoC detection · `crucible-daemon` (storage)
- [ ] **K-Means Clustering** `P2` — K-means implementation (placeholder stub, needs ndarray or similar) · `crucible-daemon` (storage)
- [x] **Block-level Embeddings** `P0` — Paragraph-granularity semantic indexing · `crucible-daemon` (llm), `crucible-daemon` (storage)
- [x] **Session Search** `P0` — Past conversations indexed and searchable via session indexing pipeline; `cru session reindex` for batch processing · `crucible-daemon` (observe), `crucible-cli`

## Agent Learning & Memory

> Agents that get smarter over time. Learning is implemented as **notes in the kiln** — not opaque database stores. Entity facts, session summaries, and accumulated knowledge are all atomic zettelkasten-style markdown notes with wikilinks, tags, and frontmatter. This means agent memory is human-readable, editable, searchable via the existing knowledge graph, and available to precognition for future context injection.
>
> **Two-tier model**: Core Rust features (precognition, auto-linking) handle the fast path. Default runtime Lua plugins handle higher-level knowledge extraction. Both are toggleable and overridable. See [[#Core Agent Features]] and [[#Default Runtime Plugins]].
>
> **Informed by**: Agno framework analysis (2026-02). Agno uses six opaque DB-backed learning stores. Crucible's approach is strictly better — same learning capabilities but with human-readable, editable, wikilinked notes as the storage layer. Local embeddings, metadata (wikilinks, tags, frontmatter), and potential future LSP integration provide rich fetchable context without custom storage.

- [x] **Precognition** `P0` — Auto-RAG: inject relevant kiln/session context before each agent turn; default on; `:set precognition`; the core differentiator — every conversation is knowledge-graph-aware · `crucible-cli`, `crucible-daemon` (acp)
- [x] **Session Persistence** `P0` — Conversations saved as markdown + JSONL in kiln; indexed for semantic search · `crucible-daemon` (observe)
- [x] **Memory Scoping** `P2` — `Scope::Workspace { path }` (single variant, workspace-only) enforced at the storage query layer (`SqliteNoteStore` filter + Lance post-filter). Write-side validation prevents writes targeting a sibling workspace; raw-SQL escape hatch gated `#[cfg(test)]`. Pre-prune `Global` and `User { id }` variants removed in the 2026-05-12 consolidation pass — neither had any in-tree consumer once session-digest shipped, then was retracted · `crucible-core`, `crucible-daemon`

### Self-Improvement Avenues

> Two complementary ways an agent gets smarter. **Knowledge insertion** is the primary path and ships today; a **reflection pass** is the second avenue still to build. Both write to the same place — atomic kiln notes — so improvement stays human-readable and editable.

- [x] **Knowledge Insertion (primary)** `P1` — Agents persist learning *during* work by writing kiln notes via tools (`create_note`, `update_note`); those notes re-enter future turns through [[Help/Concepts/Precognition|Precognition]]. The graph *is* the learning store — reactive, in-the-loop, no opaque DB · `crucible-daemon` (tools)
- [ ] **Reflection Pass (second avenue)** `P2` — Knowledge insertion only captures what the agent thinks to write mid-turn. A complementary background reflection — a forked low-cost agent that reviews a *finished* session and **proposes** new/updated notes and skills, with provenance so it never edits human-authored notes — catches learning the inline path misses. Crucially distinct from the removed `session-digest` (which auto-merged via LLM-judged dedupe and risked kiln pollution): this proposes, the user (or a curator) disposes. Informed by Hermes Agent's background-review + curator loop (2026-06-28) · `crucible-daemon`, `crucible-lua`

## Context & Execution (Core Runtime)

> Runtime primitives that every reliable agent needs. These are too fundamental to be plugins — they govern how the agent manages its own context window, enforces execution boundaries, validates its output, and allows users to recover from mistakes. Informed by competitive analysis (2026-03): Aider, CrewAI, LangGraph, and Semantic Kernel all treat these as core concerns.

### Prompt Caching
- [x] **Anthropic Cache Control** `P0` — `CacheControl::Ephemeral` set on system prompts and second-to-last conversation turn via `apply_prompt_caching()` in genai_handle.rs; 90% cost reduction on cached reads; OpenAI caching is automatic (no code needed); cache token counts flow through `message_complete` events (`cache_read_tokens`, `cache_creation_tokens`) · `crucible-daemon`
- [x] **Cache Stats** `P1` — Per-session `CacheStats` aggregate (hits, misses, read/creation tokens, hit_rate) in `AgentManager` exposed via `session.cache_stats` RPC; `cru.sessions.cache_stats(id)` Lua binding via `DaemonSessionApi`; TUI statusline supports a `{ type = "cache_hit_rate" }` component with `{percent}` placeholder, fed from `message_complete` cache token fields · `crucible-daemon`, `crucible-lua`, `crucible-cli`

### Context Window Management
- [x] **Token Budget Tracking** `P0` — `context_budget` field on `SessionAgent`, settable via RPC; `crucible_core::traits::context_ops::estimate_tokens` / `estimate_messages_tokens` provide chars/4 heuristic; auto-compact triggered in `run_reactor_handlers` via `agent_manager::autocompact::should_autocompact` when prompt usage exceeds `context_budget * autocompact_threshold` (default 0.95, configurable per session via `:set autocompact_threshold`) · `crucible-daemon`, `crucible-core`, `crucible-cli`
- [x] **Context Strategies** `P1` — `ContextStrategy` enum: `Truncate` (drop oldest, default), `SlidingWindow` (keep last N pairs + system), and `Summarize` (drains older turns and replaces them with an LLM-generated recap via the same backend the agent uses; static `[summary placeholder]` is the fallback when the summarise call errors or returns empty); all three enforced in `genai_handle.rs::enforce_context_budget` + the async wrapper inside `stream_chat_from_messages`; `:set context_strategy=summarize` accepted · `crucible-core`, `crucible-daemon`
- [x] **Lua Context Operations** `P1` — `cru.context.{usage, compact, messages, remove, estimate_tokens}` bind to `crucible-core::traits::context_ops` via `DaemonSessionApi`. `usage` returns `{messages, prompt_tokens, budget, percent}`; `compact` triggers `SessionManager::request_compaction`; `remove` mirrors `undo_turns` cursor-rewind semantics on the branchable `ConversationTree` · `crucible-lua`, `crucible-core`, `crucible-daemon`

### Execution Limits
- [x] **Max Iterations** `P1` — `DEFAULT_MAX_TOOL_DEPTH = 10`; `max_iterations` on `SessionAgent` settable via RPC; injects "Iteration limit reached" prompt on exceed; `None` = unlimited · `crucible-daemon`
- [x] **Execution Timeout** `P1` — `execution_timeout_secs` on `SessionAgent` settable via RPC; cancel and report on exceed; enforced in `execute_agent_stream()` · `crucible-daemon`

### Agent Undo
- [x] **Turn Undo** `P1` — `/undo` reverts last agent turn: file rollback via `WorkspaceSnapshot` (git mode uses `write-tree`+`commit-tree` for untracked-file safety; non-git mode uses in-memory journal capped at 5MiB) + message truncation. Snapshots keyed by `(session_id, NodeId)` in a daemon-side `SnapshotMap`; restored on `AgentManager::undo` after tree rewind. `/undo 3` for multiple turns. `/redo` deferred — no `redo_turns` analogue on `ConversationTree` yet · `crucible-daemon`, `crucible-cli`
- [x] **Undo Lua API** `P1` — `cru.sessions.undo(id, n?)`, `cru.sessions.can_undo(id)`, `cru.sessions.undo_depth(id)`, `cru.sessions.undo_history(id)` returning `[{turn_index, messages_removed}]`. `redo` deferred · `crucible-lua`, `crucible-daemon`

### Output Validation
- [x] **Validate-Retry Loop** `P1` — `validate_output` runs in `agent_manager::messaging::stream::execute_agent_stream` after each assistant turn. Failure with retries remaining injects a synthetic regenerate-prompt and re-enters the stream; exhaustion emits `ended` with reason `error: output validation exhausted retries`. `OutputValidation::None` (default) is a zero-cost early return. Lua-callback validators are deferred to Wave 1 · `crucible-daemon`, `crucible-core`
- [x] **Lua Validators** `P1` — `OutputValidation::Lua { name }` variant in core. Plugin authors register via `cru.context.register_validator(name, fn)` (validator returns `true | false, reason`); per-session enable via `cru.sessions.set_output_validation(id, { type = "lua", name = "..." })`. Daemon `DaemonPluginLoader` owns one `Arc<LuaValidatorRegistry>` shared with `AgentManager` via `OnceLock`; `validate_response_or_retry` calls `registry.run(&Lua, name, response)` synchronously (mlua `send` feature → no async lock). Unregistered names and unbound runtime degrade to validation failure (retry/exhaust path), not panic · `crucible-core`, `crucible-lua`, `crucible-daemon`

## AI Chat & Agents

### Conversation & Sessions
- [x] **Interactive Chat** `P0` — Conversational AI with streaming text, thinking, tool calls, and subagent events · [[Help/CLI/chat]] · `crucible-cli`, `crucible-daemon`
- [x] **Agent Cards** `P0` — Configurable agent personas with system prompts, model, temperature, tools · [[Help/Extending/Agent Cards]] · [[Help/Config/agents]] · `crucible-core` (config)
- [x] **Session Persistence** `P0` — Conversations saved as markdown + JSONL in kiln · [[Help/Core/Sessions]] · `crucible-daemon`
- [x] **Session Resume** `P0` — Load and continue previous sessions with full history · [[Help/Core/Sessions]] · `crucible-daemon` (rpc)
- [x] **Conversation History** `P0` — Clear history (`:clear`), resume with prior messages; TUI viewport hydrated from daemon session events · `crucible-cli`, `crucible-daemon`
- [x] **Message Queueing** `P0` — Type and queue messages during streaming; Ctrl+Enter force-sends · `crucible-cli`

### Agent Runtime
- [x] **Internal Agent** `P0` — Built-in agent with session memory and tool access · [[Help/Extending/Internal Agent]] · `crucible-daemon`, `crucible-core`
- [x] **Multiple LLM Providers** `P0` — Ollama, OpenAI, Anthropic via unified interface · [[Help/Config/llm]] · `crucible-daemon`, `crucible-core`
- [x] **Model Switching** `P0` — Runtime `:model <name>` with autocomplete; model listing from Ollama · `crucible-daemon`, `crucible-cli`
- [x] **Extended Thinking** `P0` — Budget presets (off/minimal/low/medium/high/max) via `:set thinkingbudget`; Ctrl+T toggles display · `crucible-daemon`, `crucible-cli`
- [x] **System Prompt** `P0` — Loaded from agent card at session creation; layered prompt composition · `crucible-daemon`, `crucible-core` (config)
- [x] **Environment Overrides** `P0` — `--env KEY=VALUE` flag for per-session env vars (e.g., API keys) · `crucible-cli`
- [x] **Agent Cancellation** `P0` — Ctrl+C/Esc cancels local stream and propagates to daemon via `session.cancel` RPC · `crucible-daemon`

### Tools & Permissions
- [x] **Tool Calls** `P0` — Inline tool execution with streaming results; parallel calls tracked by call_id · `crucible-daemon` (tools), `crucible-core`
- [x] **Permission System** `P0` — Multi-layer: safe-tool whitelist → pattern matching → Lua hooks → user prompt · `crucible-daemon`
- [x] **Pattern Whitelisting** `P0` — "Always allow" saves project-scoped patterns for future sessions · `crucible-daemon`
- [x] **Permission Hooks (Lua)** `P0` — Custom Lua hooks can Allow/Deny/Prompt with 1s timeout · `crucible-lua`, `crucible-daemon`
- [x] **Interaction System** `P0` — Agent can ask questions (single/multi-select, free-text) and request permission · `crucible-core`, `crucible-daemon`
- [x] **Subagent Spawning** `P0` — Background job manager for parallel subagent tasks with cancellation · `crucible-daemon`
- [x] **Delegation** `P1` — `delegate_session` spawns a child agent that reuses the same session/task primitives (`cru.sessions.{create, configure_agent, send_and_collect, end_session, collect_subagents}`) — delegation is **not a separate subsystem**, it behaves like ordinary session/task creation; depth and trust gated via `DelegationConfig` (max_depth, allowed_targets, data-classification check). Supervisor/router/broadcast are Lua recipes, not built-ins · [[Help/Concepts/Delegation]] · [[Help/Delegation Patterns]] · `crucible-daemon`

### Tool Discovery & Disclosure

> Agents shouldn't carry every tool schema in context. Discovery tools let an agent find tools on demand; progressive disclosure makes that automatic when the tool set is large.

- [x] **Tool Discovery** `P1` — `discover_tools` (search by name/description/source) and `get_tool_schema` tools (`extended_mcp_server`) let an agent enumerate and inspect tools at runtime instead of relying solely on the attached schema list · `crucible-daemon` (tools)
- [ ] **Progressive Tool Disclosure** `P2` — Automatic deferral: when MCP/plugin tools would exceed a share of the context budget, swap them for the discovery bridge (search → describe → call) and surface on demand; core tools never deferred. The internal agent currently attaches all tools every turn — this closes that gap as MCP servers proliferate. Informed by Hermes Agent's progressive disclosure (2026-06-28) · `crucible-daemon` (tools)

### Agent Skills

> Skills are markdown capability docs ([agentskills.io](https://agentskills.io)-compatible `SKILL.md` + optional `scripts/`, `references/`) that teach the agent procedures on demand. Discovery and parsing ship today; daemon-side context injection is the remaining wiring.

- [x] **Skill Discovery** `P1` — Folder discovery across search paths, `SKILL.md` frontmatter parsing, resolution; `cru skills` CLI listing; bundled help skills at `runtime/crucible-help/skills` · [[Help/Concepts/Agent Skills]] · [[Help/CLI/skills]] · `crucible-daemon` (skills), `crucible-cli`
- [-] **Skill Context Injection** `P1` — `format_skills_for_context` renders discovered skills for the prompt but has **no daemon turn-path call site**; skills currently reach the model only if the client pre-bakes them into `system_prompt`. Wire tier-1 metadata injection daemon-side, with progressive disclosure (list → view → use) so full `SKILL.md` loads only when invoked · `crucible-daemon` (skills)
- [ ] **Skill Self-Creation** `P2` — Agent-authored skills distilled from successful sessions (ties into the [[#Self-Improvement Avenues|Reflection Pass]]); provenance separating agent-created from user-authored so neither clobbers the other · `crucible-daemon` (skills)

### Context & Knowledge
- [x] **Context Enrichment** `P0` — Inject kiln context into agent conversations · `crucible-daemon`
- [x] **File Attachment** `P0` — `@file` context attachment in chat · `crucible-cli`
- [x] **Rules Files** `P0` — Project-level AI instructions (`.crucible/rules`) · [[Help/Rules Files]] · `crucible-core` (config)

### Core Agent Features

> Core capabilities implemented in Rust. Precognition is toggleable via `:set precognition`. Other features are callable RPCs. Future: expose as toggleable session config with hook points for Lua override.

- [x] **Precognition** `P0` — Auto-RAG: inject relevant kiln context before each agent turn; `:set precognition`; the core differentiator — every conversation is knowledge-graph-aware · `crucible-cli`, `crucible-daemon` (acp)
- [x] **Auto-Linking** `P1` — `suggest_links` RPC detects unlinked mentions of existing notes in text via word-boundary matching; case-insensitive, skips already-linked targets · `crucible-daemon`
- ~~**Team Patterns**~~ — supervisor / router / broadcast are user-Lua recipes against `cru.sessions.*` primitives, not hardcoded Rust types. See [[Help/Delegation Patterns]]. Removed 2026-05-12 (~1984 LOC) after recognising the hardcoded shapes shut out variants (Lua decider vs LLM, regex vs classifier, etc.) and shipped without consumers.

### Lua Session API
- [x] **Scripted Agent Control** `P0` — Lua API for temperature, max_tokens, thinking_budget, model, mode; daemon getters use local cache · `crucible-lua`
- [x] **Session Event Handlers** `P0` — Lua hooks on `turn:complete` can inject follow-up messages · `crucible-lua`, `crucible-daemon`

### Lua Session & Tool Primitives (Planned)

> These fill gaps so autonomous loops, fan-out, and context control are trivial plugins — not bespoke features.

- [x] **`cru.tools.call(name, args)`** `P1` — Programmatic tool calling from Lua; returns results synchronously; respects session permission scope; the bridge between "plugins that react" and "plugins that do intelligent work" · `crucible-lua`, `crucible-daemon` (tools)
- [x] **`cru.tools.batch({...})`** `P1` — Concurrent multi-tool calls; `batch({{"semantic_search", {query="X"}}, {"list_notes", {tag="Y"}}})` runs in parallel via async runtime; essential for digest/summarization plugins · `crucible-lua`, `crucible-daemon` (tools)
- [x] **`cru.sessions.messages(id, opts)`** `P1` — Read conversation history from Lua; opts: `{role, limit}`; enables context windowing, summarization, checkpoint detection · `crucible-lua`, `crucible-daemon`
- [x] **`cru.sessions.inject(id, role, content)`** `P1` — Insert messages mid-conversation via `session.inject_context` RPC; requires session_id as first param; persists to session log and emits broadcast event · `crucible-lua`, `crucible-daemon`
- [x] **`session.fork()`** `P1` — `cru.sessions.fork(id, opts)` returns `{ id, parent_id, messages_copied }`; copies message history and (via RPC handler) agent config; enables parallel exploration, A/B approach testing · `crucible-lua`, `crucible-daemon`
- [x] **`cru.sessions.collect_subagents(ids, timeout?)`** `P1` — `subagent.collect` RPC + Lua API; await multiple subagents with optional timeout · `crucible-lua`, `crucible-daemon`

### In Progress / Planned
- [x] **MCP Tool System** `P0` — Permission prompts via `PermissionGate` trait, ACP integration, `McpProxyTool` injection · `crucible-daemon` (tools), `crucible-daemon` (acp)
- [x] **Error Handling UX** `P0` — Toast notifications, contextual messages, graceful degradation for DB lock/search/kiln fallback, `BackendError::is_retryable()` + `retry_delay_secs()`, RPC `call_with_retry()` for idempotent daemon ops, recovery suggestions in error messages · `crucible-cli`, `crucible-core`, `crucible-daemon` (rpc)
- [x] **Per-session MCP Servers** `P0` — Agent cards define MCP servers; `mcp_servers` propagated to `SessionAgent` and wired in daemon · `crucible-daemon` (acp)
- [ ] **Grammar + Lua Integration** — `cru.grammar.{new, presets.*, set/clear/get_session_grammar}` GBNF bindings shipped briefly in Wave 2 but had no working backend; removed in the 2026-05-12 prune. Will revisit once llama-cpp is integrated · `crucible-core`, `crucible-lua`, `crucible-daemon`

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
- [x] **Streaming Graduation** `P0` — Drain-based: completed containers render through Taffy, write to stdout (terminal scrollback); viewport shows only live content · `crucible-cli`
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
- [x] **Taffy Layout** `P0` — Flexbox-based terminal layout engine; single spacing system via `gap()` for both graduated and viewport content · `crucible-oil`
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
- [x] **Execution Backends as Plugins** `P1` — Workspace tools (`bash`, `read_file`, `edit_file`, `write_file`) are intercepted via `pre_tool_call` hooks and routed to alternate backends; the agent core stays backend-agnostic. Reference: the `oci` runtime plugin (v0.2.0) runs workspace tools inside OCI containers via `docker`/`podman exec`, returning `{ handled = true, result = ... }`. Sandbox/container isolation is a **plugin, not a core concern** — the daemon never grows a per-backend abstraction · `runtime/plugins/oci/` · `crucible-lua`, `crucible-daemon`
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

- [x] **LuaCATS Type Stubs** `P1` — `StubGenerator::generate` walks the registered `cru.*` namespaces and emits `cru.lua` (EmmyLua/LuaCATS) plus `cru-docs.json`; auto-runs at daemon startup writing to `~/.config/crucible/luals/`. `cru.context` added to `UNIVERSAL_MODULES` so the new Wave 1 surface ships with stubs · `crucible-lua`, `crucible-daemon`
- [x] **Plugin Hot Reload** `P1` — `:reload <plugin>` command; invalidate `package.loaded`, re-require, re-extract services; `plugin.reload` RPC + `plugin.list` RPC · `crucible-lua`, `crucible-daemon`, `crucible-cli`
- [x] **`:lua` REPL** `P1` — `lua.eval` RPC + `cru lua` CLI command; `=expr` prints result (Neovim pattern); inspect plugin state, test API calls · `crucible-cli`, `crucible-daemon`
- [x] **`cru plugin new`** `P1` — Scaffold plugin from template: `plugin.yaml`, `init.lua`, `health.lua`, `.luarc.json`, `tests/` directory · `crucible-cli`
- [x] **Clean Error Messages** `P1` — `format_lua_error()` strips Rust FFI frames from stack traces; errors include plugin name + file path + line number · `crucible-lua`
- [-] **Plugin Test Harness** `P2` — `cru plugin test <name>` runs plugin tests with mock `cru.*` API; busted-compatible test runner; enables CI for plugins · `crucible-lua`, `crucible-cli`
- [x] **`.luarc.json` Generation** `P2` — `cru plugin new` and `cru plugin init` emit LuaLS config pointing to type stubs; zero-config IDE setup · `crucible-cli`

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

- [x] **ACP Host** `P0` — Spawn and control ACP agents with JSON-RPC 2.0 over stdio; capability negotiation, plan/act modes · [[Help/Concepts/Agents & Protocols]] · `crucible-daemon` (acp)
- [x] **Context Injection** `P0` — `PromptEnricher` performs semantic search, wraps results in `<precognition>` blocks; configurable via `ContextConfig::inject_context` · `crucible-daemon` (acp)
- [x] **In-Process MCP Host** `P0` — SSE transport MCP server running in-process; agents discover Crucible tools without external server · `crucible-daemon` (acp)
- [x] **Agent Discovery** `P0` — Parallel probing of known agents (`claude-code`, `opencode`, `cursor-acp`, `gemini`); env var overrides · `crucible-daemon` (acp)
- [x] **Sandboxed Filesystem** `P0` — Path validation, traversal prevention, mode-based permissions (plan=read-only), configurable size limits · `crucible-daemon` (acp)
- [x] **Permission Gate** `P0` — `PermissionGate` trait for pluggable permission decisions; `list_directory` in `FileSystemHandler` · `crucible-daemon` (acp)
- [x] **Streaming Responses** `P0` — Chunk processing, tool call parsing from stream, diff handling · `crucible-daemon` (acp)
- [x] **Session Management** `P0` — UUID sessions with config (cwd, mode, context size), history with ACP roles, persistence across reconnections · `crucible-daemon` (acp)

### ACP Agent (Future — Crucible as Embeddable Agent)

- [ ] **ACP Agent Mode** `P1` — Crucible as an embeddable ACP agent; any ACP host (Zed, JetBrains, Neovim) spawns Crucible to get knowledge graph + memory · `crucible-daemon` (acp)
- [ ] **ACP Registry Submission** `P1` — Agent manifest for [ACP Registry](https://github.com/agentclientprotocol/registry); one PR → available in all ACP clients · `crucible-daemon` (acp)
- [ ] **ACP Schema Bump** `P1` — Bump `agent-client-protocol-schema` from 0.10.6 → 0.10.7; SDK crate (`agent-client-protocol` 0.9.3) and wire protocol (v1) are already current · `crucible-daemon` (acp), `crucible-core`

### MCP Server (External Agents → Crucible Tools)

- [x] **MCP Server** `P0` — Expose kiln as MCP tools for external AI agents · [[Help/Concepts/Agents & Protocols]] · `crucible-daemon` (tools)
- [x] **Note Tools** `P0` — `create_note`, `read_note`, `read_metadata`, `update_note`, `delete_note`, `list_notes` · `crucible-daemon` (tools)
- [x] **Search Tools** `P0` — `semantic_search`, `text_search`, `property_search` · `crucible-daemon` (tools)
- [x] **Kiln Tools** `P0` — `get_kiln_info` · `crucible-daemon` (tools)
- [x] **Workspace Tools** `P0` — `read_file`, `edit_file`, `write_file`, `bash`, `glob`, `grep` · `crucible-daemon` (tools)
- [x] **TOON Formatting** `P0` — Token-efficient response formatting · `crucible-daemon` (tools)

### MCP Gateway (Crucible → Upstream MCP Servers)

- [x] **MCP Gateway** `P0` — Connect upstream MCP servers with prefixed tool names · [[Help/Extending/MCP Gateway]] · [[Help/Config/mcp]] · `crucible-daemon` (tools)
- [x] **Lua Plugin Tools** `P0` — Dynamic tool discovery from Lua plugins · `crucible-daemon` (tools), `crucible-lua`
- [x] **MCP Bridge/Gateway** `P0` — `McpGatewayManager` shared in daemon, `McpProxyTool` dynamic injection, live status display · `crucible-daemon` (tools)
- [x] **MCP Connection Stability** `P0` — Auto-reconnect loop, 30s SSE keepalive, live status indicators in TUI · `crucible-daemon` (acp)

## Distribution & Growth

> How Crucible reaches users and spreads. Ordered by growth impact.
>
> **Insight from OpenClaw analysis (2026-02):** Viral growth came from instant install, meeting users in apps they already use, and proactive behavior. Crucible's counter-position: "Your AI should live in your notes, not a chat app you don't control."

### Install & Onboarding (P0 — #1 adoption blocker)

- [x] **One-Line Install** `P0` — Pre-built binaries via GitHub Releases (linux x86_64/aarch64, macOS Intel/Apple Silicon); `curl|sh`, `brew install mootikins/crucible/crucible` (external tap), `cargo binstall crucible-cli`; target: working `cru` binary in <60 seconds · `crucible-cli`
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

- [x] **HTTP-to-RPC Bridge** `P1` — Wire `DaemonClient` into `crucible-web` Axum routes; translate HTTP requests to daemon JSON-RPC calls · `crucible-web`, `crucible-daemon` (rpc)
- [x] **SSE/WebSocket Event Bridge** `P1` — Subscribe to daemon session events, stream to HTTP clients via SSE; `EventBroker` fans out per-session events · `crucible-web`
- [x] **Chat HTTP API** `P1` — `POST /api/chat/send` + `GET /api/chat/events/:session_id` SSE stream; `POST /api/session`, `/list`, `/:id/pause`, `/:id/resume`, `/:id/end` · `crucible-web`
- [x] **Search HTTP API** `P1` — `POST /api/search/vectors`; `GET /api/notes`, `GET /api/notes/:name`; `GET /api/kilns` · `crucible-web`
- [x] **API Auth** `P1` — Bearer token middleware with auto-generated key; localhost bypass; `~/.config/crucible/api_key` persistence · `crucible-web`, `crucible-core` (config)
- [x] **Webhook API** `P1` — `POST /api/webhook/:name` receives payloads, broadcasts `webhook:received` event for Lua handlers; enables GitHub webhooks, IFTTT/Zapier/n8n integration · `crucible-web`, `crucible-daemon`

### Messaging Integrations (P1 — meet users where they are)

> 1-2 good messaging integrations reduce the need for a web UI substantially. Users interact daily in messaging apps; Crucible meets them there and delivers proactive kiln insights. Integrations can be daemon-side Lua plugins (Discord) or thin adapters over the HTTP gateway (Telegram, Matrix).

- [ ] **Telegram Bot** `P1` — Bot API adapter over HTTP gateway; lowest friction (HTTP API, no app store approval, huge dev audience); enables proactive digest delivery · `crucible-telegram` (new crate) · depends: [[#HTTP Gateway|HTTP-to-RPC Bridge]]
- [x] **Discord Plugin** `P1` — Discord integration (REST API + Gateway) as daemon-side Lua plugin; tools: `discord_send`, `discord_read`, `discord_channels`, `discord_register_commands`; commands: `:discord connect/disconnect/status`. **Proof-of-concept that a plugin can *be* a gateway** — a full messaging entry point in Lua, not just a thin adapter over the HTTP gateway; the model for Telegram/Matrix/etc. · `plugins/discord/`
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
- [x] **Scheduled Lua Hooks** `P2` — `cru.schedule({every=N}, fn)` interval-based callbacks with `cru.schedule.cancel(handle)`; enables periodic digests and task reminders · `crucible-lua`, `crucible-daemon`
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

- [x] **Runtime Plugin Infrastructure** `P1` — `$CRUCIBLE_RUNTIME/plugins/` path with `PluginSource` provenance tracking; `plugin.list` RPC includes source/version; shadow-by-name semantics · `crucible-lua`, `crucible-daemon`
- [ ] **`session-digest` Runtime Plugin** — Shipped in Wave 2 then removed in the 2026-05-12 prune. LLM-judged dedupe risked wrong merges and kiln pollution; users preferred prompted refinement over automatic digests · `runtime/plugins/session-digest/` (removed)

### Ecosystem & Shareability (P1-P2)

- [ ] **Plugin Install** `P1` — `cru plugin add <git-url>` or `cru plugin add <name>`; Git-native distribution (lazy.nvim model, not centralized marketplace) · `crucible-lua`, `crucible-cli`
- [ ] **Agent Memory Branding** `P1` — Rename "Precognition" to "Agent Memory" in user-facing docs; communicates the value proposition directly · docs
- [ ] **`cru share`** `P2` — Export sessions as self-contained HTML or shareable artifacts; `:export` exists for local markdown, this adds sharable formats · `crucible-cli`
- [ ] **Graph Visualization** `P2` — Shareable knowledge graph renders (SVG/HTML); creates viral demo moments ("look at my AI-connected notes") · `crucible-cli` or `crucible-web`

## Workflow Automation

- [ ] **Markdown Handlers** `P2` — Event handlers in pure markdown, inject context into agents · [[Help/Extending/Markdown Handlers]] · depends: [[#Extensibility & Plugins]]
- [ ] **Workflow Markup** `P2` — DAG workflows in markdown (`@agent`, `->` data flow, `> [!gate]`) · [[Help/Workflows/Workflow Syntax]]
- [ ] **Workflow Sessions** `P2` — Log execution as markdown, resume interrupted work · [[Help/Workflows/Index]]
- [ ] **Session Learning** `P2` — Codify successful sessions into reusable workflows
- [x] **Parallel Execution** `P2` — `(parallel)` heading suffix and `&` step prefix for concurrent steps; consecutive parallel siblings join before the next step · [[Help/Workflows/Workflow Syntax]] · `crucible-core` (parser + engine), `crucible-daemon`
- [ ] **Workflow Authoring** `P2` — Guide for creating workflows · [[Help/Extending/Workflow Authoring]]

## Storage & Processing

- [x] **SQLite Backend** `P0` — Default storage; fast, lightweight, recommended for most users · [[Help/Config/storage]] · `crucible-daemon` (storage)
- [x] **Vector Embeddings** `P0` — FastEmbed (ONNX) local embedding generation · [[Help/Config/embedding]] · `crucible-daemon` (llm)
- [x] **Embedding Reranking** `P0` — Search result reranking for relevance · `crucible-daemon` (storage)
- [x] **File Processing** `P0` — Parse, enrich, and index notes via pipeline · [[Help/CLI/process]] · `crucible-daemon`
- [x] **Transaction Queue** `P0` — Batched database operations with consistency · `crucible-daemon` (storage)
- [x] **Hash-based Change Detection** `P0` — Content-addressable block hashing · `crucible-core`
- [x] **Task Storage** `P0` — Task records, history, dependencies, file associations · `crucible-daemon` (storage)
- [x] **Kiln Statistics** `P0` — Note counts, link analysis, storage metrics · [[Help/CLI/stats]] · `crucible-cli`
- [x] **Daemon Server** `P0` — Unix socket server with 35 RPC methods · `crucible-daemon`
- [x] **Daemon Client** `P0` — Auto-spawn, reconnect, RPC client library · `crucible-daemon` (rpc)
- [x] **Event Subscriptions** `P0` — Per-session and wildcard event streaming via daemon · `crucible-daemon`
- [x] **Notification RPC** `P0` — Add, list, dismiss notifications via daemon · `crucible-daemon`
- [x] **File Watching** `P0` — File change detection (notify/polling, debouncing, daemon bridge) with auto-reprocessing: `file_changed` events trigger `pipeline.process()` via daemon reprocess task; enrichment disabled for now (parsing + storage only) · `crucible-daemon` (watch)
- [ ] **Burn Embeddings** `P?` — Burn ML framework for local embeddings (stubbed) · `crucible-daemon` (llm)
- [ ] **LlamaCpp Embeddings** `P?` — GGUF model inference for embeddings (stubbed) · `crucible-daemon` (llm)
- [ ] **Session Compaction** `P?` — Compact sessions with cache purge for memory efficiency · `crucible-daemon`

## Configuration & Setup

- [x] **Config System** `P0` — TOML config with profiles, includes, environment overrides · [[Help/Configuration]] · `crucible-core` (config)
- [x] **Provider Config** `P0` — Unified provider configuration (Ollama, OpenAI, Anthropic) · [[Help/Config/llm]] · `crucible-core` (config)
- [x] **Embedding Config** `P0` — Provider, model, batch size, concurrency settings · [[Help/Config/embedding]] · `crucible-core` (config)
- [x] **Storage Config** `P0` — Backend selection, embedded vs daemon mode · [[Help/Config/storage]] · `crucible-core` (config)
- [x] **MCP Config** `P0` — Upstream MCP server connections · [[Help/Config/mcp]] · `crucible-core` (config)
- [x] **Workspace Config** `P0` — Multi-workspace kiln associations · [[Help/Config/workspaces]] · `crucible-core` (config)
- [x] **Agent Config** `P0` — Default agent, temperature, max_tokens, thinking budget · [[Help/Config/agents]] · `crucible-core` (config)
- [x] **CLI Commands** `P0` — 16 command modules: chat, session, process, search, stats, config, etc. · [[Help/CLI/Index]] · `crucible-cli`
- [x] **Init Command** `P0` — Project initialization (`cru init`) with path validation · `crucible-cli`
- [x] **Setup Wizard** `P0` — Oil TUI first-run wizard: auto-triggers on `cru chat` when no kiln exists; guides through kiln path, provider detection, model selection · `crucible-cli`
- [x] **Kiln Discovery** `P0` — Git-like upward `.crucible/` search with priority: CLI flag → ancestor walk → env var → global config · `crucible-cli`
- [x] **Kiln Path Validation** `P0` — Shared validation layer: hard blocks (root, nested kiln), strong warnings (git repo, source project, home dir, tmp), mild warnings (cloud sync) · `crucible-cli`
- [x] **Getting Started** `P0` — Installation and first steps guide · [[Guides/Getting Started]] · [[Guides/Your First Kiln]]
- [x] **Platform Guides** `P0` — Windows setup, GitHub Copilot integration · [[Guides/Windows Setup]] · [[Guides/GitHub Copilot Setup]]
- [x] **CLI Help & Discoverability** `P0` — Every subcommand carries a `long_about` with examples; `infer_subcommands = true` on the top-level `Cli` so unique prefixes resolve (e.g. `cru con show` → `cru config show`); clap 4 emits "did you mean" suggestions on typos by default; insta snapshots in `cli_help_snapshot_tests` lock `cru --help`, `cru chat --help`, and `cru session --help` · `crucible-cli`
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
- [x] **Oil Node Serialization** `P1` — `impl Serialize for Node` — Oil nodes to JSON for browser rendering; foundational primitive for all rich display · **Core Rust** · `crucible-oil` (behind `serde` cargo feature)
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
- [x] **PWA Support** `P1` — Manifest + service worker; installable from browser, mobile access without app store · **Core Rust** · `crucible-web`
- [ ] **SolidJS Oil Renderer** `P1` — `<OilNode>` component tree for browser; foundational rendering like Neovim's terminal grid — everything else depends on it · **Core Rust** (frontend) · `crucible-web`, `crucible-oil`

### Knowledge & Search (P2 — Crucible's unique strength)

- [ ] **Knowledge Graph Visualization** `P2` — Interactive force-directed wikilink graph; Lua provides data query, SolidJS + d3 renders; no new Rust primitives needed · **Lua extension** · `crucible-web`
- [ ] **Note Browser** `P2` — Browse notes with frontmatter, wikilinks, backlinks; users want custom columns/sort/filters — classic plugin territory · **Lua extension** · `crucible-web`
- [ ] **Search UI** `P2` — Unified semantic + full-text + property search; Rust owns the search RPC (perf-critical), Lua owns result formatting + custom scopes · **Hybrid** · `crucible-web`
- [ ] **Structured Data Views** `P3` — Obsidian Bases-style tables/kanban from frontmatter; the canonical "plugin not core" feature; depends on mature plugin system · **Lua extension** · `crucible-web`, `crucible-daemon` (storage) (query)

### Plugin UI & Artifacts (P2 — easy primitives to _make_ things)

- [ ] **Agent Artifacts** `P2` — Agent responses produce rendered outputs (code, documents, diagrams) in side panel; artifact extraction is domain logic, rendering uses panel system · **Lua extension** · `crucible-web`
- [ ] **Skills Browser** `P2` — Browse/enable/disable plugins with documentation; CRUD over plugin registry · **Lua extension** · `crucible-web`, `crucible-lua`
- [ ] **Rich Content Renderers** `P2` — Mermaid diagrams, LaTeX, syntax highlighting; each renderer is a plugin registering a content-type handler · **Lua extension** · `crucible-web`

### Configuration & System (P2)

- [ ] **Config Editor** `P2` — Schema-driven form for `config.toml`; form UI is plugin work, schema generated from config types · **Lua extension** · `crucible-web`, `crucible-core` (config)
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
| 2026-02-05 | Team patterns as core Rust, not Lua plugins | Multi-agent orchestration (supervisor, router, broadcast) is fundamental infrastructure, not optional behavior. Implemented in Rust, builds on existing subagent spawning. Configurable via `:set team.default_pattern`. Lua hooks can intercept delegation decisions. **Reversed 2026-05-12 — see next row.** |
| 2026-05-12 | Team patterns as Lua recipes, not Rust orchestrators | The hardcoded `Supervisor` / `Router` / `Broadcast` types each picked one delegation shape (Lua decider, single-shot classifier, parallel fan-out) and shut out variants. Users want to script delegation with primitives — supervisor + LLM judge, router + regex, supervisor + DAG, fan-in pipelines, retries. `cru.sessions.{create, configure_agent, send_and_collect, end_session, collect_subagents}` already covers it; the three patterns become 5–20 line recipes in [[Help/Delegation Patterns]]. ~1984 LOC removed. The 2026-02-05 "fundamental infrastructure" framing was wrong: delegation *primitives* are infrastructure, delegation *patterns* are user code |
| 2026-02-05 | SQLite is default storage | Product map updated to reflect SQLite as default, SurrealDB as advanced option. Docs (CLAUDE.md, Systems.md) corrected. SQLite is fast, lightweight, recommended for single-user local-first usage |
| 2026-03-21 | Context & Execution as core runtime | Competitive analysis (Aider, CrewAI, LangGraph, Semantic Kernel) revealed 5 features universally treated as core: context window management, execution limits, prompt caching, agent undo, output validation. All too fundamental for plugins. Partial infrastructure already exists (context_ops module, UndoTree, session.compact, genai CacheControl) |
| 2026-03-21 | Prompt caching via genai (no new deps) | genai v0.5.3 already exposes `CacheControl::Ephemeral`; Crucible just needs to import and use it. Anthropic: 90% savings on cached reads. OpenAI: automatic, no code needed. No Rust framework (Rig, kalosm) provides this — all DIY |
| 2026-03-21 | Token counting: heuristic, not library | No accurate local Claude 3+ tokenizer exists. tiktoken-rs covers OpenAI only. char/4 heuristic calibrated against provider usage stats is sufficient for budget decisions. Exact counts only matter for billing (provider reports post-hoc) |
| 2026-03-21 | Context strategies: truncate default, summarize opt-in | Truncate (drop oldest) is simple, no LLM call, good default. Summarize is expensive but preserves context — opt-in via `:set context_strategy=summarize`. Sliding window is middle ground. Prompt caching reduces urgency (cached prefix is cheap to resend) |
| 2026-03-21 | Agent undo: git-based + message rollback | Most valuable undo is file changes. Git stash before tool execution, apply on undo. Non-git: file journal. UndoTree<T> already exists in crucible-core. /undo slash command matches Aider pattern |
| 2026-03-21 | Output validation: primarily Lua-facing | Interactive chat rarely needs structured output validation. Programmatic/Lua sessions (entity extraction, digest generation) need it most. Built-in validators (json, regex) + custom Lua validators. Validate→retry loop is the universal pattern across CrewAI, Agno, Semantic Kernel |
| 2026-06-28 | Second self-improvement avenue: reflection pass | Hermes Agent review showed knowledge insertion (write-notes-mid-turn) is reactive and misses learning the agent doesn't think to record. A forked background reviewer that *proposes* notes/skills after a finished session — never auto-merging, provenance-tagged so it can't touch human notes — is the complement. Deliberately avoids the `session-digest` failure mode (automatic LLM-judged merges → kiln pollution): propose, don't dispose |
| 2026-06-28 | Tool/skill access via progressive disclosure | Hermes Agent review: attaching every MCP/plugin tool schema every turn bloats context and degrades as servers proliferate. `discover_tools`/`get_tool_schema` already exist; add automatic budget-based deferral (search → describe → call bridge) with core tools never deferred. Same pattern unifies tools and skills (list → view → use) |
| 2026-06-28 | Dropped user-facing temperature / max_tokens knobs | The genai turn path never applied session `temperature`/`max_tokens` (advertised via `temperature_control`/`max_tokens_control` capability flags that had zero readers — dead code). Like Claude Code/Codex, Crucible doesn't expose these as interactive knobs: low temp is near-always right for tool-use, and a user `max_tokens` mostly just truncates output. Removed the dead capability flags and the interactive `:set temperature`/`:set max_tokens` (TUI) + `cru set` (CLI) surfaces. Programmatic config (agent cards, Lua, web API) retains the fields for the rare deterministic-extraction case. |
| 2026-06-28 | Execution backends stay plugins, not core | Confirmed by Hermes Agent contrast: Hermes ships 6 hardcoded backends (local/Docker/SSH/Singularity/Modal/Daytona) in core. Crucible keeps the agent backend-agnostic and routes alternate execution through `pre_tool_call` hooks — the `oci` plugin is the reference. Docker/sandbox isolation is plugin territory; the daemon never grows a per-backend abstraction |

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
| SurrealDB Backend | Removed — SQLite is default and only backend; SurrealDB crate deleted |
| hermit plugin | Removed (2026-03-29) — capabilities belong in chat/messaging integration plugins |

## Links

- [[Meta/Roadmap]] — Phase-based development timeline
- [[Meta/Tasks]] — Active operational tasks
- [[Meta/backlog]] — Development backlog
- [[Meta/Systems]] — System architecture and boundaries
- [[Meta/TUI User Stories]] — TUI requirements
- [[Meta/Plugin User Stories]] — Plugin requirements
- [[Meta/Plugin API Sketches]] — Plugin API designs
