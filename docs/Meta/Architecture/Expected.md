---
title: Expected Architecture
description: Clean-room architecture derived from the product docs alone, with the two drafts' disagreements recorded.
tags: [meta, architecture]
status: clean-room
---

# Expected Architecture

This document is the merge of two clean-room architecture drafts for Crucible.
It describes the architecture the product documents imply. It does not describe
the code. Where the code differs from this document, the difference is a finding,
not an error in this document. See [[Product]], [[TUI User Stories]],
[[Web User Stories]] and [[Plugin User Stories]] for the source material.

A footnote marker such as `[D3]` points to a row in section 10. The row records
a point where the two drafts differed and the reason for the pick.

## 1. Method

Two agents worked in isolation. Each agent read six product documents only:
`Product.md`, `README.md`, `Terminology.md`, `TUI User Stories.md`,
`Web User Stories.md` and `Plugin User Stories.md`. Neither agent read source
code. Draft 1 worked from features inward to subsystems. Draft 2 worked from the
data model outward to operations. A third agent merged the two drafts. Where the
drafts agree, this document states the point once. Where the drafts differ, this
document uses one position in the body, marks it with `[Dn]`, and records both
positions in section 10. Both drafts assumed the same constraints: a Rust
workspace, one `cru` binary, a headless daemon with JSON-RPC 2.0 over a Unix
socket, thin TUI and web clients, Luau scripting, SQLite on the daemon
side, and the ACP and MCP protocols.

The clean room had one input class: feature documents. Neither draft read a
threat model, because the product documents carry none. As a result the drafts
derived every closed set from features alone. Section 8 shows the cost. The
drafts sized `ToolSurface`, `StageId` and `EventName` for the features, and
the code sizes them for the attacks: an unclassified tool, an isolated
session, a deleted file. A clean room needs the threat model as an input.
[[Filesystem Containment]] is that input now. Section 7a restates its
invariants and names the closed-set members each one requires.

## 2. Feature inventory

Each line names one user-facing feature and its source document. Source tags:
`P` = Product.md, `R` = README.md, `T` = TUI User Stories, `W` = Web User
Stories, `PL` = Plugin User Stories, `TM` = Terminology. The list is the union of
both drafts with duplicates removed. A feature marked *(planned)* has a product
entry but no shipped proof.

### 2.1 Notes and the knowledge graph

| # | Feature | Source |
|---|---------|--------|
| F1 | Wikilinks `[[note]]` with aliases `[[note\|alias]]` resolve to a target note | P, R, W |
| F2 | Wikilink fragments `[[Note#Section]]` and `[[Note#^id]]` parse; the target is the note | P |
| F3 | Link-preserving rename and move; inbound links are rewritten, decorations kept | P, W |
| F4 | Tags `#tag` and `#nested/tag` in the body and in frontmatter | P |
| F5 | Frontmatter in YAML (`---`) or TOML (`+++`) | P |
| F6 | Block references `^block-id` *(planned, parser only)* | P |
| F7 | Callouts `> [!type]` render in the web views | P |
| F8 | LaTeX inline and block render in the web views | P |
| F9 | Footnotes parse; render is *(planned)* | P |
| F10 | Tables render in the TUI as boxes and in the web as HTML; the editor realigns them | P |
| F11 | Task lists `- [ ]` and `- [x]` render | P |
| F12 | Task harness over `TASKS.md`: `cru tasks list\|next\|pick\|done\|blocked` with a dependency graph | P, R |
| F13 | Kilns: a directory with `.crucible/kiln.toml`; open, list, close | P, R, TM |
| F14 | JSON Canvas 1.0 read and write with a byte-identical round trip | P, W, PL |
| F15 | Canvases contribute file cards and text-card wikilinks to the graph | P |
| F16 | Plaintext first: markdown files are the source of truth; the index is rebuildable | P, R |
| F17 | Note types and templates *(planned)* | P |

### 2.2 Knowledge discovery

| # | Feature | Source |
|---|---------|--------|
| F18 | Semantic search over notes with a score per hit | P, R, W |
| F19 | Content search with ripgrep semantics, with line and offset data | P, W |
| F20 | Full-text search with FTS5 and BM25 over title and body | P |
| F21 | Knowledge graph: `kiln.graph` returns notes plus resolved and dangling links | P, W |
| F22 | Backlinks: linked mentions and unlinked mentions with byte spans | P, W |
| F23 | Graph traversal `cru.kiln.neighbors(path, depth)` from Lua | P |
| F24 | Property search by frontmatter field (AND) and by tag (OR) | P |
| F25 | Session search by text across transcripts | P, R |
| F26 | Auto-linking: `suggest_links` finds unlinked mentions; one click wraps the mention | P, W |
| F27 | Session semantic indexing *(planned)* | P |
| F28 | Block-level embeddings at paragraph granularity *(planned; one vector per note today)* | P, R |
| F29 | Document clustering and k-means *(planned)* | P |
| F30 | Query system *(planned)* | P |

### 2.3 Agent memory and context

| # | Feature | Source |
|---|---------|--------|
| F31 | Precognition: matching kiln notes are injected before the first user message of a session; `precognition_complete` carries the note list | P, R, W |
| F32 | Precognition toggle `:set precognition` in four spellings, session-scoped, daemon-synced | P, R, T |
| F33 | Precognition selection seam: a Lua `precognition_select` handler filters or reorders candidates | P |
| F34 | Memory scoping: a kiln-bound repository never returns a sibling workspace's notes | P |
| F35 | Knowledge insertion: the agent writes kiln notes with `create_note` and `update_note` | P |
| F36 | Proposal review: `cru proposals list\|show\|accept\|reject` over `KILN/.crucible/proposals/` | P |
| F37 | Reflection pass: on session end a cheap subagent proposes notes *(in progress)* | P |
| F38 | The precognition badge persists because `precognition_complete` is part of the session log | W |
| F39 | Anthropic cache control on the system prompt and the second-to-last turn | P |
| F40 | Cache statistics: `session.cache_stats`, `cru.session.cache_stats`, `sl.cache` | P, T |
| F41 | Token budget tracking with `context_budget` and a chars/4 estimate | P, T, W |
| F42 | Auto-compaction request at `context_budget * autocompact_threshold` *(in progress)* | P, T |
| F43 | Context strategies: Truncate, SlidingWindow, Summarize; Lua strategies *(planned)* | P |
| F44 | Lua context operations `cru.context.{usage, messages, remove, estimate_tokens}` | P |
| F45 | `cru.context.attach`: mid-turn attachment, deduplicated by key, capped by budget | P |
| F46 | Max iterations: a depth cap replays the prompt and the turn ends with text | P, W |
| F47 | Execution timeout `execution_timeout_secs` per turn | P, W |
| F48 | Turn undo `/undo [N]`: file rollback plus message truncation | P, T |
| F49 | Undo Lua API `cru.session.{undo, can_undo, undo_depth, undo_history}` | P |
| F50 | Output validation after each assistant turn, with retries | P, W |
| F51 | Lua validators `cru.context.register_validator` | P |

### 2.4 Chat, sessions and agents

| # | Feature | Source |
|---|---------|--------|
| F52 | Interactive chat with streamed text, thinking, tool calls and subagent events | P, R, T, W |
| F53 | Agent cards: persona files with prompt, model or specialty, tool policy, mode and MCP servers | P, R |
| F54 | Agent card discovery from three sites; `specialty:` resolves through `[llm.models]` | P, R |
| F55 | `cru session create --agent <card>` and `--acp <profile>` | P, R |
| F56 | Session persistence as append-only JSONL under the daemon data root | P, R, TM |
| F57 | Session resume with full history; `cru chat --resume <id>` | P, R, T, W |
| F58 | Sessions are always resumable; a send to an ended session revives it | P |
| F59 | Auto-title on the first completed turn by the `auto-title` plugin; `title_changed` broadcast | P, W |
| F60 | Auto-archive of idle sessions after `auto_archive_hours`; restore; delete | P, W |
| F61 | Segmented turn convergence: `segment_complete` at each text-to-tool boundary | P |
| F62 | A session-unique scratch workspace when no workspace is given | P |
| F63 | Conversation history: `:clear`; hydration from daemon events on resume | P, T |
| F64 | The draft survives streaming; Ctrl+Enter cancels and keeps the draft | P, T |
| F65 | An internal agent with session memory and tool access | P |
| F66 | Chat providers: Ollama, OpenAI, Anthropic, Cohere, VertexAI, OpenRouter, GitHubCopilot, ZAI; FastEmbed for embeddings | P, R |
| F67 | Model switching `:model <name>` and the web picker, with a lazy model list | P, T, W |
| F68 | Ctrl+T toggles the reasoning display; Crucible caps no reasoning | P, T |
| F69 | Layered system prompt: workspace, kiln, base prompt, rules files, skills catalog, deferral note | P |
| F70 | Environment overrides `--env KEY=VALUE` for an ACP subprocess | P |
| F71 | Agent cancellation from Esc, Ctrl+C or the web stop control | P, T, W |
| F72 | Error handling: toasts, retryability classes, transparent retry of idempotent RPCs | P, T |
| F73 | Provider error classification and fallback chains *(planned)* | P |
| F74 | Global estop sentinel *(planned)* | P |
| F75 | Session settings from the web: nine advanced knobs plus the mode | W |
| F76 | Session export to markdown: `:export`, `cru session export`, web download | P, R, T, W |
| F77 | Multi-kiln sessions: `connect_kiln`, `disconnect_kiln`, `set_workspace` | P, TM, W |
| F78 | File attachment `@file`, resolved on the daemon | P, T |
| F79 | Rules files `AGENTS.md`, `.rules`, `.github/copilot-instructions.md` in the prompt, root to workspace | P |

### 2.5 Tools, permissions and delegation

| # | Feature | Source |
|---|---------|--------|
| F80 | Tool calls correlated by `call_id`, streamed as `tool_call` then `tool_result` | P, T, W |
| F81 | Permission system: an ordered layer stack decides allow, deny or prompt | P, T, W |
| F82 | Pattern whitelisting: "always allow" saves a project-scoped pattern | P, T |
| F83 | Lua permission hooks `cru.permissions.on_request` with a 1 s budget | P |
| F84 | Permission prompts run one at a time with a 300 s deny timeout | P, T |
| F85 | Diff synthesis in permission prompts, with late diff updates | P, T, W |
| F86 | Interaction system: seven `InteractionRequest` kinds reach the attached client | P, T, W |
| F87 | Agent-initiated questions *(planned)* | P |
| F88 | Delegation `delegate_session` with depth, allowlist, concurrency, timeout and trust limits | P, R, T |
| F89 | Hidden child sessions; `session.list --include-children` | P |
| F90 | Background bash jobs: `list_jobs`, `get_job_result`, `cancel_job` | P, R |
| F91 | Repeat-failure tool blocking within one stream | P |
| F92 | Security enforcement: `[permissions]`, `[security.shell]` per chained statement, filesystem containment, derived trust | P |
| F93 | Prompt-injection scanning *(planned)* | P |
| F94 | Verification evidence ledger and verify-on-stop nudge *(planned)* | P |
| F95 | Per-session MCP servers named by an agent card | P |
| F96 | Tool discovery `discover_tools`, `get_tool_schema`, `invoke_tool` | P |
| F97 | Progressive tool disclosure when schemas exceed 15% of the budget | P |
| F98 | Agent skills: `SKILL.md` discovery across scopes, catalog in the prompt, `skill_view`, `cru skills list\|show\|search` | P, R, W |
| F99 | Chat modes `ask`, `plan`, `auto` declared in Lua; BackTab cycles; one slash command per mode | P, R, T |
| F100 | Active tool set narrowing `cru.tools.set_active` and `get_active` | P |
| F101 | Lua tool primitives `cru.tools.{call, batch, list}` under the operator's rules | P |
| F102 | Lua session primitives `cru.session.{messages, inject, fork, collect_subagents, subscribe, create}` | P |
| F103 | `cru.ui.{ask, ask_batch, edit, show, permission, popup, panel}` open a modal and await | P |
| F104 | Session event handlers: `turn:complete` can inject a follow-up message | P |
| F105 | Plugin-published session status `cru.plugin.set_status{}` | P |
| F106 | Scripted agent control: `session.mode`, `session.model`, `session.system_prompt` | P |

### 2.6 TUI

| # | Feature | Source |
|---|---------|--------|
| F107 | Input modes `>` chat, `:` command, `!` shell | P, T |
| F108 | Slash commands: built-ins, one per mode, plugin commands, else forward to the agent | P, T |
| F109 | REPL commands (section 8.12) | P, T |
| F110 | `:set` with `?`, `??`, `&`, `^`, `!`, `no` and `inv` forms; session keys sync to the daemon | P, T |
| F111 | `:lua <expr>` and `:=` evaluate on the daemon plugin VM | P, T |
| F112 | Double Ctrl+C quit within 300 ms | P, T |
| F113 | Bracketed paste | T |
| F114 | Token streaming with graduation to terminal scrollback | P, T |
| F115 | Thinking display with a word count; `:set thinking` | P, T |
| F116 | Markdown rendering with syntax colors | P, T |
| F117 | Statusline with mode, model, usage and cache; Lua `sl.setup{}` rows; daemon-pushed expressions | P, T |
| F118 | Lua UI bridge: colorscheme, highlight groups, geometry, hot reload | P, T |
| F119 | Terminal palette colors `term4`, `bright_*` | P |
| F120 | Tool call rows with summaries and source badges `[mcp:x]`, `[plugin:x]`, `[acp:agent]` | P, T |
| F121 | Turn indicator spinner | P |
| F122 | Tool output tail, buffer cap, spill to file above 10 KB | P, T |
| F123 | Subagent rows with status and elapsed time | P, T |
| F124 | `:mcp` server list with live status | P, T |
| F125 | ACP presentation parity with the internal agent | T |
| F126 | Permission modal: `y`, `n`, `a`, and `h` toggles the diff; prompts queue in order | P, T |
| F127 | Ask modal: single, multi, free text; the other interaction renderers | P, T |
| F128 | Diff preview, unified or side-by-side, syntax colored | P, T |
| F129 | `:set perm.show_diff`, `perm.autoconfirm_session`, `perm.full_commands` | P, T |
| F130 | Autocomplete for nine triggers; `:set completion_style` | P, T |
| F131 | Command palette on F1 | P, T |
| F132 | `:pick notes\|files\|commands` | P, T |
| F133 | Shell modal for `!cmd`: scroll, `i` inserts, `t` inserts truncated, `e` opens an editor; shell history | P, T |
| F134 | Toasts, messages drawer, warning badges | P, T |
| F135 | Keybindings, including a readline set | P |
| F136 | Bottom-anchored layout; input history with Up and Down | P |
| F137 | Replay mode `cru chat --replay` with speed and auto-exit | T |
| F138 | Stream gap warning when the daemon dropped events | T |
| F139 | Stable rendering at widths 50, 80 and 120 | T |
| F140 | Viewport caching of messages, tool calls, shell runs and subagents | P |
| F141 | Plugin load status `:plugins` and `:reload <plugin>` | P |
| F142 | Hero story: one session across the TUI and the web | T, W |

### 2.7 Web

| # | Feature | Source |
|---|---------|--------|
| F143 | Chat: stream, thinking, tool cards, permission and ask modals, model picker, cancel, export | P, W |
| F144 | Sessions: create, switch, resume, auto-title, archive, delete, settings; sessions dock in the right panel | W |
| F145 | Unified explorer over projects and kilns; `fs.list_dir`; live SSE patching | P, W |
| F146 | Editor: CodeMirror, tabs, dirty state, save, autosave, vim keys, live preview, reading view | P, W |
| F147 | Kiln-safe writes: traversal and oversize rejected | W |
| F148 | One kiln truth between chat and editor | W |
| F149 | Backlinks panel with one-click link; hover popovers as floating windows; follow links; wikilink completion | W |
| F150 | File drag: move on disk, open in pane, insert a link; `fs.move`, `fs.mkdir`, `fs.trash` | P, W |
| F151 | Right-click menus with native fall-through | W |
| F152 | Panel system: ribbons, edge panels, splits, floating windows, server-side layout `/api/layout` | P, W |
| F153 | Graph view over `kiln.graph` with physics and settings | P, W |
| F154 | Changes panel and inline review: `review.list_hunks`, `set_state`, review gate chips | W, P |
| F155 | Image viewer pane with zoom and pan | W |
| F156 | Inbox of pending interactions across sessions; attention badges | P, W |
| F157 | Omnibox Ctrl+P with `>` and `[[` scopes; note switcher Ctrl+O | P, W |
| F158 | Composer context chips; completion of commands, notes, files and tags | P, W |
| F159 | Status bar that knows the surface and the scope | W |
| F160 | Terminal over a PTY WebSocket; localhost only by default; opt-in remote | P, W |
| F161 | Canvas panel | P, W |
| F162 | Skills panel, plugins panel, search panel | P, W |
| F163 | Clone a repository with `scm.clone` and register it as a project | P, W |
| F164 | Auth: bearer key, cookie login, localhost bypass, `cru web key` | P, W |
| F165 | Voice input; rich renderers (mermaid, KaTeX, shiki); PWA | P |
| F166 | Settings panel: session knobs over RPC, UI knobs client-side | P, W |
| F167 | Reload keeps a pane bound to its session | W |
| F168 | Full-flow proof: a real `write_file` through a permission to bytes on disk | W |
| F169 | System info: health, ready, kilns, MCP status, plugin health | P |

### 2.8 Extensibility and plugins

| # | Feature | Source |
|---|---------|--------|
| F170 | Luau runtime; host-owned `require`; `io`/`os` compatibility from the host | P, R, PL |
| F171 | Plugin system: discovery on a search path, `plugin.yaml` manifest or bare `init.lua`, lifecycle, hot reload | P, R |
| F172 | Plugin spec table: `tools`, `commands`, `handlers`, `setup(cfg)` | P, R |
| F173 | Event hooks `cru.on(name, opts, handler)` with `pattern` and `priority` | P, R, PL |
| F174 | Note lifecycle events `note:created`, `note:modified`, `note:deleted`, `note:renamed` | P, PL |
| F175 | `FileChanged` workspace hook | P, T |
| F176 | `pre_tool_call` can cancel, transform or handle; `tool_result` can patch | P, R |
| F177 | Execution backends as plugins; the `oci` container plugin fails closed | P |
| F178 | Lua API modules under `cru.*` with the `crucible.*` alias | P |
| F179 | Timer, rate limit, retry, emitter and argument check helpers | P |
| F180 | `cru.storage` per-plugin key-value store | P |
| F181 | Plugin config: `[plugins.<name>]` TOML, then `setup{}` in `init.lua` wins | P |
| F182 | Lua config beats TOML; `cru.config` reads and writes app config | P |
| F183 | `chat.system_prompt` session default tier; `cru.modes` mode declarations | P |
| F184 | Plugin-declared commands reachable as `/name` and over RPC | P |
| F185 | Plugin file watcher `[plugins] watch = true` | P |
| F186 | HTTP client `cru.http` | P |
| F187 | LuaCATS type stubs generated at daemon start; `.luarc.json` scaffold | P |
| F188 | `cru lua` CLI and the `lua.eval` RPC | P |
| F189 | `cru plugin new\|test\|stubs\|add\|list\|remove\|update\|health` | P, R |
| F190 | Clean Lua error messages with plugin, file and line | P, PL |
| F191 | Plugin test harness with mocked `cru.*` | P, PL |
| F192 | `cru.service` long-running services with `start`, `stop` and `health` | P |
| F193 | `cru.schedule({every=N}, fn)` and `[[schedules]]` config | P |
| F194 | Webhook `POST /api/webhook/:name` to `webhook:received` | P |
| F195 | Bundled runtime plugins inside the binary; extraction on first start | P |
| F196 | Plugin search path with shadow-by-name | P |
| F197 | Templates, dry run, undo, registry, canvas builder, manifest permissions *(planned)* | PL |
| F198 | Discord plugin; Telegram and Matrix *(planned)* | P |

### 2.9 Protocols

| # | Feature | Source |
|---|---------|--------|
| F199 | ACP host: spawn external agents over stdio JSON-RPC and negotiate capabilities | P, R |
| F200 | ACP context injection: precognition as a tagged system block | P |
| F201 | In-process MCP host over HTTP/SSE; stdio fallback `cru mcp --stdio --standalone` | P |
| F202 | Agent discovery for `opencode`, `claude`, `gemini`, `codex`, `cursor`; `[acp.agents.*]` with `extends` | P, R |
| F203 | ACP permission gate through the session policy; no handler means deny | P |
| F204 | ACP streaming with diff handling; cancel closes the transport | P |
| F205 | ACP model switching `session/set_config_option` | P |
| F206 | ACP recording and replay `CRUCIBLE_ACP_RECORD_DIR` | P |
| F207 | ACP agent mode `cru acp` for editors | P, R |
| F208 | MCP server `cru mcp`: note, search, kiln, delegation and job tools | P, R |
| F209 | Workspace tools `read_file`, `edit_file`, `write_file`, `bash`, `glob`, `grep` for the internal agent only | P |
| F210 | MCP gateway to upstream servers with prefixed names; auto-reconnect; per-session filter from cards | P |
| F211 | Lua plugin tools appear in the agent tool list | P, R |
| F212 | `readOnlyHint` lowers an upstream tool's safety class | P |
| F213 | TOON formatting for plugin tool results | P |

### 2.10 Storage, daemon and configuration

| # | Feature | Source |
|---|---------|--------|
| F214 | SQLite backend with notes, links, properties, blocks and tags tables; FTS; vectors | P |
| F215 | Vector embeddings with FastEmbed or Ollama; `batch_size` | P |
| F216 | File processing pipeline `cru process`; hash-based change skip; `force_reprocess` | P, R |
| F217 | Kiln statistics `cru stats`; storage status `cru status`; `cru doctor`; `cru models`; `cru auth login` | P, R |
| F218 | Daemon server; `daemon.capabilities` lists the methods | P |
| F219 | Daemon client: auto-spawn, version check, restart on SHA mismatch | P, R |
| F220 | Event subscriptions per session or `*`; stream gap markers | P, T |
| F221 | Notification RPC: add, list, dismiss | P |
| F222 | File watching with auto-reprocess | P, R |
| F223 | Storage maintenance `verify`, `cleanup`, `backup`, `restore` | P, R |
| F224 | Git: `scm.clone` in the daemon; branches and worktrees in the `worktree` plugin | P, W |
| F225 | Config system: TOML with `{file:}`, `{dir:}`, `{env:}`; CLI overrides; `cru config show --trace` | P, R |
| F226 | Provider, embedding, agent, MCP, project and storage config sections | P |
| F227 | Project registry `project.register\|list\|get\|unregister` with `.crucible/project.toml` | P, TM, W |
| F228 | `cru init`, `cru setup`, the first-run wizard, kiln discovery by upward walk, kiln path validation | P, R |
| F229 | CLI help with examples, prefix inference, typo suggestions | P |
| F230 | Workflows: markdown DAGs, parallel steps, gates, resume from a snapshot; `cru workflow` | P |
| F231 | Remote access: API key, tunnels *(planned)* | P |
| F232 | Documentation site | P |

## 2a. Features found in code, absent from the product docs

The clean room could list only what the product documents name. The code
carries surfaces that [[Actual]] sections 3 and 5 found live, with a
registration the daemon runs at start. This table lists each one. The third
column says whether `Product.md` names the feature. A `no` row is a product
gap, not an expectation gap: the feature exists, and no document promised it.
Rows marked `yes` are in section 2 already; they appear here because the lead
review named them as suspects, and the check found them covered.

| Feature | Surface that exposes it | In Product.md |
|---|---|---|
| Lua webhook handlers: `webhook:received` from `POST /api/webhook/:name` and the `webhook.receive` RPC | `crates/crucible-web/src/routes/webhook.rs:47`; `crates/crucible-daemon/src/rpc/dispatch.rs:234`; `crates/crucible-lua/src/handlers/hook_name.rs:60` | yes (F194) |
| Webhook secret minting and HMAC verification | `crates/crucible-daemon/src/webhook/mod.rs:359`; `crates/crucible-daemon/src/webhook/mod.rs:256` | yes, as `webhook.receive`; the secret file is not named |
| `cru.http` client | `crates/crucible-lua/src/http.rs:47`; registered at `crates/crucible-lua/src/executor.rs:290` | yes (F186) |
| `cru.session.*`, 28 names over `DaemonSessionApi` | `crates/crucible-lua/src/sessions/register.rs:154`; wired at `crates/crucible-daemon/src/daemon_plugins/mod.rs:572` | yes (F102) |
| `cru.schedule` interval callbacks | `crates/crucible-daemon/src/daemon_plugins/mod.rs:212` | yes (F193) |
| `cru.ratelimit` | `crates/crucible-lua/src/ratelimit.rs:117`; registered at `crates/crucible-lua/src/executor.rs:293` | yes (F179) |
| MCP gateway: upstream servers with prefixed names, reconnect loop, gateway tools on the served MCP surface | `crates/crucible-daemon/src/tools/mcp_gateway.rs:486`; `crates/crucible-daemon/src/tools/extended_mcp_server.rs:123` | yes (F210); the wiring landed in Tier 3 A10 and A11 |
| Auto-title plugin over the `session_title` publication channel | `crates/crucible-daemon/src/agent_manager/title.rs:42` | yes (F59) |
| Publications: `cru.plugin.publish(key, value)` stored by the daemon and served by `plugin.publications` | `crates/crucible-lua/src/publications.rs:100`; `crates/crucible-daemon/src/daemon_plugins/mod.rs:966`; `crates/crucible-daemon/src/server/plugins.rs:156` | yes, since 2026-08-23 (Plugin Publications) |
| Plugin options: `cru.plugin.options{}` declared once, served by `plugin.options`, `plugin.option_get`, `plugin.option_set`, `plugin.option_execute` | `crates/crucible-lua/src/options/mod.rs:389`; `crates/crucible-daemon/src/daemon_plugins/mod.rs:973`; `crates/crucible-daemon/src/server/plugins.rs:187` | yes, since 2026-08-23 (Plugin Options) |
| Provider auth hooks: `cru.on_provider_auth(fn)` | `crates/crucible-lua/src/auth_plugin.rs:9`; registered at `crates/crucible-lua/src/executor.rs:259` | yes, since 2026-08-23 (Provider Auth Hooks) |
| `cru.log.notify`, `cru.log.notify_once`, `cru.log.messages.*` | `crates/crucible-lua/src/notify.rs:30`; registered at `crates/crucible-lua/src/executor.rs:260` | yes, since 2026-08-23 (Lua Notifications, `[-]`). The queue reaches no client (G122) |
| Isolation claim: `cru.isolation.require{}` | `crates/crucible-lua/src/isolation.rs:208`; wired at `crates/crucible-daemon/src/daemon_plugins/mod.rs:240` | yes, since 2026-08-23 (Isolation Claims) |
| Plugin status slots: `cru.plugin.set_status{}`, `cru.plugin.clear_status` | `crates/crucible-lua/src/plugin_status.rs:118`; wired at `crates/crucible-daemon/src/daemon_plugins/mod.rs:253` | yes (F105) |
| Statusline expressions pushed from the daemon | `crates/crucible-lua/src/statusline_exprs.rs:190`; wired at `crates/crucible-daemon/src/daemon_plugins/mod.rs:361` | yes (F117) |
| `cru.context.attach` registry | `crates/crucible-lua/src/context_attach.rs:171`; wired at `crates/crucible-daemon/src/daemon_plugins/mod.rs:350` | yes (F45) |
| `cru.ws` WebSocket client | `crates/crucible-lua/src/ws.rs:187`; wired at `crates/crucible-daemon/src/daemon_plugins/mod.rs:199` | yes, in the module list only |
| `cru.oq` multi-format parse and jq-style query | `crates/crucible-lua/src/json_query.rs:284`; wired at `crates/crucible-daemon/src/daemon_plugins/mod.rs:204` | yes, in the module list only |
| `cru.shell` with a plugin shell policy | `crates/crucible-lua/src/shell.rs:366`; wired at `crates/crucible-daemon/src/daemon_plugins/mod.rs:202` | yes, in the module list only |
| Session lifecycle hooks `cru.on_session_start`, `cru.on_session_end` | `crates/crucible-lua/src/hooks.rs:34`; registered at `crates/crucible-lua/src/executor.rs:258` | yes |
| Review comments and rebase: `review.comment`, `review.resolve_comment`, `review.rebase` | `crates/crucible-daemon/src/rpc/dispatch.rs:181`; `crates/crucible-daemon/src/server/session/review/mod.rs:371` | yes, since 2026-08-23 (Review Comments and Rebase) |
| `session.export_to_file`: a transcript written to a caller path under write protection | `crates/crucible-daemon/src/rpc/dispatch.rs:172`; `crates/crucible-daemon/src/server/observe.rs:295` | yes, since 2026-08-23 (Session Export to a Path) |
| Plugin RPC management: `plugin.install`, `plugin.remove`, `plugin.run_command` | `crates/crucible-daemon/src/rpc/dispatch.rs:194` | yes, since 2026-08-23 (Plugin Install names the RPCs) |

`Product.md` gained one row per former `no` or `partly` line on 2026-08-23 (plan T5-38); section 2 of this document still lacks them. Section 9 should
gain "a plugin option" and "a publication" as extension points.

## 3. Domain entities

Each entity has one owner. The owner is the only subsystem that writes the
entity. Type names are proposals. They name the shape a clean design needs.

### 3.1 Kiln

A kiln is a directory of notes. It holds knowledge. A session attaches kilns. A
session is never stored in a kiln.

```rust
pub struct KilnName(String);          // registry key, unique per daemon
pub struct KilnId(Uuid);              // stable across a directory rename
pub struct Kiln {
    id: KilnId,
    name: KilnName,
    root: PathBuf,                    // holds .crucible/kiln.toml
    classification: DataClass,
    index_state: IndexState,          // Unindexed | Indexing { done, total } | Ready { generation }
}
pub enum DataClass { Public, Internal, Confidential }
```

Lifecycle: `Registered` → `Open` → `Closed`. `kiln.open` starts an index pass.
`kiln.close` releases the watcher and the SQLite handle. Many sessions can hold
one kiln open at the same time.

Owner: **KilnRegistry**.

### 3.2 Project

A project is where work output goes. It is a registered directory with
`.crucible/project.toml`.

```rust
pub struct ProjectId(Uuid);
pub struct Project {
    id: ProjectId,
    root: PathBuf,                    // git root or invocation dir
    attached_kilns: Vec<KilnName>,    // declared in project.toml
    security: ProjectSecurity,        // shell policy, project_files allow list
    patterns: PatternStore,           // saved "always allow" patterns
}
```

Lifecycle: `Registered` → `Unregistered`. No other state.

Owner: **ProjectRegistry**. The project registry is separate from the kiln
registry. [D23]

### 3.3 Workspace

A workspace is an instance of a project directory: the root or a worktree. It
has no config file. [D16]

```rust
pub struct Workspace {
    path: PathBuf,                    // canonical
    kind: WorkspaceKind,              // ProjectRoot(ProjectId) | Worktree { project, branch } | Scratch(SessionId)
}
```

Lifecycle: a scratch workspace lives as long as its session. A project
workspace has no lifecycle of its own.

Owner: **SessionManager** creates scratch workspaces. **ProjectRegistry**
resolves project workspaces. The `worktree` plugin creates worktrees.

### 3.4 Note

A note is one markdown file inside a kiln. The parser produces a `ParsedNote`.
The index stores a `Note` row.

```rust
pub struct NoteKey { kiln: KilnId, rel_path: RelPath }   // the identity
pub struct ParsedNote {
    frontmatter: Option<Frontmatter>,  // Yaml(Map) | Toml(Map)
    title: Option<String>,
    blocks: Vec<Block>,
    wikilinks: Vec<LinkOccurrence>,   // raw, unresolved
    tags: Vec<Tag>,
    callouts: Vec<Callout>,
    footnotes: FootnoteMap,
    tasks: Vec<TaskItem>,
    content_hash: ContentHash,
}
pub struct Note {
    key: NoteKey,
    title: String,
    content_hash: ContentHash,        // drives change detection
    frontmatter: Map,
    tags: Vec<Tag>,
    blocks: Vec<Block>,
    mtime: SystemTime,
    scope: Scope,                     // Kiln(KilnId) | Workspace { path }
}
pub struct Block { hash: ContentHash, span: ByteSpan, kind: BlockKind }
pub struct ByteSpan { start: usize, end: usize }
pub struct ContentHash([u8; 32]);
pub struct LinkOccurrence {
    span: ByteSpan,
    raw_target: String,               // text before '#' and '|'
    alias: Option<String>,
    fragment: Option<Fragment>,       // Heading(String) | BlockRef(String)
    embed: bool,
}
```

Lifecycle: `Created` → `Modified`* → `Deleted`. A rename is `Deleted` plus
`Created` plus `Renamed`. The parser never changes a note. The file on disk wins
over the index.

Owner: **NoteStore** for the rows. The filesystem holds the truth.

### 3.5 Resolved link

A resolved link is a row in the link index. The parser does not know it. [D17]

```rust
pub struct ResolvedLink {
    source: NoteKey,
    span: ByteSpan,
    form: LinkForm,                   // Bare | Path | Alias | Heading | Block | Embed
    target: LinkTarget,
}
pub enum LinkTarget { Resolved(NoteKey), Ambiguous(Vec<NoteKey>), Dangling(String) }
pub struct Backlinks { linked: Vec<ResolvedLink>, unlinked: Vec<UnlinkedMention> }
pub struct UnlinkedMention { target: NoteKey, span: ByteSpan, text: String }
```

Lifecycle: rebuilt on every change of the source; re-resolved by target key when
a candidate appears or disappears.

Owner: **LinkIndex**.

### 3.6 Canvas

A `.canvas` file in a kiln. Nodes of kind text, file, link and group. Labelled
edges. The reader keeps key order and unknown keys, so the writer gives back the
same bytes.

```rust
pub struct CanvasDoc { path: NoteKey, raw: Vec<u8>, nodes: Vec<CanvasNode>, edges: Vec<CanvasEdge>, unknown: PreservedKeys }
```

Lifecycle: the same as a note. A file node and a wikilink inside a text node
give `ResolvedLink` rows.

Owner: **CanvasIndex** for links; **NoteStore** for the bytes.

### 3.7 Embedding

An embedding is a separate entity, not a field on the note. The `block` field
lets block-level retrieval ship without a schema change. [D18]

```rust
pub struct Embedding { note: NoteKey, block: Option<ContentHash>, model: ModelId, dims: u16, vector: Vec<f32> }
```

Lifecycle: computed after parse when a provider exists; deleted with the note.
The product today collapses block vectors to one note vector.

Owner: **EmbeddingStore**.

### 3.8 Session

A session is one conversation with one agent. It lives in the daemon.
Transcripts live under the daemon data root.

```rust
pub struct SessionId(String);        // "chat-<timestamp>-<random>"
pub struct Session {
    id: SessionId,
    title: Option<String>,           // None until auto-title or a manual set
    parent: Option<SessionId>,       // set for a delegated child
    workspace: Workspace,
    kilns: BTreeSet<KilnName>,       // flat set, no primary
    agent: SessionAgent,             // 3.9
    config: SessionConfig,           // 3.10
    mode: ModeId,
    state: SessionState,
    archived: bool,
    created: SystemTime,
    last_activity: SystemTime,
}
pub enum SessionState { Active, Paused, Streaming { turn: TurnId }, Ended }
```

Lifecycle: `Active` ↔ `Paused`; `Active` → `Streaming` → `Active` per turn; any
state → `Ended`. A message to an `Ended` session revives it from storage.
`archived` is a flag, not a state. A child session ends with its parent. There is
no `Compacting` state. Compaction is an operation on the conversation tree. [D24]

Owner: **SessionManager**.

### 3.9 SessionAgent

`SessionAgent` is an enum. An ACP agent has no provider and no temperature. An
enum removes those `None` paths. [D1]

```rust
pub enum SessionAgent { Internal(InternalAgent), Acp(AcpAgent) }
pub struct InternalAgent {
    provider: ProviderName,
    model: ModelId,
    card: Option<AgentCardName>,
    system_prompt: Option<String>,
    tool_policy: ToolPolicy,
    mcp_servers: Vec<UpstreamName>,  // filter over the gateway
    generation: GenerationSettings,  // temperature, max_tokens
}
pub struct AcpAgent {
    profile: AcpProfileName,
    env: BTreeMap<String, String>,
    remote_session_id: Option<String>,
    advertised_models: Vec<ModelId>,
    current_model: Option<ModelId>,
}
```

Lifecycle: built at `session.create`; rebuilt on `switch_model`, `set_mode`
or a card change. A rebuild invalidates the cached handle.

Owner: **SessionManager** holds the record. **AgentFactory** builds the handle.

### 3.10 SessionConfig

The session-scoped knobs. Every knob round-trips through RPC under one field
name. Every knob survives resume.

```rust
pub struct SessionConfig {
    precognition: bool,              // default true
    context_budget: Option<u32>,
    context_window: Option<u32>,
    context_strategy: ContextStrategy,
    autocompact_threshold: f32,      // default 0.95
    max_iterations: Option<u32>,     // default 10
    execution_timeout_secs: Option<u32>,
    validation_retries: u32,
    output_validation: OutputValidation,
}
pub enum ContextStrategy { Truncate, SlidingWindow, Summarize, Lua { name: String } }
pub enum OutputValidation { None, Lua { name: String } }
```

Owner: **SessionManager**.

### 3.11 Turn

A turn starts with one user message. It ends when the agent stops.

```rust
pub struct TurnId(u32);              // monotonic per session
pub struct Turn {
    id: TurnId,
    session: SessionId,
    snapshot: Option<WorkspaceSnapshot>,   // git write-tree, or an in-memory journal <= 5 MiB
    segments: Vec<SegmentId>,
    tool_calls: Vec<CallId>,
    blocked_tools: HashSet<ToolName>,      // repeat-failure block list for this stream
    attached: BTreeMap<AttachKey, String>, // cru.context.attach, this turn only
    usage: Usage,
    outcome: TurnOutcome,
}
pub enum TurnOutcome { Completed, Cancelled, Error(String), DepthCapped, ValidationExhausted, Timeout }
pub struct UndoRecord { turn: TurnId, messages_removed: u32 }
pub struct CacheStats { read_tokens: u64, creation_tokens: u64, completions: u32 }
```

Lifecycle: `Preparing` → `Streaming` → `ToolRound`* → `Validating` → `Done`.
Undo removes turns from the tail.

Owner: **TurnLoop**.

### 3.12 ConversationTree and ContextMessage

The prompt for the next call is built from a tree, not from the log. The log is
for persistence and replay.

```rust
pub struct MessageId(String);        // backend-canonical when the backend gives one
pub struct ContextMessage {
    id: MessageId,
    role: Role,                      // System | User | Assistant | Tool
    content: Vec<ContentPart>,       // Text | Thinking | ToolCall | ToolResult | Attachment
    tag: Option<ContextTag>,         // Precognition | Attachment | Rules | Skills | Injected
    timestamp: SystemTime,
}
pub struct ConversationTree { nodes: Vec<ContextMessage>, path: Vec<MessageId> }
```

Lifecycle: append on each message; rewind on undo; `remove` from Lua; `compact`
replaces a range with one summary message. `path` is the prompt.

Owner: **TurnLoop**.

### 3.13 SessionEvent

The one type every wire surface shares. Everything a client renders is an event.
Everything persisted is an event.

```rust
pub struct SessionEventMessage { session: SessionAddr, seq: u64, at: Timestamp, event: SessionEvent }
pub enum SessionAddr { Id(SessionId), Wildcard }
pub enum SessionEvent {
    // turn
    TextDelta { segment: SegmentId, text: String },
    ThinkingDelta { segment: SegmentId, text: String },
    SegmentComplete { segment: SegmentId, message: MessageId },
    ToolCall { call: CallId, name: ToolName, args: Value, source: ToolSource, diffs: Vec<Diff> },
    ToolCallDiffUpdate { call: CallId, diffs: Vec<Diff> },
    ToolResult { call: CallId, result: ToolOutcome },
    MessageComplete { message: MessageId, full_response: String, usage: Usage, cache: CacheCounts },
    UsageUpdate { usage: Usage, limit_source: ContextLimitSource },
    Ended { turn: TurnId, reason: TurnOutcome },
    // interaction
    InteractionRequested { request: InteractionRequest },
    InteractionResolved { id: RequestId },
    // knowledge
    PrecognitionComplete { notes: Vec<PrecognitionNoteInfo> },
    ContextInjected { key: String },
    NoteEvent(NoteEvent),
    FileChanged { path: PathBuf },
    // delegation
    SubagentSpawned { child: SessionId, prompt: String, target: DelegateTarget },
    SubagentCompleted { child: SessionId, result: String },
    SubagentFailed { child: SessionId, error: String },
    // session meta
    TitleChanged { title: String },
    ModeChanged { mode: ModeId },
    ModelChanged { model: ModelId },
    ConfigChanged { key: String, value: Value },
    StatusPublished { plugin: String, key: String, text: String, level: Level },
    UndoComplete { records: Vec<UndoRecord> },
    ReviewGate { state: GateState, file: Option<RelPath> },
    ReviewChanged,
    Notification(Notification),          // [D14]
    WebhookReceived { name: String },
    // transport
    StreamGap { dropped: Option<u64> },
    UiStyleChanged { payload: UiConfig },
}
```

Lifecycle: emitted once; broadcast to subscribers; appended to the session log
when the event is on the persist allow-list. `StreamGap` is never persisted.
`PrecognitionComplete` is persisted, because the badge must survive a reload.

Owner: **EventBus** for delivery. **SessionLog** for persistence.

### 3.14 ToolCall

```rust
pub struct CallId(String);
pub enum ToolSource { Builtin, Plugin(PluginName), McpUpstream(UpstreamName), Acp(AcpProfileName) }
pub struct ToolCall { id: CallId, name: ToolName, args: Value, source: ToolSource, gate: GateDecision }
pub enum GateDecision { Allowed(Layer), Denied(Layer, String), Prompted(RequestId) }
pub enum ToolOutcome { Ok(Value), Err(String), Blocked(String), Spilled { path: PathBuf, head: String } }
```

`ToolSource` has four variants. A built-in tool shows no badge. Every other
source shows a badge. [D9]

Lifecycle: `Proposed` → `Gated` → (`Executing` | `Denied`) → `Resulted`. A tool
that fails again and again in one stream moves to the per-stream block list.

Owner: **ToolDispatch**.

### 3.15 InteractionRequest

```rust
pub struct RequestId(Uuid);
pub struct InteractionRequest { id: RequestId, session: SessionId, kind: InteractionKind, timeout: Duration }
pub enum InteractionKind {
    Permission { call: CallId, name: ToolName, args: Value, diffs: Vec<Diff>, scopes: Vec<AllowScope> },
    Ask { question: String, options: Vec<String>, multi: bool, allow_other: bool },
    AskBatch { questions: Vec<InteractionKind> },
    Edit { title: String, initial: String },
    Show { title: String, body: String },
    Popup { title: String, items: Vec<String> },
    Panel { title: String, items: Vec<String>, multi: bool, filter: bool },
}
pub enum InteractionResponse {
    Permission { decision: PermDecision },   // AllowOnce | AllowSession | AllowProject | Deny
    Ask { selected: Vec<String>, other: Option<String> },
    AskBatch(Vec<InteractionResponse>),
    Edit { text: String },
    Acknowledged,
    Popup { selected: Option<String> },
    Panel { selected: Vec<String> },
    Cancelled,
}
```

`Popup` carries items. [D12] The permission response is one enum. [D13]
`Cancelled` is a response, not an error.

Lifecycle: `Pending` → (`Answered` | `TimedOut` | `Cancelled`). Permission
requests are serialized per session. Other kinds are not.

Owner: **InteractionBroker**.

### 3.16 AgentCard

```rust
pub struct AgentCardName(String);
pub struct AgentCard {
    name: AgentCardName,
    description: String,             // the only required field
    system_prompt: Option<String>,
    model: Option<ModelId>,
    specialty: Option<String>,       // resolved through [llm.models]
    tool_policy: ToolPolicy,
    mode: Option<ModeId>,
    mcp_servers: Vec<UpstreamName>,
    generation: GenerationSettings,
    source: CardSource,              // UserConfig | Kiln(KilnName) | Project(ProjectId)
}
```

Lifecycle: discovered at session creation from three sites. A later site shadows
an earlier one. No runtime mutation.

Owner: **AgentCardRegistry**.

### 3.17 AcpProfile

```rust
pub struct AcpProfileName(String);
pub struct AcpProfile {
    name: AcpProfileName,
    extends: Option<AcpProfileName>,
    command: Vec<String>,
    env: BTreeMap<String, String>,
    trust: DataClass,
    delegation: DelegationLimits,    // depth, allowlist, max_concurrent, timeout
    installed: Option<PathBuf>,      // absolute path from discovery
}
```

Lifecycle: built-in profiles plus `[acp.agents.*]`. Discovery probes once per
resolved environment.

Owner: **AcpHost**.

### 3.18 Provider and Model

```rust
pub struct ProviderName(String);
pub enum ProviderKind { Ollama, OpenAi, Anthropic, Cohere, VertexAi, OpenRouter, GitHubCopilot, Zai, FastEmbed }
pub struct ProviderConfig { name: ProviderName, kind: ProviderKind, endpoint: Option<Url>, api_key: Option<Secret>, default_model: Option<ModelId> }
pub struct ModelId(String);
pub struct ModelInfo { id: ModelId, provider: ProviderName, context_limit: Option<u32> }
```

`ProviderKind` has nine variants. `FastEmbed` implements only the embedding
backend. [D8]

Lifecycle: loaded from config. The model list is fetched on demand and cached
per provider.

Owner: **ProviderRegistry**.

### 3.19 Skill

```rust
pub struct SkillName(String);
pub struct Skill { name: SkillName, description: String, path: PathBuf, scope: SkillScope, allowed_tools: Vec<ToolName>, shadowed_by: Option<SkillScope> }
pub enum SkillScope { Personal, Workspace, Kiln(KilnName), Bundled }
```

Four scopes. [D6] A higher scope shadows a lower one. The body loads on
`skill_view` only.

Owner: **SkillRegistry**.

### 3.20 Plugin

```rust
pub struct PluginName(String);
pub struct Plugin {
    name: PluginName,
    version: Version,
    source: PluginSource,            // EnvPath | User | RuntimePath(PathBuf) | Runtime
    dir: PathBuf,
    language: ScriptLang,            // Luau
    state: PluginState,
    last_error: Option<String>,
    declares: PluginDecl,            // tools, commands, services, hooks, config schema
}
pub enum PluginState { Discovered, Loaded, Active, Error, Unloaded }
```

`PluginSource` has four variants. [D7]

Lifecycle: `Discovered` → `Loaded` → `Active` → `Unloaded` → `Loaded` again on
reload. A failed reload leaves the old plugin `Active`.

Owner: **PluginHost**.

### 3.21 Hook registration

```rust
pub struct HookId(u64);
pub struct HookReg { id: HookId, plugin: Option<PluginName>, name: HookName, pattern: Option<Glob>, priority: i32 }
pub enum HookName { Stage(StageId), Event(EventName) }
pub enum StageResult { Continue, Cancel, Handled { result: Value }, Transform { args: Value } }
```

A stage is synchronous. Its return value decides the next step. An event is a
broadcast. Only `cancel` means anything in its return value.

Lifecycle: registered at plugin load or from `init.lua`; removed at unload.

Owner: **HookRegistry** inside PluginHost.

### 3.22 Mode

```rust
pub struct ModeId(String);
pub struct ModeDecl { id: ModeId, tools: ToolSelector, stance: ModeStance, rules: Vec<PermRule>, label: String, color: Color }
pub enum ModeStance { Allow, Ask, Deny }
```

Lifecycle: declared in Lua with `cru.modes.<id> = {...}`. The shipped three are
data in the default `init.lua`. An unknown mode id fails closed.

Owner: **ModeRegistry** inside PluginHost. **PermissionEngine** and
**ToolDispatch** read it.

### 3.23 Job

```rust
pub struct JobId(Uuid);
pub struct Job { id: JobId, session: SessionId, command: String, state: JobState, output: OutputBuffer }
pub enum JobState { Running, Done { exit: i32 }, Cancelled, Failed(String) }
```

Owner: **JobManager**.

### 3.24 Notification

```rust
pub struct NotificationId(Uuid);
pub struct Notification { id: NotificationId, session: Option<SessionId>, kind: NotificationKind, message: String, created: SystemTime }
pub enum NotificationKind { Toast, Progress, Warning }
```

Owner: **SessionManager** stores them. Clients render them. [D22]

### 3.25 Proposal

```rust
pub struct ProposalId(String);
pub struct Proposal { id: ProposalId, kiln: KilnName, staged_path: PathBuf, target: RelPath, provenance: Provenance }
```

Lifecycle: `Staged` in `KILN/.crucible/proposals/`, outside the index →
`Accepted` (moved into the kiln, provenance removed) or `Rejected` (moved into `KILN/.crucible/proposals/rejected/`, so the reviewer does not propose it again).

Owner: **ProposalStore** inside Knowledge.

### 3.26 ReviewHunk

```rust
pub struct HunkId(String);           // derived from file plus range, not a call id
pub struct ReviewHunk { id: HunkId, session: SessionId, file: PathBuf, range: LineRange, before: String, after: String, origin: HunkOrigin, state: HunkState }
pub enum HunkOrigin { Tool(CallId), External }
pub enum HunkState { Unreviewed, Accepted, Rejected }
```

The queue is the composed diff from the session base to the worktree. It is not
a list of tool calls.

Owner: **ReviewLedger**.

### 3.27 Workflow

```rust
pub struct WorkflowRunId(Uuid);
pub struct WorkflowDef { source: NoteKey, steps: Vec<WorkflowStep>, gates: Vec<Gate>, validation: Vec<ValidationEntry> }
pub struct WorkflowStep { name: String, agent: Option<String>, output: Option<String>, kind: StepKind, parallel_group: Option<u32> }
pub struct WorkflowSnapshot { run: WorkflowRunId, session: SessionId, position: StepCursor, scope: BTreeMap<String, String> }
```

Lifecycle: `Parsed` → `Running` → (`AtGate` ↔ `Running`) → (`Done` | `Failed` |
`Cancelled`). A snapshot on disk rehydrates a run after a daemon restart.

Owner: **WorkflowEngine** (pure, in core) and **WorkflowRunner** (daemon).

### 3.28 Schedule and Service

```rust
pub struct ScheduleHandle(u32);
pub struct ScheduleSpec { every: Duration, action: ScheduleAction }   // Lua callback or "lua:<code>"
pub struct ServiceDesc { name: String, plugin: PluginName, state: ServiceState }
```

Owner: **PluginHost / Scheduler**.

### 3.29 UiConfig

```rust
pub struct UiConfig { colorscheme: Colorscheme, groups: HighlightGroups, geometry: UiGeometry, statusline: StatuslineRegions, exprs: BTreeMap<String, String> }
pub struct StatuslineRegions { top: Vec<Row>, prompt: Vec<Row>, bottom: Vec<Row> }
```

Lifecycle: computed in the plugin VM from `init.lua` and themes; pushed to every
client on change as `UiStyleChanged`.

Owner: **PluginHost / Theme projection**. A client holds a copy only.

### 3.30 Task file and task graph

```rust
pub struct TaskFile { phases: Vec<Phase>, tasks: Vec<Task> }
pub struct TaskGraph { /* dependency edges over Task ids */ }
```

Owner: **Parser** produces them. The `cru tasks` command reads them.

## 4. Subsystems

Each subsystem has one responsibility, the entities it owns, the operations it
exposes, and the things it must never know. The decomposition follows draft 2.
The grouping follows draft 1. [D15]

### 4.1 Parser

- Responsibility: turn bytes into `ParsedNote`, `CanvasDoc`, `TaskFile` and `WorkflowDef` with byte spans. No resolution.
- Owns: nothing persistent. Produces `ParsedNote`, `Block`, `LinkOccurrence`, `Frontmatter`, `Tag`.
- Operations: `parse_note(text) -> ParsedNote`, `parse_canvas(bytes) -> CanvasResult<CanvasDoc>`, `write_canvas(&CanvasDoc) -> Vec<u8>`, `parse_tasks(bytes) -> TaskFile`, `parse_workflow(note) -> WorkflowDef`, `hash_blocks(&ParsedNote) -> Vec<Block>`.
- Must never know: kilns, other notes, paths on disk, SQLite, link resolution, sessions, embeddings, the daemon. The parser sees one string at a time.

### 4.2 Knowledge

Knowledge is four parts under one name. A change in one part does not reach the
others.

**4.2.1 KilnRegistry**

- Responsibility: kiln identity and open state.
- Owns: `Kiln`.
- Operations: `kiln.open`, `kiln.close`, `kiln.list`, `kiln.info`, `find_enclosing_kiln(path)`, `open_registered(project)`.
- Must never know: note contents, embeddings, sessions, the LLM, the wire.

**4.2.2 NoteStore and NotePipeline**

- Responsibility: keep the SQLite index equal to the files.
- Owns: `Note` rows, `Block` rows, FTS rows, `Proposal`.
- Operations: `process_file`, `process_batch`, `list_notes(scope)`, `get_note_by_name`, `read`, `create`, `update`, `delete`, `search_text`, `property_search`, `verify`, `backfill_text_index`.
- Pipeline stages: read → hash compare (skip when equal unless forced) → parse → index rows → link index update → embedding when a provider exists → emit `note:*` events. The pipeline reports `ProcessingResult { skipped, indexed, events }`.
- Must never know: the session model, permissions, the wire format.

**4.2.3 LinkIndex and CanvasIndex**

- Responsibility: resolve link occurrences to note keys; keep backlinks; splice on rename.
- Owns: `ResolvedLink`.
- Operations: `resolve_all(source)`, `reresolve_target(key)`, `backlinks(key) -> Backlinks`, `graph(kiln) -> {notes, links}`, `neighbors(key, depth)`, `rename(key, new_key) -> RenameReport`, `suggest_links(note) -> Vec<UnlinkedMention>`.
- Rename rule: splice inbound links in descending byte-span order. Skip an `Ambiguous` target. Reindex the touched notes.
- Must never know: the parser's internals beyond `LinkOccurrence`; embeddings.

**4.2.4 EmbeddingStore and Retrieval**

- Responsibility: vectors in, ranked notes out.
- Owns: `Embedding`.
- Operations: `embed(texts) -> Vec<Vec<f32>>`, `upsert(note, vectors)`, `search_vectors(query_vec, scope, limit) -> Vec<SearchHit>`, `semantic_search(query, scope, limit)`.
- The `EmbeddingBackend` trait has `embed`, `dimensions` and `batch_size`. `FastEmbedBackend` (local ONNX) is the default. `OllamaEmbedBackend` is the other implementation.
- Must never know: who asks. The scope filter is a parameter applied in SQL.

### 4.3 ProjectRegistry

- Responsibility: which directories are projects; per-project security and patterns.
- Owns: `Project`, `PatternStore`.
- Operations: `project.register|list|get|unregister`, `is_registered_root(path)`, `patterns(project)`.
- Must never know: sessions, kilns beyond their names.

### 4.4 Containment

- Responsibility: one allow-list of roots per session. Every path a tool reaches goes through one capability handle.
- Owns: nothing persistent. `ContainmentRoots { workspace, kilns, session_dir }` and `CapabilityHandle`.
- Operations: `roots(session)`, `contain(session, path) -> Result<CanonPath>`, `contain_glob`, `contain_parent`, `is_protected(path)` for `.crucible/*` and transcripts.
- Rules: canonicalize, then check containment. Drop a symlink that escapes. Reject `..` before disk access. Hide dotfiles and gitignored files by default. Exclude `.crucible/trash` from the index and the watcher.
- Must never know: tool names. It answers for a path, not for an intent.

### 4.5 FsOps

- Responsibility: filesystem mutations the clients ask for.
- Operations: `fs.list_dir`, `fs.move`, `fs.mkdir`, `fs.trash`, `fs.read`, `fs.write`, `note.rename`, `note.move`. The last two call `LinkIndex.rename`.
- Must never know: the web. It takes a session or a registered root plus a relative path.

### 4.6 Watcher

- Responsibility: turn a filesystem change into pipeline work and a `FileChanged` event.
- Operations: `watch(kiln)`, `unwatch(kiln)`, debounce, `ignored?` via git.
- Must never know: what a note is.

### 4.7 SessionManager

- Responsibility: the session record and its lifecycle; notifications; scratch workspaces.
- Owns: `Session`, `SessionConfig`, `Notification`, the `session_kilns` index.
- Operations: `session.create|get|list|pause|resume|end|archive|unarchive|delete|set_title|set_workspace|connect_kiln|disconnect_kiln`; every `session.set_*` and `session.get_*` knob; `session.list_modes|set_mode|get_mode`; `session.switch_model|list_models`; `session.pending_interactions`; `session.cache_stats`; `session.search`; `session.render_markdown`; `session.export`; `session.fork`; notification add, list and dismiss; the title sweep; the archive sweep; revive on send.
- Must never know: provider wire formats, tool execution, rendering.

### 4.8 SessionLog

- Responsibility: an append-only JSONL file per session; load for resume and replay.
- Owns: the files under `<data_root>/sessions/<id>/session.jsonl` and `review.jsonl`.
- Operations: `append(event)`, `load_events(id)`, `render_markdown(events)`, `search(query)`.
- Must never know: clients. The log is not inside any kiln, so a kiln can be shared without its transcripts.

### 4.9 TurnLoop

- Responsibility: run one turn. Assemble context, call the provider, dispatch tools, validate, emit events, persist.
- Owns: `Turn`, `ConversationTree`, the per-stream block list, `WorkspaceSnapshot`, attachments.
- Operations: `send_message(session, text, attachments)`, `cancel(session)`, `undo(session, n)`, `inject(session, content)` for the next turn, `attach(session, key, content)` for this turn, `compact(session)`.
- Stage order: `session_start` once → `turn_start` → precognition (first user message only) → `precognition_select` → `@file` attachments → rules files → skills catalog → `transform_context` → provider call → stream deltas → `segment_complete` at a text-to-tool boundary → per call: `pre_tool_call` → gate → execute → `tool_result` → depth check → loop → `validate_output` → `turn_complete` → persist → `session_end` on end. [D3] [D25]
- The handle contract is one trait with three required methods: [D2]

```rust
pub trait AgentHandle {
    fn send(&self, msg: UserMessage, sink: EventSink) -> AgentResult<()>;
    fn cancel(&self) -> AgentResult<()>;
    fn configure(&self, agent: &SessionAgent, config: &SessionConfig) -> AgentResult<()>;
}
```

- Every knob flows through `configure` with the whole record. A new knob cannot compile without a path to the handle.
- Must never know: SQLite, the socket framing, HTTP, terminal cells, how a Lua table is shaped. It knows the hook runtime as a trait.

### 4.10 AgentFactory and ProviderRegistry

- Responsibility: build a provider handle from `SessionAgent`, `SessionConfig`, the card, the rules and the skills; compose the system prompt; size the tool set.
- Owns: `ProviderConfig`, `ModelInfo`, a handle cache keyed by `(session, agent hash)`.
- Operations: `build_handle(session)`, `invalidate(session)`, `providers.list`, `models(provider)`, `enforce_context_budget`, `visible_tools(mode, active_set, budget)`, `apply_prompt_caching`.
- The backend trait:

```rust
pub trait ChatBackend {
    fn stream(&self, req: ChatRequest) -> BoxStream<Result<StreamChunk, ChatError>>;
    fn list_models(&self) -> ChatResult<Vec<ModelInfo>>;
    fn context_limit(&self, model: &ModelId) -> Option<u32>;
}
pub struct ChatRequest { system: Option<String>, messages: Vec<ContextMessage>, tools: Vec<ToolDefinition>, options: ChatOptions }
pub struct ChatOptions { temperature: Option<f32>, max_tokens: Option<u32>, reasoning: Option<ReasoningEffort>, cache_breakpoints: Vec<usize> }
pub enum StreamChunk { Text(String), Thinking(String), ToolCall(ToolCall), Usage(Usage), Done { reason: StopReason } }
pub enum ChatError { RateLimited { retry_after: Option<Duration> }, Auth, Network, Provider(String) }
```

- Must never know: Lua, sessions, kilns, tool bodies, permissions. It receives `ToolDefinition`s and returns `ToolCall`s. It never executes one.

### 4.11 ToolDispatch and ToolRegistry

- Responsibility: the one enumerated table of tools; execution with the gate in front.
- Owns: `ToolCall`, `ToolSource`, `BuiltinTool`, `ToolDefinition { name, description, parameters, source, read_only_hint, deferrable }`, `ExecutionContext { session, workspace, kilns, session_dir, mode, active_set, interactive }`.
- Operations: `list(session) -> Vec<ToolDefinition>`, `dispatch(session, call)`, `discover_tools`, `get_tool_schema`, `invoke_tool` (unwrapped to the inner tool before hooks and the gate), `set_active`, `get_active`.
- `pre_tool_call` handlers run before the gate for `cancel` and `transform`. A `handled` result returns before the gate. The design admits it only when the session carries an isolation claim that the plugin published through `set_status`. The dispatch layer refuses an isolation claim for an ACP session. [D21] That ordering is one function with one test.
- Must never know: how a client renders; which wire the call came from.

### 4.12 PermissionEngine

- Responsibility: evaluate one `(tool, args)` against one ordered rule stack. Answer `Allow`, `Deny` or `Ask` with the layer that decided. [D11]
- Owns: nothing persistent. Reads `Project.patterns`, `ModeDecl` and config.
- Operations: `decide(ctx) -> GateDecision`, `store_pattern(project, pattern)`, `check_shell(command) -> per-statement verdicts`.
- Layer order: `is_safe` → CLI `--permissions` override → global `[permissions]` (deny is absolute, allow short-circuits) → project `PatternStore` → Lua `on_request` hooks → mode rules → mode stance → non-interactive deny → prompt with a 300 s deny timeout.
- `DiffSynthesizer` attaches a `Diff` to a write or edit prompt.
- Must never know: the UI. It never prompts. It returns `Ask`. ToolDispatch prompts.

### 4.13 InteractionBroker

- Responsibility: correlated one-reply-with-timeout requests from the daemon to the attached client.
- Owns: the `InteractionRequest` pending table; the permission serializer per session.
- Operations: `request(kind, timeout) -> await InteractionResponse`, `respond(id, response)`, `pending(session?)`, `cancel(id)`.
- Must never know: what the answer means.

### 4.14 Delegation and JobManager

- Responsibility: child sessions and background shell jobs.
- Owns: the parent-child map, `Job`.
- Operations: `delegate_session(target, prompt, opts)`, `collect(ids, timeout)`, `list_jobs`, `get_job_result`, `cancel_job`.
- Limits with agent-visible errors: depth, allowlist, self-delegation, concurrency, timeout, and data-class trust derived from the child's real provider.
- Must never know: rendering. It emits `Subagent*` events.

### 4.15 Precognition

- Responsibility: pick notes for the first turn; run the `precognition_select` seam; emit `PrecognitionComplete`.
- Operations: `enrich(session, message) -> Vec<ContextMessage>` through Retrieval with the session's kiln scope and `max_precognition_chars`.
- Must never know: providers.

### 4.16 SkillRegistry, AgentCardRegistry and RulesLoader

- Responsibility: the things that reach the system prompt.
- Operations: `discover_skills(session)`, `skill_view(name)`, `skills.list|search|show`; `discover_cards(session)`, `resolve_card(name)`; `load_rules(workspace) -> Vec<RulesFile>` from root to workspace.
- A future prompt-injection scanner sits here. It annotates local content and blocks fetched content.
- Must never know: the turn loop.

### 4.17 PluginHost (Lua)

- Responsibility: the daemon plugin VM and the per-session VM; discovery, load, reload, hooks, modes, defaults, services, schedules, theme projection, type stubs.
- Owns: `Plugin`, `HookReg`, `ModeDecl`, `ScheduleSpec`, `ServiceDesc`, `UiConfig`, `cru.storage` keys.
- Operations: `plugin.list|reload|install|remove|commands|run_command|options|set_option|publications`, `lua.eval`, `lua.init_session`, `fire_stage(stage, ctx) -> StageResult`, `broadcast_event(event)`, `modes()`, `stubs.generate`.
- Two VMs exist. The daemon plugin VM runs plugins, `cru lua` and `:lua`. The session VM runs `session.*` hooks. `cru.*` and `crucible.*` name the same tables.
- Projection modules are safe alone: theme, statusline, geometry, oil, json, fs, notify, paths. Interception modules are capability-grade: `pre_tool_call` handled or transform, `on_request`, `precognition_select`, `transform_context`, validators, strategies.
- The shipped `init.lua` is compiled into the binary. It is the only definition of the three modes, the plan-mode deny hook, the default system prompt and the precognition formatter. `ModeRegistry` has no Rust fallback.
- It calls back into the daemon through a `DaemonBridge` trait: sessions, tools, kiln, context, ui, storage.
- Must never know: SQLite tables, the socket, terminal cells.

### 4.18 EventBus

- Responsibility: fan-out of `SessionEventMessage` to subscribers with a per-connection cursor. Lag becomes `StreamGap`.
- Operations: `subscribe(addrs) -> Stream`, `emit(msg)`, `persist_filter(event) -> bool`.
- Must never know: what an event means; how a client paints a modal.

### 4.19 AcpHost

- Responsibility: Crucible as an ACP client. Spawn, handshake, prompt, stream, permissions, model switch, cancel, recording and replay.
- Owns: `AcpProfile`, the child process, the agent's remote session id, `AcpRecording`.
- Operations: `discover()`, `spawn(profile, env) -> AcpHandle` where `AcpHandle: AgentHandle`; `session/new`, `session/prompt`, `session/cancel`, `session/set_config_option` (the model selector the agent lists in `configOptions`), `session/set_mode`; handle `session/request_permission` through InteractionBroker and PermissionEngine; map `session/update` frames to `SessionEvent` with `ToolSource::Acp(profile)`.
- Transport for Crucible's tools to the agent: in-process MCP over HTTP/SSE when the agent supports it, else stdio.
- The ACP wire types come from the `agent_client_protocol` crate. They never leave this subsystem.
- Must never know: the internal provider path, SQLite, kiln internals, the TUI. It reaches tools only through the in-process MCP host.

### 4.20 AcpAgentServer

- Responsibility: Crucible as an ACP agent (`cru acp`). Map `session/new` to `session.create`, `session/prompt` to `send_message`, and events to `session/update`.
- Must never know: anything but the daemon RPC.

### 4.21 McpServer and McpGateway

- McpServer: serve the kiln surface (note tools, search tools, `get_kiln_info`, `delegate_session`, job control, `skill_view`) and every plugin tool to external agents over stdio or HTTP/SSE. Never serve workspace tools.
- McpGateway: connect upstream servers from `[mcp]` config, prefix their tools, filter per session by card, reconnect with backoff, report `UpstreamStatus { name, state, tool_count }`. One gateway per daemon.
- The MCP wire types come from the `rmcp` crate. They never leave this subsystem.
- Must never know: rendering, conversation trees, Lua.

### 4.22 ReviewLedger

- Responsibility: the composed diff from the session base to the worktree; hunk state; `review.jsonl`.
- Operations: `review.list_hunks`, `review.set_state(hunk, state)` (reject reverts on disk in the same call), `review.comment(range, text)`, `review.resolve(comment)`, the gate (`ReviewGate` events). An ACP session degrades to review at turn end.
- Must never know: the editor.

### 4.23 WorkflowRunner

- Responsibility: run a `WorkflowDef` with parallel joins and gates; persist snapshots; rehydrate.
- Operations: `workflow.list|show|start|approve|status|cancel`.

### 4.24 Scm

- Responsibility: `scm.clone` with URL hardening and destination containment. Branches and worktrees are a plugin.

### 4.25 Config

- Responsibility: evaluate `init.lua` once at boot and merge the layers into one store with per-leaf provenance — defaults, plugin defaults, `settings.json`, the human's Lua lines, CLI flags, the runtime knob; validate; reject legacy keys with an actionable error. `config.toml` is not a config source: the reader is gone and only `cru config migrate` still parses the file.
- Owns: `CliAppConfig`, one canonical struct; `ConfigStore`; `SourceTag` and `ProvenanceMap`.
- Operations: `config.effective`, `config.get`, `config.set` (runtime, in memory), `config.save` (durable, `settings.json`, refuses a pinned leaf), `config.origin`, `config.controls`.
- Must never know: runtime state.
- See [[Config Boot]] for the sequence, the layer order and the two verbs.

### 4.26 Daemon server and RPC client

- Responsibility: bind the socket; authenticate by uid through socket permissions; dispatch JSON-RPC to the subsystems; stream events; report `daemon.capabilities`; end itself on a signal or after `server.idle_shutdown_minutes` idle. On the client side: auto-spawn, version check, and reaping a spawned daemon that never became reachable.
- Owns: `SocketPath`, `RpcMethod`, `RpcRequest`, `RpcResponse`, `RpcError { code, message, data }`, `Capabilities { methods, build_sha }`, `Subscription`.
- Operations: `Server::bind_with_data_home(data_home, config)`, `DaemonClient::connect_or_start()`, `DaemonClient::call<T>(method, params)`, `DaemonClient::subscribe(targets) -> EventStream`. Idempotent methods retry twice on a transport timeout.
- Must never know: domain logic. A handler is a thin translation from params to one subsystem call.

### 4.27 Clients

- TUI: an `OilChatApp` state machine over `SessionEvent`s; an Oil renderer with Taffy layout; graduation to scrollback; modals in the footer slot; a `Vt100` test runtime. Pure display state (theme copy, `show_thinking`, `completion_style`) stays local. Anything multi-client goes to the daemon.
- Web server: Axum routes that translate HTTP to RPC; three SSE streams; one PTY WebSocket; bearer and cookie auth; SSRF validation; wire-shape normalization; layout file persistence.
- Web frontend: SolidJS panels in one registry; a window manager; CodeMirror; TypeScript extensibility.
- CLI commands: thin callers of RPC.
- Must never know: SQL, tool execution, the permission stack, link resolution. A client that needs to duplicate daemon logic is in the wrong place.

## 5. Seams

Each seam names what crosses, the direction, and whether the call blocks.

| # | Seam | What crosses | Direction | Sync |
|---|------|--------------|-----------|------|
| S1 | Parser → NotePipeline | `ParsedNote`, `CanvasDoc` with byte spans | one way | sync |
| S2 | NotePipeline → LinkIndex | `NoteKey` plus `Vec<LinkOccurrence>` | one way | sync, same transaction |
| S3 | NotePipeline → EmbeddingStore | `NoteKey` plus texts | one way | async, after index commit |
| S4 | NotePipeline → EventBus | `note:*`, `process_complete` | fan-out | async |
| S5 | Watcher → NotePipeline | `FileChanged { path, kind }` | one way | async, debounced |
| S6 | Watcher → PluginHost | `FileChanged` event | fan-out | async |
| S7 | SessionManager → SessionLog | `SessionEventMessage` on the persist allow-list | one way | sync append |
| S8 | TurnLoop → AgentFactory | `(SessionAgent, SessionConfig, card, rules, skills)` | request | sync build, cached |
| S9 | AgentFactory → Provider | `ChatRequest` | request | async stream of `StreamChunk` |
| S10 | TurnLoop → Precognition | `(SessionId, first message)` | request | async |
| S11 | Precognition → Retrieval | `(query, Scope, limit)` | request | async |
| S12 | TurnLoop → PluginHost | `fire_stage(StageId, ctx)` | request with reply | sync, ordered by priority |
| S13 | TurnLoop → ToolDispatch | `ToolCall` | request | async per call, sequential |
| S14 | ToolDispatch → PermissionEngine | `(session, tool, args, mode)` | request | sync |
| S15 | ToolDispatch → InteractionBroker | `InteractionKind::Permission` | request with one reply | async, 300 s timeout |
| S16 | InteractionBroker → EventBus | `InteractionRequested`, `InteractionResolved` | fan-out | async |
| S17 | Client → InteractionBroker | `respond(id, InteractionResponse)` | request | sync |
| S18 | ToolDispatch → Containment | `(session, path)` | request | sync |
| S19 | ToolDispatch → Knowledge | note tool calls as typed operations | request | async |
| S20 | ToolDispatch → JobManager | `bash` in the background | request | async |
| S21 | ToolDispatch → Delegation | `delegate_session` | request | async; the blocking variant waits |
| S22 | Delegation → SessionManager | `session.create` with `parent` | request | async |
| S23 | Delegation → EventBus | `Subagent*` on the parent's address | fan-out | async |
| S24 | TurnLoop → SessionManager (snapshot) | `PathBuf` in; `WorkspaceSnapshot` out; restore | request | async |
| S25 | EventBus → Daemon server | `SessionEventMessage` per subscriber cursor | fan-out | async; lag → `StreamGap` |
| S26 | Daemon server → TUI, Web, CLI | JSON-RPC replies and event notifications | both | request sync, events async |
| S27 | Web server → Browser | SSE frames, HTTP JSON, one PTY WebSocket | both | SSE async |
| S28 | AcpHost → External agent | ACP JSON-RPC over stdio | both | request sync, updates async |
| S29 | External agent → In-process MCP host | MCP over HTTP/SSE or stdio | request | async |
| S30 | AcpHost → TurnLoop | `SessionEvent` with `ToolSource::Acp` | one way | async |
| S31 | AcpHost → PermissionEngine plus InteractionBroker | `session/request_permission` | request with reply | async |
| S32 | PluginHost → ToolDispatch | `cru.tools.call\|batch` through the same gate, no prompt fallback | request | async |
| S33 | PluginHost → SessionManager | `cru.session.*` through a `DaemonBridge` | request | async |
| S34 | PluginHost → InteractionBroker | `cru.ui.*` | request with reply | async, not serialized |
| S35 | PluginHost → EventBus | `UiStyleChanged`, `StatusPublished`, `ConfigChanged` | fan-out | async |
| S36 | PluginHost ↔ Knowledge | `cru.kiln.search\|neighbors` out; note events in | both | async |
| S37 | EventBus → PluginHost | `EventName` events | fan-out, `cancel` only | async |
| S38 | McpGateway → AgentFactory | `Vec<ToolDefinition>` with prefixes | one way | sync at build |
| S39 | McpServer → ToolDispatch | `ToolCall` in; `ToolOutcome` out, gate applied | request | async |
| S40 | ReviewLedger ↔ TurnLoop | `ReviewGate` blocks a write; call ids stamp hunks | both | sync check |
| S41 | Config → every subsystem | `AppConfig` by value at bind | one way | sync at startup |
| S42 | CLI → Daemon server | connect or spawn; version check | request | sync with retry |
| S43 | TurnLoop → SkillRegistry | search paths in; `Vec<Skill>` out | request | sync |

Three rules hold across every seam:

1. `SessionEventMessage` is the only type two wire bindings share. No codec, framing or error class is shared.
2. A request that needs a reply goes through a pending-reply registry with a timeout. A fan-out never waits.
3. A stage result decides what happens next. An event result is read by nothing except `cancel`. `Handled` from a `pre_tool_call` hook returns before the gate. The gate order in the turn loop is the protection against escalation.

## 6. Wire surfaces

### 6.1 Daemon JSON-RPC over the Unix socket

Transport: one socket per uid, mode 0700. The path comes from `$CRUCIBLE_SOCKET`,
else `$XDG_RUNTIME_DIR`, else `<tmpdir>/crucible-<uid>/`. Notifications carry
`SessionEventMessage`. The method list is in section 8.5.

Exposed types: `Session`, `SessionAgent` and `SessionConfig` fields (one getter
and one setter per knob, the same field name both ways), `SessionEventMessage`,
`InteractionRequest`, `InteractionResponse`, `Note`, `SearchHit`, graph edges,
`Backlinks`, `Kiln`, `Project`, `ToolDefinition`, `Plugin`, `Skill`, `UiConfig`,
`Notification`, `UpstreamStatus`, `Capabilities`, `UndoRecord`, `CacheStats`,
`ReviewHunk`, `WorkflowSnapshot`.

Private: `ConversationTree`, `WorkspaceSnapshot`, `CapabilityHandle`,
`PatternStore` internals, `ContainmentRoots`.

Errors: `{ code, message, retryable }`. The client retries a retryable transport
error twice for an idempotent method.

### 6.2 Web HTTP, SSE and WebSocket

Routes mirror the RPC families one to one. A knob the daemon advertises must
have a route. A test derives the route set from the daemon's method list. Each
route uses the daemon's field name.

```
GET  /health  /ready
POST /api/auth/login  /api/auth/logout
GET  /api/session  POST /api/session  GET /api/session/:id  DELETE /api/session/:id
POST /api/session/:id/{send,cancel,pause,resume,archive,unarchive,export,title,workspace,kiln,mode,model}
GET  /api/session/:id/{history,models,mode,config}  PUT /api/session/:id/config/:knob
GET  /api/chat/events/:id              (SSE)
GET  /api/interactions/pending  POST /api/interaction/respond
GET  /api/commands
GET  /api/kilns  GET /api/kiln/graph  GET /api/kiln/file  PUT /api/kiln/file
GET  /api/notes  GET /api/notes/:name  PUT /api/notes/:name  GET /api/notes/resolve  GET /api/backlinks
POST /api/search/{grep,semantic,vectors}
GET  /api/fs/list  POST /api/fs/{move,mkdir,trash}  GET /api/fs/events   (SSE)
GET  /api/canvas  PUT /api/canvas
GET  /api/skills  GET /api/skills/search  GET /api/skills/:name
GET  /api/plugins  POST /api/plugins  DELETE /api/plugins/:name  POST /api/plugins/:name/reload
GET  /api/plugins/{publications,options}  POST /api/plugins/:name/option  POST /api/plugins/command
GET  /api/review/hunks  POST /api/review/hunk/:id/state  POST /api/review/comment
GET  /api/scm/branches  POST /api/scm/worktree  POST /api/scm/clone
GET  /api/layout  POST /api/layout  DELETE /api/layout
GET  /api/config  POST /api/config  GET /api/mcp/status
POST /api/webhook/:name
POST /exec  (SSE)   GET /api/terminal/ws  (WebSocket; localhost, or remote_shell opt-in)
GET  /  and static assets, SPA fallback
```

SSE streams: `/api/chat/events/:id` carries `SessionEventMessage` with
`InteractionRequested` flattened to the browser shape; `/api/fs/events` carries
`NoteEvent` and `FileChanged`; `/exec` carries shell output.

Private to the web: the flattened interaction shape
`{request_id, kind, tool, args, diff, scope_options}`; the SSE frame names
`token|thinking|tool_call|tool_result|message_complete|title_changed|precognition_result|review_changed|stream_gap`;
the layout file; the PTY lifecycle; cookie sessions; SSRF validation.

Shared with the daemon: `SessionEventMessage` at the input of the translator.
Nothing else.

### 6.3 ACP

Crucible as host: sends `initialize` with no filesystem capabilities,
`session/new`, `session/prompt`, `session/cancel`, `session/set_config_option`
(for the model selector the agent lists in `configOptions`), `session/set_mode`. Receives `session/update` (text, thought, `tool_call`,
`tool_call_update` with diffs) and `session/request_permission`. Sends the
precognition block as a tagged system content item. Serves MCP to the agent.

Crucible as agent (`cru acp`): the mirror over stdio framing. `loadSession` is
advertised. `SessionEvent` maps to `agent_message_chunk`, `agent_thought_chunk`,
`tool_call` and `tool_call_update`.

Types: owned by the `agent_client_protocol` crate, not by Crucible. One
translator maps them to `SessionEvent`. A recording and replay harness covers
the translator. `ToolSource::Acp(profile)` is the only render difference.

Private: `SessionAgent`, `ConversationTree`. The ACP agent owns its own history.

### 6.4 MCP

Served: `create_note`, `read_note`, `read_metadata`, `update_note`,
`delete_note`, `list_notes`, `semantic_search`, `grep_notes`, `property_search`,
`get_kiln_info`, `delegate_session`, `list_jobs`, `get_job_result`,
`cancel_job`, `skill_view`. Fifteen built-in tools, plus every Lua plugin tool.
[D20] Workspace tools are never served. Results are JSON. A TOON formatter is an
option per tool.

Consumed: upstream servers from `[mcp.upstreams]`, prefixed `<name>_<tool>`.
`readOnlyHint` lowers the safety class.

Types: owned by the `rmcp` crate. Crucible maps them to `ToolDefinition` and
`ToolOutcome`.

### 6.5 Shared across all four

One type is shared: `SessionEventMessage` and its `SessionEvent` payload. The
daemon RPC and the web SSE carry it as JSON. The ACP host and the ACP agent
translate to and from it. The MCP surface reads it only for `delegate_session`
results. Each surface has its own framing, error codes and correlation ids.
`RpcError` codes, HTTP status codes, ACP `-32601` and MCP error content are four
separate mappings.

## 7. Crate layout

Fewer, larger crates. A crate is a compilation unit and a dependency firewall.
It is not an organization tool.

| Crate | Holds | Reason for the split |
|-------|-------|----------------------|
| `crucible-core` | Parser, canvas read and write, task file, workflow engine (pure), domain types from section 3 with no I/O, `SessionEvent`, `InteractionRequest`, `ContextMessage`, `Scope`, `KilnName`, hashing, config loader, runtime root resolution, theme and statusline wire types, the embedded default runtime tree | Every other crate needs the types. The parser must compile without SQLite, tokio or Lua. |
| `crucible-lua` | The Luau VM, the module resolver, `cru.*` module projections, hook and mode registries, test runner, stub and declaration generator | Lua bindings need `mlua` and must not pull the daemon. Depends on `crucible-core` only. [D19] |
| `crucible-oil` | Terminal rendering primitives: nodes, Taffy layout, styles, palette mapping, graduation planning, `render_to_string` | Pure rendering with tests that need no terminal. Depends on `crucible-core` only. [D19] |
| `crucible-daemon` | Sections 4.2 to 4.26: storage (SQLite, FTS, vectors), pipeline, watcher, session manager and log, turn loop, agent factory, providers, tools, permissions, interactions, delegation, jobs, skills, cards, rules, plugin host glue, event bus, ACP host, ACP agent server, MCP server and gateway, review, workflow runner, scm, RPC dispatch and the RPC client | All storage lives here. All business logic lives here. The RPC client lives here so the CLI and the web share one client. |
| `crucible-web` | Axum server, routes, middleware, SSE translators, PTY, asset embedding; the SolidJS app under `web/`, built with bun and embedded with rust-embed | Needs axum, tower, rust-embed and a JavaScript build. Behind a default-on `web` feature of the binary, so a build without bun still works. |
| `crucible-cli` | The `cru` binary: clap commands, `OilChatApp`, chat runner, modals, autocomplete, shell modal, REPL, setup wizard, doctor, the `cru acp`, `cru mcp` and `cru web` launchers | The only binary. Thin. Owns the terminal. |
| `vendor/markdown-it` | One patched crate | Isolated patches with `NOTE(crucible):` comments and regression tests. |

Dependency direction: `core` ← `lua`; `core` ← `oil`; `core`, `lua`, `oil` ←
`daemon` ← `web` ← `cli`. `lua` and `oil` do not depend on each other.

Test support: a `test-utils` feature on `crucible-daemon` exposes `TestDaemon`
(child-scoped env, temp data root), `PromptCapturingAgent`, a fake Ollama server
and `mock-acp-agent`. The TUI exposes `Vt100TestRuntime`, `AppHarness` and JSONL
fixtures.

The planned `crucible-telegram` and `crucible-matrix` crates should not be
crates. They are Lua plugins over `cru.service` and `cru.http`, as the Discord
plugin is.

## 7a. Security invariants

[[Filesystem Containment]] and [[Actual]] section 3.1 state the invariants the
code enforces. The clean room did not derive them, because no product document
states them. Each invariant below names the closed-set members it requires. A
closed set that lacks the member cannot carry the invariant, so the member is
not optional.

**I1. Roots are a default-deny allowlist.** A session holds a set of allowed
roots. A path outside every root is refused. A denied root survives only as a
carve-out inside an allowed root. The judge reads a path once into a lexical
form and a canonical form, and asks both. Inside by name but outside when
resolved is `SymlinkEscape`, not a silent refusal.
(`crates/crucible-daemon/src/tools/containment.rs:152`,
`crates/crucible-daemon/src/tools/containment.rs:272`,
`crates/crucible-daemon/src/tools/path_resolution.rs:174`.)
Requires: `Containment` with a distinct `SymlinkEscape` outcome
(`crates/crucible-daemon/src/tools/containment.rs:89`); `RootSet` with
`Ambient` and `Rooted`; seam S18 carries a path through `FsScope` only
(`crates/crucible-daemon/src/tools/fs_scope.rs:162`). Section 4.4 should say
"allowlist", not "capability handle over a deny list".

**I2. A read proof and a write proof are distinct types.** `FsScope::resolve`
returns `ContainedPath`; `FsScope::resolve_for_write` returns `WritablePath`.
Neither has a public constructor or a `From<PathBuf>`. A signature that takes
one carries a compiler-checked proof that containment ran.
(`crates/crucible-daemon/src/tools/fs_scope.rs:92`,
`crates/crucible-daemon/src/tools/fs_scope.rs:130`,
`crates/crucible-daemon/src/tools/fs_scope.rs:273`.)
Requires: no `From<ContainedPath> for WritablePath`; `grep resolve_for_write`
enumerates the write surface. `bash` is outside this layer
(`crates/crucible-daemon/src/tools/fs_scope.rs:64`); the invariant covers the
file tools, not the session.

**I3. An unclassified tool surface is refused.** `BuiltinTool::surface` is one
exhaustive table. A name with no row is `ToolSurface::Unknown`. The isolation
gate refuses `Unknown` as it refuses `Host`.
(`crates/crucible-daemon/src/tools/surface.rs:220`,
`crates/crucible-core/src/traits/tools.rs:56`,
`crates/crucible-daemon/src/agent_manager/messaging/isolation_gate.rs:16`.)
Requires: `ToolSurface` is `Host`, `Daemon`, `Unknown`, and not the `Daemon`,
`Mcp`, `Both` of section 8.1; MCP exposure is a second predicate (G8). The
isolation stage is a member of the gate order. `IsolationRegistry` and
`cru.isolation.require` (`crates/crucible-lua/src/isolation.rs:208`) are
the Lua side; the `oci` plugin is the one caller.

**I4. `handled` returns before the permission gate.** A `pre_tool_call`
handler that returns `{ handled = true, result = … }` ends the call before
the permission gate runs. Only the statement order in `tool_call.rs` prevents
a plugin from escalating through it.
(`crates/crucible-daemon/src/agent_manager/messaging/tool_call.rs:289`.)
Requires: the gate order is fixed and documented: plan-mode bar, active-tool
set, card policy, review gate, `pre_tool_call` hooks, isolation gate,
permission gate, dispatch. `PreToolCall` stays a `StageId` with a reply
(`cancel`, `transform`, `handled`); `cancel` is safe, `handled` and
`transform` are capability-grade. Section 5 rule 3 and section 9.3 already
say this; section 8.2 should mark `Handled` as the one return a mode may
refuse.

**I5. The socket is per uid and 0700.** The daemon binds under
`$CRUCIBLE_SOCKET`, else `$XDG_RUNTIME_DIR`, else `<tmpdir>/crucible-<uid>/`.
The directory is created `0700`; a directory with a foreign owner is refused.
The accept loop checks the peer uid before it dispatches.
(`crates/crucible-core/src/protocol/lifecycle.rs:61`,
`crates/crucible-daemon/src/server/socket_privacy.rs:63`,
`crates/crucible-daemon/src/server/socket_privacy.rs:95`,
`crates/crucible-daemon/src/server/core/mod.rs:86`.)
Requires: `SocketDirRefusal` as a closed set of refusal reasons; seam S42
includes the uid check, not only the version check; section 6.1 should say
the RPC surface is unauthenticated and therefore owner-only.

**I6. A destructive sink asserts canonical equality.** `remove_session_dir`
canonicalizes the root and the target and refuses unless the target equals
the expected session directory. "Beneath a root" is not enough for
`remove_dir_all`. `session.export_to_file` resolves the caller path through
write protection before it writes.
(`crates/crucible-daemon/src/session_manager.rs:120`,
`crates/crucible-daemon/src/server/observe.rs:295`.)
Requires: `SessionId` is a validated newtype with `parse`
(`crates/crucible-core/src/session/types/id.rs:76`), never a raw string that
reaches `Path::join`; section 3.8 should say so.

**I7. A protected set denies writes no allow rule re-opens.** `PROTECTED_DIRS`
names the trees the daemon or a login session later executes. The loaders and
the protected set read one list of execution roots.
(`crates/crucible-daemon/src/tools/protected.rs:104`,
`crates/crucible-daemon/src/tools/protected.rs:184`,
`crates/crucible-daemon/src/execution_roots.rs:155`.)
Requires: `Roots` carries a `protected` member beside `allowed`, `denied`
and `carved` (`crates/crucible-daemon/src/tools/containment.rs:179`);
`Protection` is its own reason type; section 4.4 and 8.7 should list the
protected set as a layer that precedes every allow.

**I8. A plugin sees a delete.** The watcher emits three file events. A plugin
that keeps an index must see `FileDeleted` and `FileMoved`, or it serves stale
paths. (`crates/crucible-lua/src/handlers/hook_name.rs:48`,
`crates/crucible-lua/src/handlers/hook_name.rs:50`.)
Requires: `EventName` holds `FileChanged`, `FileDeleted`, `FileMoved`, four
`Note*` and `WebhookReceived`, not the `SessionCreated` and `SessionEnded` of
section 8.3 (G81). Session start and end are lifecycle hooks that may refuse a
session before a turn exists (G40).

**I9. A webhook is authenticated before it broadcasts.** The web route
verifies the HMAC and the timestamp, then calls `webhook.receive`, which only
broadcasts. (`crates/crucible-daemon/src/webhook/mod.rs:256`,
`crates/crucible-daemon/src/rpc/dispatch.rs:1792`.)
Requires: `WebhookReceived` is an `EventName` whose handlers can only
`cancel`; no stage reply exists for it.

Summary of the closed-set members these invariants require:
`ToolSurface::Unknown` (I3); the isolation gate as a fixed position in the
gate order (I3, I4); `Containment::SymlinkEscape` (I1); `WritablePath` apart
from `ContainedPath` (I2); `Roots.protected` (I7); `EventName::FileDeleted`
and `EventName::FileMoved` (I8); `SocketDirRefusal` (I5); `SessionId::parse`
(I6). Sections 8.1, 8.3 and 8.7 stand corrected by this section; the
disagreements D4 and D20 in section 10 are settled the code's way.

## 8. Closed sets

Each set is one enumerated type with an exhaustive match and an `ALL` array. A
test proves the array complete by walking the enum with `EnumIter`, not by a
source grep. Two module-level clippy denies guard each table:
`wildcard_enum_match_arm` and `match_wildcard_for_single_variants`. The decision
type has no `Default`.

### 8.1 `BuiltinTool` with `ToolSurface`

```rust
pub enum BuiltinTool {
    // workspace: internal agent only, never over MCP
    ReadFile, EditFile, WriteFile, Bash, Glob, Grep,
    // kiln: internal agent and MCP
    CreateNote, ReadNote, ReadMetadata, UpdateNote, DeleteNote, ListNotes,
    SemanticSearch, GrepNotes, PropertySearch, GetKilnInfo, SkillView,
    // delegation and jobs: internal agent and MCP
    DelegateSession, ListJobs, GetJobResult, CancelJob,
    // disclosure bridge: never hidden, never deferred, never over MCP
    DiscoverTools, GetToolSchema, InvokeTool,
}
pub enum ToolSurface { Daemon, Mcp, Both }
pub fn surface(t: BuiltinTool) -> ToolSurface { match t { /* exhaustive */ } }
```

Twenty-four built-in tools. Fifteen have surface `Both`. Nine have surface
`Daemon`. [D20]

Plan-mode tools are the read-shaped subset: `ReadFile`, `Glob`, `Grep`,
`ReadNote`, `ReadMetadata`, `ListNotes`, `SemanticSearch`, `GrepNotes`,
`PropertySearch`, `GetKilnInfo`, `SkillView`, `DiscoverTools`, `GetToolSchema`.
Safe tools that skip the prompt are the same subset. `WriteFile` is not safe.

### 8.2 `StageId`: eleven synchronous turn-loop stages

```rust
pub enum StageId {
    SessionStart, TurnStart, PrecognitionSelect, TransformContext,
    PreToolCall, ToolResult, ValidateOutput, TurnComplete, SessionEnd,
    PermissionRequest, Compact,
}
```

Return contracts: `PreToolCall` → `nil | {cancel} | {transform = args} |
{handled, result}`; `ToolResult` → `nil | {result}`; `TurnComplete` → `nil |
{inject = {content}}`; `PrecognitionSelect` → `nil | {notes}`;
`TransformContext` → `nil | {messages}`; `ValidateOutput` → `{ok} | {fail,
reason}`; `PermissionRequest` → `nil | {allow} | {deny} | {prompt}`;
`SessionStart` → `nil | {refuse}`. [D3]

### 8.3 `EventName`: eight daemon broadcast events

```rust
pub enum EventName {
    NoteCreated, NoteModified, NoteDeleted, NoteRenamed,
    FileChanged, WebhookReceived, SessionCreated, SessionEnded,
}
```

Lua spellings: `note:created`, `note:modified`, `note:deleted`,
`note:renamed`, `FileChanged`, `webhook:received`, `session_created`,
`session_ended`. One table maps the Rust name to the Lua spelling. The outbound
bridge reads the same table. Only `cancel` means anything in a handler's return
value. [D4]

### 8.4 `ScriptingEvent`: the ten names scripting and transport share

`text_delta`, `thinking_delta`, `tool_call`, `tool_result`, `message_complete`,
`segment_complete`, `precognition_complete`, `interaction_requested`,
`mode_changed`, `title_changed`. [D5]

### 8.5 `RpcMethod`: generated from one `rpc_methods!` table

One table generates `RpcMethod`, `METHODS` and the dispatcher's `parse`. Tests:
no duplicates, every variant listed, every advertised name parses. The spellings
below follow the product docs where the docs name a method. [D10]

- `daemon.capabilities`, `daemon.status`, `daemon.shutdown`
- `session.create`, `session.list`, `session.get`, `session.send`, `session.cancel`, `session.pause`, `session.resume`, `session.end`, `session.archive`, `session.unarchive`, `session.delete`, `session.export`, `session.render_markdown`, `session.search`, `session.history`, `session.fork`, `session.undo`, `session.can_undo`, `session.undo_history`
- `session.connect_kiln`, `session.disconnect_kiln`, `session.set_workspace`, `session.set_title`, `session.generate_title`
- `session.list_modes`, `session.set_mode`, `session.get_mode`, `session.set_model`, `session.get_model`, `session.list_models`, `session.switch_model`
- `session.set_*` and `session.get_*` for every `SessionConfig` field and for `temperature`, `max_tokens` and `system_prompt`
- `session.request_compaction`, `session.compact`, `session.cache_stats`
- `session.pending_interactions`, `session.respond_interaction`
- `session.add_notification`, `session.list_notifications`, `session.dismiss_notification`
- `session.subscribe`, `session.unsubscribe`, `session.configure`
- `subagent.collect`
- `kiln.open`, `kiln.close`, `kiln.list`, `kiln.info`, `kiln.graph`, `kiln.stats`
- `project.register`, `project.list`, `project.get`, `project.unregister`
- `note.list`, `note.get`, `note.create`, `note.update`, `note.delete`, `note.rename`, `note.move`, `note.resolve`, `get_backlinks`, `suggest_links`
- `process_file`, `process_batch`, `search_vectors`, `search_semantic`, `search_text`, `search_grep`, `property_search`
- `fs.list_dir`, `fs.move`, `fs.mkdir`, `fs.trash`, `fs.read`, `fs.write`
- `canvas.get`, `canvas.put`
- `review.list_hunks`, `review.set_state`, `review.comment`, `review.resolve_comment`
- `scm.clone`
- `providers.list`, `models.list`, `auth.store_key`
- `skills.list`, `skills.search`, `skills.get`
- `plugin.list`, `plugin.reload`, `plugin.install`, `plugin.remove`, `plugin.commands`, `plugin.run_command`, `plugin.options`, `plugin.set_option`, `plugin.publications`, `plugin.test`, `plugin.status`
- `lua.eval`, `lua.init_session`
- `ui.config`, `ui.set_theme`
- `mcp.status`
- `acp.discover`
- `agents.list`
- `storage.verify`, `storage.cleanup`, `storage.backup`, `storage.restore`, `storage.status`
- `workflow.list`, `workflow.show`, `workflow.start`, `workflow.approve`, `workflow.status`, `workflow.cancel`
- `config.show`, `config.get`, `config.set`
- `webhook.deliver`

### 8.6 Permission modes: `ModeId` with shipped declarations

Not a Rust enum. `ModeId` is a string validated against `ModeRegistry`. Shipped
declarations: `ask` (stance Ask, all tools), `plan` (stance Deny for a
mutating tool, a read-shaped tool selector, plus an unconditional Rust deny for a
mutating tool that reaches the gate), `auto` (stance Allow). A Lua config may
remove or add any. An unknown mode fails closed. `ModeRegistry` has no Rust
default.

### 8.7 Permission layers, in decision order

```rust
pub enum Layer { SafeGate, CliOverride, GlobalConfig, ProjectPattern, LuaHook, ModeRule, ModeStance, NonInteractive, Prompt }
```

`ModeStance::{Allow, Deny, Ask}`. `PermDecision::{AllowOnce, AllowSession,
AllowProject, Deny}`.

### 8.8 Interaction kinds

Seven: `Permission`, `Ask`, `AskBatch`, `Edit`, `Show`, `Popup`, `Panel`. Every
client has one arm per kind. A test fails when a kind has no arm. A response
carries its own `kind` tag.

### 8.9 Tool sources

`Builtin`, `Plugin(name)`, `McpUpstream(name)`, `Acp(profile)`. `Builtin` shows
no badge. The others show a badge. [D9]

### 8.10 Session states

`Active`, `Paused`, `Streaming`, `Ended`. `archived` is a flag. There is no
`Compacting`.

### 8.11 Providers

Nine kinds (section 3.18). A new one is a variant plus one match arm in the
handle builder. [D8]

### 8.12 TUI REPL commands and slash commands

REPL: `:quit` `:q`, `:help`, `:clear`, `:model`, `:set`, `:export`, `:messages`
`:msgs` `:notifications`, `:mcp`, `:config`, `:palette` `:commands`, `:lua`
`:=`, `:pick`, `:plugins`, `:reload`, `:undo`.

Slash built-ins: `/mode`, `/default`, `/undo`, `/help`, plus one per declared
mode, plus plugin commands. Anything else forwards to the agent.

### 8.13 Autocomplete triggers

Nine: `@` files, `[[` notes, `/` commands, `:` REPL, `:model `, `:set ` names,
`:set ` values, F1 palette, `:pick`.

### 8.14 Notification kinds

`Toast`, `Progress`, `Warning`. There is no `Error` level.

### 8.15 Plugin sources

`EnvPath`, `User`, `RuntimePath(PathBuf)`, `Runtime`. Search order:
`CRUCIBLE_PLUGIN_PATH`, `~/.config/crucible/plugins/`, each `runtimepath`
entry's `plugins/`, then `$CRUCIBLE_RUNTIME/plugins/` or the exe-relative tree.
No kiln or project directory is on the list. A kiln's plugins load when the kiln
is on `runtimepath`. [D7]

### 8.16 Bundled runtime plugins

`auto-title`, `daily-notes`, `discord`, `graph-view`, `oci`, `reflection`,
`review`, `todo-list`, `web-search`, `worktree`.

### 8.17 Skill scopes

`Personal`, `Workspace`, `Kiln`, `Bundled`. A higher scope shadows a lower one.
[D6]

### 8.18 Context strategies, output validations

`ContextStrategy::{Truncate, SlidingWindow, Summarize, Lua{name}}`.
`OutputValidation::{None, Lua{name}}`.

### 8.19 Built-in ACP profiles

`opencode`, `claude`, `gemini`, `codex`, `cursor`.

## 9. Extension points

Each point names where a new thing plugs in and what it must implement.

### 9.1 A new built-in tool

Add a `BuiltinTool` variant. The compiler then demands a `surface()` arm, a
`ToolDefinition` (name, description, JSON schema), a `dispatch` arm, a safety
class, a plan-mode class, and a containment call for every path argument. Reach
every path through `CapabilityHandle`. Never open a path directly. Declare
`read_only_hint` and `deferrable`. The `ALL` test and the surface test derive
their expectation from the enum. There is no list to edit by hand.

### 9.2 A plugin tool

Add a spec-table entry `tools.<name> = { desc, params, fn }`. The host registers
it with `ToolSource::Plugin(name)`, advertises it to the internal agent and over
`cru mcp`, and routes calls through the same gate. A plugin tool is deferrable
under progressive disclosure. Kiln and workspace tools are not.

### 9.3 A hook

Call `cru.on(name, { pattern, priority }, handler)`. `name` must be a
`StageId` spelling or an `EventName` spelling. Any other name is an error at
registration. The handler receives `(ctx, event)` and returns the contract for
that stage. The host removes a plugin's registrations at unload. The stub
generator picks a new name up by walking the VM.

### 9.4 A permission hook

Call `cru.permissions.on_request(handler)` with a 1 s budget. Return `{allow}`,
`{deny}`, `{prompt}` or `nil`. The hook runs after project patterns and before
mode rules, so a user hook beats the shipped `auto` stance.

### 9.5 An execution backend

Write a `pre_tool_call` handler that returns `{handled = true, result}`. This is
capability-grade: it returns before the permission gate. The design allows it
only when the session carries an isolation claim the plugin published through
`set_status`. The dispatch layer refuses an isolation claim for an ACP session.
A tool that no handler takes over in an isolated session is denied by name. If
isolation cannot be established, return `{cancel = true}`. The daemon never
gains a per-backend abstraction. The `oci` plugin is the reference. [D21]

### 9.6 A mode

Declare `cru.modes.<id> = { tools = selector, permissions = { default, allow,
deny, ask }, label, color }` in `init.lua`. The TUI derives the badge, the
BackTab cycle and the `/<id>` command from `session.list_modes`.

### 9.7 A context strategy or validator

Call `cru.context.register_strategy(name, fn)` or
`cru.context.register_validator(name, fn)`. `SessionConfig` enables one per
session. A strategy may not trigger a turn on its own session. An unregistered
name degrades to a failure, not a panic.

### 9.8 A chat provider or an embedding provider

Add a `ProviderKind` variant and one arm in the handle builder that returns
`impl ChatBackend`. Add credential resolution from `cru auth login` storage or
config. Map the provider's error shape onto `ChatError`. A provider error must
classify `retryable` and `retry_after`. Nothing in the turn loop changes.

For embeddings, implement `EmbeddingBackend` and add an
`EmbeddingProviderConfig` variant. The pipeline and precognition see the trait
only.

### 9.9 An ACP profile

Add `[acp.agents.<name>]` with `extends`, `command`, `env`, `trust` and
delegation limits. Discovery probes the command against the resolved
environment and records an absolute path.

### 9.10 An MCP upstream

Add `[mcp.upstreams.<name>]` with a transport, a command or URL, `allow` and
`block` filters, and `auto_reconnect`. Tools appear as `<name>_<tool>`. The
gateway registers them as `deferrable`.

### 9.11 A client

A client implements: subscribe to `SessionEventMessage`; render every
`SessionEvent` variant with an exhaustive match; render every `InteractionKind`
with an exhaustive match; send `session.respond_interaction`; call the session
methods. It holds no business state. A multi-client knob travels the full chain:
client command → RPC setter → `SessionConfig` → `AgentHandle::configure` → event
back. A test sets a knob on one client and reads it on another after resume.

### 9.12 A storage backend

The product has one backend. The seam, if a second arrives, is the set of
repositories the pipeline and retrieval call: `NoteRepository`,
`LinkRepository`, `EmbeddingRepository`, `SessionLogRepository`. Until a second
implementation exists in a different crate, an enum over the variants beats a
trait.

### 9.13 A plugin-declared command

Add `spec.commands.<name> = { desc, args, fn }`. `plugin.commands` lists it.
`plugin.run_command` runs it. It is reachable as `/<name>` in the TUI and in the
web palette. A plugin cannot shadow a built-in.

### 9.14 A scheduled automation

Call `cru.schedule({every = secs}, fn)` or add a `[[schedules]]` block. The cap
is 256 active schedules. A durable cron job with history is a planned separate
entity.

### 9.15 A web panel

Register a TypeScript component in the one panel registry. The panel reads
daemon state over HTTP and SSE. The contract shared with Lua is data (events and
RPC), not widgets. Lua does not reach the browser.

### 9.16 A theme

Call `cru.colorscheme.setup{}`, `cru.hl.set|link`,
`cru.geometry.setup{}` and `sl.setup{}` in `init.lua` or `themes/*.lua`. The
plugin host projects them to `UiConfig` and pushes `UiStyleChanged`.

### 9.17 A new RPC method

Add one row to the `rpc_methods!` table. `RpcMethod`, `METHODS`,
`daemon.capabilities` and the dispatcher derive from it. If a browser needs the
method, add one route that uses the same field names. The route-existence test
fails otherwise.

### 9.18 A new session knob

Add the field to `SessionConfig`. Add a getter and a setter pair to
`rpc_methods!` with one field name. Add the `:set` key in the TUI and the web
route. Assert the value on the outgoing `ChatOptions`, not on a getter.

### 9.19 A new hook stage or event

Add a `StageId` or `EventName` variant. Add the dispatch site in the turn loop
(stage) or in the emitter (event). Add the Lua spelling to the one name table.
`cru.on` validates the name against the enum, not against a list.

## 10. Disagreements

Each row records one point where the two drafts differ. The body of this
document uses the pick.

| # | Topic | Draft 1 | Draft 2 | Pick | Reason |
|---|-------|---------|---------|------|--------|
| D1 | Shape of `SessionAgent` | One flat struct with `kind`, provider, model and every knob | An enum `Internal \| Acp` plus a separate `SessionConfig` | Draft 2 | An ACP agent has no provider and no temperature; the enum removes those `None` paths. |
| D2 | `AgentHandle` contract | Three required methods; `configure` takes the whole record | A required, non-defaulted handle method per knob, built by `AgentFactory` | Draft 1 | One required method carries every knob; a per-knob method can be added without being implemented. |
| D3 | `StageId` members | `pre_turn`, `post_tool_call`, `output_validate`, `context_compact` | `turn_start`, `permission_request`, `validate_output`, `compact` | Draft 2 | Product.md names `validate_output`; the permission hook is synchronous and its return value is read, so it is a stage. |
| D4 | `EventName` members | `session_created`, `session_ended` as the last two | `session:titled`, `plugin:reloaded` as the last two | Draft 1 | Product.md names `on_session_start` and `on_session_end` hooks that fire into the plugin VM; the other two names have no source. |
| D5 | `ScriptingEvent` members | Includes `segment_complete` and `mode_changed` | Includes `turn_started` and `ended` | Draft 1 | `segment_complete` and `mode_changed` are named in the docs; `turn_started` is not. |
| D6 | `SkillScope` members | `Personal, Workspace, Kiln, Bundled` | `Personal, Workspace, Kiln, Runtime, CrossHarness` | Draft 1 | Product.md names personal, workspace and kiln paths plus bundled help skills; no cross-harness scope is named. |
| D7 | `PluginSource` members | `Path, User, Runtime` (three) | `EnvPath, User, RuntimePath, Runtime` (four) | Draft 2 | Product.md lists four search-path entries; `runtimepath` trees need their own provenance. |
| D8 | `ProviderKind` members | Eight chat kinds; embeddings listed apart | Nine kinds, with `FastEmbed` | Draft 2 | Product.md says nine backends exist and names FastEmbed among them. |
| D9 | `ToolSource` members | `Builtin, Plugin, McpUpstream, Acp` | `Core, Workspace, Kiln, Plugin, Mcp, Acp` | Draft 1 | The badge rule treats the three unbadged variants alike; the split adds no information a client reads. |
| D10 | RPC method spellings | Flat names such as `search_vectors`, `process_file`, `get_backlinks`, `session.respond_interaction` | Dotted families such as `search.vectors`, `process.file`, `interaction.respond`, `jobs.list` | Draft 1 | Product.md attests the flat spellings; the dotted families have no source. |
| D11 | Permission structure | One `PermissionGate` trait inside the tools subsystem, which also prompts | A `PermissionEngine` that never prompts, plus a `Layer` enum; `ToolDispatch` prompts | Draft 2 | The engine returns the deciding layer, which the docs ask for in prompts and logs; the prompt is a dispatch concern. |
| D12 | `Popup` payload | `{ title, items }` | `{ title, body }` | Draft 1 | The TUI story renders a popup as a list with a selection. |
| D13 | Permission response | `{ allowed: bool, scope: Once \| Session \| Project }` | `decision: AllowOnce \| AllowSession \| AllowProject \| Deny` | Draft 2 | One enum cannot express `allowed = false` with a scope. |
| D14 | Notification event | `Notification(Notification)` is a `SessionEvent` variant | No notification event; only the RPC | Draft 1 | The TUI messages drawer updates live, which needs a push. |
| D15 | Subsystem granularity | Fourteen subsystems | Twenty-seven subsystems | Draft 2 decomposition under draft 1 grouping | Tools, permissions, interactions and delegation have distinct owners and distinct must-never-know lists. |
| D16 | Workspace entity | `workspace: PathBuf` on the session | A `Workspace { path, kind }` entity with `Scratch(SessionId)` | Draft 2 | The scratch workspace has a lifecycle tied to the session; a bare path cannot carry it. |
| D17 | Link target | `resolved: Option<RelPath>` plus a `dangling` flag | `LinkTarget::{Resolved, Ambiguous, Dangling}` | Draft 2 | Rename skips an ambiguous stem, so the index must represent ambiguity. |
| D18 | Embedding placement | `embedding: Option<Vec<f32>>` on the note record | A separate `Embedding` entity with a `block` field | Draft 2 | Block-level retrieval is a documented goal; the separate entity ships it without a schema change. |
| D19 | Crate dependency between `lua` and `oil` | `crucible-lua` uses `crucible-oil` for theme types | A chain `core ← lua ← oil ← daemon`, and also "oil does not depend on lua" | Neither; both depend on `core` only | Draft 2 contradicts itself; theme wire types belong in `core` so neither crate needs the other. |
| D20 | MCP tool count | "23 built-in; MCP serves 15 plus `skill_view`" | 24 built-in; MCP serves 15 including `skill_view` | Draft 2 | Draft 1 lists 24 variants and its own table gives `skill_view` surface `Both`; 15 is the count of `Both`. |
| D21 | `handled` invariant | Gate ordering is the only protection | An isolation claim through `set_status`; refused for an ACP session | Draft 2 | A stronger invariant with a named check; draft 1 records the same gap as an open question. |
| D22 | Notification owner | The interaction and events subsystem | `SessionManager` stores; clients render | Draft 2 | The RPC family is `session.*_notification`, so the session manager owns the store. |
| D23 | Project registry placement | Inside the kiln registry subsystem | A separate `ProjectRegistry` | Draft 2 | Project and kiln are never interchangeable; a shared owner invites a shared path. |
| D24 | Compaction owner | A `context_compact` stage in the session manager | An operation on `ConversationTree` plus a `Compact` stage in the turn loop | Both, in the turn loop | The tree is the thing compacted; the stage lets Lua replace the operation. |
| D25 | Turn stage order detail | `pre_turn → precognition → precognition_select → transform_context` | `turn_start → precognition → attachments → rules → skills → transform_context` | Union | The two lists do not conflict; draft 2 names the steps between. |

## 11. Open questions

The union of both drafts. Each item is a point where the documents do not
determine the design.

1. **Precognition frequency.** README says "before each LLM turn". Product.md says "first user message of a session only". This design follows Product.md. A per-turn mode needs a deduplication rule against notes already in context.
2. **Block-level retrieval.** The docs promise paragraph granularity and admit one vector per note. The schema keeps a `block` column. The docs do not say whether search returns blocks or notes, or how a block hit maps to a precognition excerpt.
3. **Compaction.** Auto-compaction sets a state nobody consumes. The docs do not say who performs compaction. This design puts it on the conversation tree as a `Compact` stage. The `Summarize` strategy's LLM recap is unspecified.
4. **ACP session persistence.** ACP agents own their history. On resume, the docs do not say whether Crucible replays its transcript, calls `loadSession`, or starts cold. The design stores `remote_session_id` and leaves the policy per profile.
5. **ACP filesystem capability.** Deliberately unwired. If read-only capture is wanted, `readTextFile: true` is the documented direction, and the content capture path needs a home.
6. **`handled` ordering.** The `oci` plugin takes over execution before the gate. The isolation claim in section 9.5 is stronger than the docs require. A capability token on the session is possible but unspecified.
7. **Project config `kilns` table.** Parsed and ignored. Multi-kiln association uses a global registry plus `session.connect_kiln`. The docs do not say which wins, or whether `project.toml` seeds the kiln set at creation.
8. **Kiln-local config.** `cru init` writes `.crucible/init.lua`, which loads into that kiln's session runtimes. The `.crucible/config.toml` older versions wrote never loaded at all. Whether per-kiln app config is a layer is undecided.
9. **Webhook auth posture.** The route sits inside bearer auth, so a remote sender gets 401 before its HMAC is read. Move it outside, or add an opt-in key. Not decided.
10. **Agent-initiated questions.** Seven interaction kinds render in both clients. No agent tool produces an `Ask`. Should `ask` be a `BuiltinTool` with surface `Daemon`, or a Lua-only primitive through `cru.ui`? Its plan-mode class is unspecified.
11. **`cru.session.fork`.** The Lua path copies history with no agent config. Whether fork copies the agent, the mode and the kiln set is unspecified.
12. **`cru.session.inject`.** Writes the log only. Whether inject also appends to the conversation tree for the next turn is unspecified. `cru.context.attach` covers this turn.
13. **Two Lua objects named `session`.** The hook parameter reaches `SessionAgent`. The `cru.get_session()` object on the plugin VM does not. One of them should go, or the plugin VM should reach session state only through `cru.session.*`. — *Resolved 2026-08: one canonical `cru.session` module (lifecycle verbs, `current()`, handle-returning `create`/`get`/`list`/`fork`); `cru.get_session()` and the plural `cru.sessions` are deprecated aliases into it. The hook parameter remains a distinct argument-passed `Session` — same type, different delivery — with its gap to `SessionAgent` unchanged.*
14. **Session VM versus plugin VM.** RESOLVED: the session default tier is the config store, which both VMs read, so `cru lua` sees it.
15. **`cru.oil` versus `cru.ui`.** `cru.oil` builds nodes that no client consumes. `cru.ui` opens real modals. Withdraw `cru.oil` or wire it.
16. **Review gate for ACP agents.** The daemon cannot hold an external agent at a pre-write gate. The effective policy for an ACP session is "review at turn end". Whether to block the turn's result is open.
17. **Kiln attach from the web composer.** `session.connect_kiln` exists, yet the web story says the daemon has no RPC to change a live session's kiln set. One of the two docs is stale. The design keeps the RPC.
18. **Workspace for web-created sessions.** A TUI session acts in the kiln. A web-created session acts in the registered project root. The docs call this a documented asymmetry. One rule (`workspace` is explicit, else the scratch dir) would remove it.
19. **Prompt caching.** Cache breakpoints on the outgoing request are unobserved. The docs do not say which providers get them, or how the second-to-last turn is chosen when tool rounds split a turn.
20. **Progressive disclosure tiers.** One 15% threshold. Intermediate tiers are planned and unspecified.
21. **Graph traversal over RPC.** `kiln.graph` returns a flat edge list. `cru.kiln.neighbors` walks it in Lua only. Whether an n-hop RPC should exist is open.
22. **Reranking.** Removed. Whether retrieval should rerank, and where, is open.
23. **Session semantic indexing.** No pipeline exists. Whether transcripts are embedded, and where the vectors live given that transcripts are outside kilns, is open.
24. **Durable scheduled jobs and the global estop.** Planned. Whether a job run is a session, where the estop sentinel lives, and which admission points check it (session admission, delegation spawn, the scheduled-job tick) are specified only in outline.
25. **Verification evidence ledger.** A second record type in `review.jsonl`. The rules that classify a bash call as evidence are named but not defined.
26. **Plugin shadow-by-name.** The priority table promises it. No test proves it. The tie-break when two `runtimepath` trees hold the same name is unspecified.
27. **Typed plugins.** `cru plugin check` proves a plugin parses and its declarations are readable; the type check itself runs only where `luau-analyze` is installed, and how much of `cru.*` carries a real signature is still growing.
28. **Plugin manifest permissions and sandboxing.** The plugin stories ask for declared capabilities (`read`, `write`, `network`). The product has `cru.tools.call` under operator rules and an `oci` sandbox for tools, but no per-plugin capability model for the Lua VM itself.
29. **Dry run and undo for plugins.** The plugin stories want a preview of what a plugin would change. The product has turn undo via `WorkspaceSnapshot`. Whether a plugin run is a turn, and therefore undoable, is unspecified.
30. **Template and registry packaging.** A single file with frontmatter, or a folder with a manifest. A self-hosted or GitHub-based registry. Both are open in the plugin stories.
31. **Workflow fan and ralph steps.** `[type:: fan]` and `[type:: ralph]` are reserved with outlined semantics. Dynamic cardinality and the loop predicate are not specified.
32. **Mobile shell and offline cache.** A separate shell on one origin. Caching authenticated responses needs a policy for what, how long, and on sign-out.
33. **Model picker context limit source.** `ContextLimitSource::Agent` for ACP. For internal providers the limit per model is not always known. The fallback is unspecified.
34. **TUI viewport after undo.** Files and the tree rewind. The viewport does not. Whether the TUI truncates on `UndoComplete` is a UX decision the docs record but do not make.
35. **Snapshot durability for undo.** The snapshot map is in memory. Undo after a daemon restart rewinds the chat and leaves files alone. Whether snapshots persist under the data root is open.
36. **Daemon `PATH` for ACP discovery.** Probe in the client's environment, or resolve the login-shell `PATH` once. Either way, store absolute paths and key the cache on them.
37. **Stream lag policy for SSE.** The web SSE is a bounded channel with no stated drop policy. The daemon's `StreamGap` marker should be forwarded, not swallowed.
38. **MCP gateway at daemon bind.** Both bind sites pass no MCP config, so upstream tools reach no agent. The design assumes the gateway is built once at bind from `[mcp]`.
39. **Pipeline tuning knobs.** Eight of nine `[enrichment.pipeline]` fields are inert. Delete them or implement them. The design keeps only `max_precognition_chars`.
40. **Per-session tool parallelism.** The provider can emit parallel tool calls. The loop dispatches them one at a time. Concurrent dispatch changes permission-prompt ordering.
