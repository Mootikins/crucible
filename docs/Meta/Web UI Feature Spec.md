---
title: Web UI Feature Spec
description: Long-run feature specification and design brief for the Crucible web UI — the complete feature map, competitive completeness check, and high-value differentiators
type: spec
status: active
created: 2026-07-11
updated: 2026-07-11
tags:
  - meta
  - web
  - spec
  - design
  - product
---

# Web UI Feature Spec (Long Run)

> **Audience:** a design/rebuild effort for the Crucible web UI. This document is the single synthesis of what the web UI must be *in the long run* — the design principles, the UI system, the complete feature catalog with current status, a completeness check against every comparable agent UI, and the differentiators worth building that nobody else can.
>
> **Relationship to other docs:** [[Meta/Product]] is the whole-product inventory; [[Meta/Web User Stories]] is the test-coverage contract for what's shipped; [[Meta/FlexLayout-Spec]] is the behavioral spec for the windowing system. This document is the *forward-looking* web feature map that ties them together. Status markers here were audited against the codebase on 2026-07-11.

**Legend**: `[x]` shipped · `[-]` partial · `[ ]` planned · `[s]` stub (placeholder in UI) — horizon tags: `NOW` (finish/fix), `NEXT` (the coming arc), `LATER` (mature-product)

---

## 1. Vision

The web UI is a **stateless console onto the daemon** — the same relationship the TUI has. The daemon is the hypervisor: sessions, knowledge, permissions, agents, and plugins all live there; the browser renders and routes input. A session started in `cru chat` is resumable in the browser mid-turn, and vice versa (the shipped WS-HERO story proves this).

What the web UI adds over the TUI is **space and richness**: multiple simultaneous panels, visual knowledge exploration, rendered artifacts, diff review, drag-and-drop composition. What it must never add is **state** — if a web frontend would need to duplicate daemon logic, the logic is in the wrong place.

The product identity is **"Obsidian, if the vault could act."** Not a chat app with a file tree bolted on; a knowledge workspace where agents are first-class inhabitants. Three loops must all feel native:

1. **Converse** — chat with agents that know your kiln (Precognition), with full supervision (permissions, plans, undo).
2. **Curate** — read, edit, link, and organize the knowledge the conversations produce and consume.
3. **Supervise** — run many agents, approve their actions, review their diffs and proposals, watch them learn.

### Design principles

1. **Gateway-centric, stateless views.** All state via HTTP→RPC + SSE. Every feature must answer: "does the TUI see the same state?" If not, it's broken.
2. **Obsidian-like workspace.** Dockable panes, tabs, splits, a command palette, wikilinks that work everywhere, plaintext underneath. Users arrange their own workspace; layouts persist.
3. **Neovim-like extensibility.** Core provides primitives (panels, renderers, commands, events); plugins compose them. A Lua plugin should be able to contribute a web panel, a content renderer, a palette command, or a tool card — without a frontend rebuild.
4. **The knowledge graph is the differentiator.** No competitor renders a live, agent-fed knowledge graph in the browser. Every feature that makes knowledge *visible* (graph view, backlinks, precognition transparency, proposals) widens the moat.
5. **Supervision over automation theater.** Permissions show full arguments. Diffs are reviewable. Undo is reachable. Agent memory is inspectable and editable. Nothing the agent does is invisible or irreversible.
6. **Keyboard-first, mouse-friendly.** Palette (Ctrl+P) is the universal entry point; every palette action also has a discoverable UI affordance. Browser-reserved chords (Ctrl+T, Ctrl+Shift+N) can't be load-bearing — they only work in PWA/desktop.
7. **Self-hosted remote is the deployment story.** Localhost keyless; remote always keyed (`cru web key`, connect URLs); `cru tunnel` for zero-config exposure. PWA for mobile without app stores.

---

## 2. The UI system (shell)

The shell is a bespoke SolidJS window manager (VS Code / FlexLayout hybrid). Its behavioral contract lives in [[Meta/FlexLayout-Spec]] — tabs, tabsets, 4-edge dockable borders with expand/collapse/hide cycles, splits, drag-and-drop with drop previews, floating windows, JSON layout round-trip. This section states what the *product* needs from it long-run.

### 2.1 Workspace primitives

- [x] Recursive splits, tab groups, tab DnD (reorder + cross-pane), edge panels (left/right/bottom) with flyout mode, floating windows, minimized tray, status bar
- [x] Layout persistence — debounced auto-save to `/api/layout`, restored at startup; tab icons survive reload
- [x] Panel registry — panels declare id/title/icon/default-zone; palette `openPanelTab()` opens-or-focuses
- [ ] `NOW` **Popout windows** — any tab to a separate browser window (multi-monitor supervision; FlexLayout Tier-1)
- [ ] `NEXT` **Named layout presets** — save/switch arrangements ("writing", "supervising", "reviewing"); per-project layouts
- [ ] `NEXT` **Pinned + preview tabs** — pin protects from bulk close; single-click opens an italic ephemeral tab that reuses its slot (VS Code pattern; essential once the file tree is a primary surface)
- [ ] `NEXT` **Maximize / zen mode** — one tabset fills the layout; distraction-free writing mode
- [ ] `LATER` Activity bar (icon rail), dual sidebars, auto-hide slide-in panels — see FlexLayout-Spec Tier 3

### 2.2 Command palette & keyboard

- [x] Ctrl+P palette (cmdk) with Chat/Session/Navigation/Settings categories; shortcut table (splits, panel toggles, Shift+Tab mode cycle, Esc)
- [-] `NOW` **Unified omnibox** — SHIPPED 2026-07-11 (Crucible Shell): GO surfaces + note quick-switcher + sessions + commands with `>`/`[[` prefixes (WS-304). Remaining: `@` files, `#` tags, `:set`-style knobs. Fuzzy matching shipped 2026-07-12 (scored subsequence in `lib/fuzzy.ts`: substring > word-start > position, label hits outrank keyword hits).
- [ ] `NEXT` **User-definable keybindings** — JSON/Lua keymap with chord support; plugin-contributed palette commands
- [ ] `NEXT` **Slash-command parity with TUI** — the composer's `/` commands and the TUI registry should be one list served by the daemon, not two hardcoded sets

### 2.3 Theming & design system

- [-] Tailwind v4 + ~100 `--fl-*` chrome tokens; **dark theme only is wired** (a complete `light.css` exists but is never imported; no toggle, no `prefers-color-scheme`)
- [ ] `NOW` **Light theme + system-follow + toggle** — wire the existing tokens; theme choice persists
- [ ] `NEXT` **Theme tokens as a public contract** — document the token set so themes are data (CSS custom properties), swappable at runtime; ship 2–3 built-ins (dark, light, high-contrast)
- [ ] `LATER` **Community themes + CSS snippets** — Obsidian's model: a theme is a file in the kiln/config dir; plugins can register additional tokens
- [-] `NOW` **Design-token pass for chat surface** — PARTIAL 2026-07-11: shell identity tokens landed (ember `--color-primary`, near-black surfaces, shell semantic colors, IBM Plex via @fontsource) and all hardcoded blue utilities swept onto tokens; remaining: migrate leftover `zinc/neutral` utility classes and specify the typography/spacing scale

### 2.4 Responsive & mobile (PWA)

- [x] PWA manifest + service worker (app-shell precache; `/api/*` never intercepted). Known issue: SW serves stale bundles across rebuilds — needs a versioned-update toast ("new version available — reload")
- [ ] `NEXT` **Mobile layout mode** — the window manager is desktop-only today. Below a breakpoint, collapse to a single-column stack: chat full-screen, panels as bottom sheets/drawers, palette as the navigation spine. Mobile is *supervision-first*: read sessions, approve permissions, answer Asks, quick-capture notes — not multi-pane editing.
- [ ] `NEXT` **Web push notifications** — permission requests, Asks, and turn completion push to the phone (this is what makes remote/background agents actually usable; see §4.3)
- [ ] `LATER` Quick-capture share target (PWA share_target: share a URL/text into the kiln as a note)

---

## 3. Feature catalog

### 3.1 Chat & conversation

The core loop. Most of it is shipped and story-tested (WS-101…109).

- [x] Token streaming over SSE with reconnect; cancel with partial-output retention
- [x] Thinking blocks (collapsible, toggle wired); tool cards with status, formatted args, streamed results, error auto-expand, terminated badge
- [x] Diff rendering in tool cards (Shiki, multi-edit)
- [x] Permission modal — action-type badge, **full tool arguments**, write-diff preview, allow/deny with once/session/project/user scope; queued requests open sequentially
- [x] Ask/Popup interactions (single/multi-select, free-text) — GAP: no cancel affordance (WS-105)
- [x] Model picker mid-conversation; mode switching (Normal/Plan/Auto) persisted via `session.set_mode` — note: plan mode enforcement is daemon-side tool filtering; verify the web can't bypass
- [x] Markdown rendering, code-block copy buttons, message editing, per-message token usage (incl. cache), context-usage meter, precognition badge on enriched messages
- [x] Voice input (Whisper — local WebGPU or server)
- [x] Export dialog (markdown); auto-title on first message
- [x] Composer autocomplete: `/` commands, `@` files, `#` tags, `[[` wikilinks
- [x] `[[note]]` links in messages navigate to the editor (shipped); **hover previews** SHIPPED 2026-07-15: one document-level `WikilinkHoverPreview` reacts to any `data-note` element (chat anchors, editor decorations, backlinks rows) with title/path/rendered-excerpt card, click-to-open, per-kiln caching (WS-208). Aliased `[[note|alias]]` links now display the alias and resolve the target.
- [ ] `NOW` **Message queueing while streaming** — TUI has it; web composer should queue-and-send identically (queued messages steer the run — Claude Code/Codex "real-time steering")
- [ ] `NOW` **Regenerate / retry a turn** — table stakes in every chat UI; daemon `undo_turns` + resend covers it. Include regenerate-with-a-different-model (pick a model in the retry affordance).
- [ ] `NOW` **Turn undo UI** — the daemon has `/undo` with real file rollback (`WorkspaceSnapshot`). Surface it: an undo affordance on the last agent turn showing *what will be reverted* (messages + files). No competitor ties chat undo to actual workspace rollback this cleanly.
- [ ] `NEXT` **Cost gauge** — extend the context meter with running per-session cost in currency (LibreChat's context/cost gauge is the reference); cache stats RPC already carries the inputs
- [ ] `NEXT` **Deny-with-feedback** — the permission modal's Deny takes an optional reason returned to the agent as context, not a bare block (`pre_tool_call` already supports `{cancel, reason}`); turns rejection into steering
- [ ] `NEXT` **Conversation branching** — daemon `ConversationTree` + `session.fork` already exist. UI: "branch from here" on any message; a branch indicator/switcher on the session. (ChatGPT/LibreChat-style edit-and-branch, but backed by a real tree.) Long-run: render the tree as a pannable node graph with prune/flatten (Msty Flowchat is the reference) — a conversation graph is on-brand for a knowledge-graph product.
- [ ] `NEXT` **Side chat** — branch a quick question off a running session without polluting the main thread's context (Claude Code ⌘; pattern); trivially a scoped fork that never merges back
- [ ] `NEXT` **Attachments & images** — file/image upload into the turn context (multimodal providers exist); drag-drop onto the composer; paste screenshots. Store attachments in the kiln so they're linkable.
- [ ] `NEXT` **Plan-approval flow** — when the agent is in Plan mode, render the proposed plan as a structured card with approve/revise (Claude Code's plan mode UX); on approval, flip mode and continue
- [ ] `NEXT` **Agent task/progress display** — render agent-emitted task lists / plan progress as a live checklist card (Claude Code TodoWrite-style); workflow step progress uses the same component
- [ ] `NEXT` **Prompt library** — reusable prompt templates with variables, stored as kiln notes (a `prompts/` folder + frontmatter), insertable from the palette/composer
- [ ] `LATER` **TTS output** (speak responses) — pairs with existing voice input
- [ ] `LATER` **Side-by-side model comparison** — fork a session to two models, render responses in adjacent panes (the windowing system already supports the layout; this is orchestration + a compare affordance)

### 3.2 Agent supervision (the command center)

The biggest structural gap vs. competitors. Codex/Cursor/Claude Code all converged on "a queue of agents you supervise"; Crucible's daemon is *already* multi-session — the web just doesn't show it.

- [x] Sessions panel — list/filter/search, create/switch/pause/resume/archive/delete
- [x] Activity panel — subagent + delegation cards with elapsed time
- [x] Status bar — mode/context/model for focused session
- [-] `NOW` **Agent Inbox as the landing page** — SHIPPED v1 2026-07-11 (Crucible Shell): Home landing surface + Inbox panel with in-place approvals and all-sessions status, header attention badge (WS-301/302/303). Remaining: daemon-side aggregation (today only open-tab sessions raise attention — no wildcard subscription), unread markers, ACP/workflow heterogeneity. Original scope: a dashboard composing `session.list` + wildcard event subscription: every running session with live status (streaming/**waiting-on-you**/idle/ended), **pending interactions surfaced at top** (approve from the inbox without switching tabs), recent completions, unread-since-last-look markers. Crucially it is *heterogeneous*: internal sessions, ACP-hosted external agents (Claude Code, OpenCode, Gemini), and workflow runs share one inbox and one state machine (Linear's open agent-inbox is the reference — Crucible's ACP host is already the plumbing). This is Wave 5's flagship item and the single most valuable unshipped feature.
- [ ] `NEXT` **Cross-session attention badges** — a session waiting on a permission/Ask shows a badge on its tab + inbox row + the **browser favicon** (Devin's status-dot trick for peripheral awareness) + (with push) the phone. Nothing should silently block.
- [ ] `NEXT` **Delegation tree view** — visualize the fan-out: parent session → subagents → ACP delegates as a live graph with per-node status/cost (Amp's thread map; Crucible's delegation depth/trust model gives the edges for free)
- [ ] `NEXT` **Diff review center** — a per-turn "changes" view aggregating every file the turn touched, with per-file accept/revert backed by `WorkspaceSnapshot` (Cursor's review flow, but daemon-truth). Pairs with turn undo.
- [ ] `NEXT` **Scheduled & proactive agents UI** — `cru.schedule` and webhook triggers exist daemon-side; show scheduled/recurring runs, their last results, and a "run now"; kiln-digest results land in the inbox
- [ ] `NEXT` **Workflow runs panel** — the markdown workflow engine (parallel steps, gates, resumability) has zero web surface. Show DAG progress, step outputs, gate approvals, resume-from-interruption. (The gate-approval interaction reuses the permission modal pattern.)
- [ ] `LATER` **Usage & cost dashboard** — per-session and rollup token/cost/cache-hit stats (`session.cache_stats` RPC exists); provider breakdown. Lesson from the ops field: dashboards report overspend after the fact — pair with a **fail-closed session budget** (daemon-enforced hard cap that pauses the session, not just an alert).
- [ ] `LATER` **Session replay** — step through a finished session event-by-event (the JSONL is already recorded; this is a scrubber over fixture-replay infrastructure that already exists for tests)
- [ ] `LATER` **Audit trace: artifact → session** — every agent-written note/commit carries a session link (frontmatter / commit trailer), so "why does this note say that?" jumps from the note to the exact turn that wrote it (GitHub's `Agent-Logs-Url` pattern; sessions-as-notes makes it a wikilink)

### 3.3 Knowledge: editor, notes, graph, search

This is where "Obsidian, if the vault could act" is won or lost. Currently the thinnest area relative to ambition.

**Editor**
- [x] CodeMirror 6, markdown mode, dirty-state tabs, Ctrl+S save via `/api/kiln/file` (byte-exact, kiln-contained)
- [x] Multi-file tabs — bugs 5 (spurious dirty flag on reused CM instance) and 6 (silent discard on close) FIXED 2026-07-12; shared `CodeMirrorEditor` component, `contentSync` annotation, close confirms
- [x] Syntax highlighting beyond markdown — FIXED 2026-07-12 (was bug 7): md/js/jsx/ts/tsx/rs mapped in the shared `getLanguageExtension()`
- [x] **Unsaved-changes guard** on close — FIXED 2026-07-12 (was bug 6): `closeFile()` + `confirmTabClose()` prompt before discarding dirty files
- [-] `NEXT` **Live-preview markdown editing** — PARTIAL 2026-07-16 (WS-210): Edit ↔ Preview toggle shipped (`EditorWithPreview`, corner button + Mod-Shift-E) rendering through the chat markdown pipeline — headings/lists/code render, wikilinks are live `data-note` anchors (hover cards + click-to-open). Remaining: Obsidian-style inline WYSIWYG (rendered inline with source under cursor), callout/LaTeX/transclusion renderers.
- [x] **Vim keybindings** — SHIPPED 2026-07-16 (WS-210): `@replit/codemirror-vim`, default ON, Settings → Editor toggle (persisted, applies live to open editors).
- [x] **Editor chrome on shell tokens** — SHIPPED 2026-07-16: oneDark's blue-grey background/gutters overridden to the ember shell palette (`crucibleEditorChrome`); wikilinks styled as ember pills (hover underline) distinct from plain links.
- [-] `NEXT` **Wikilink intelligence in the editor** — PARTIAL 2026-07-15: link decorations + Ctrl/Cmd+Click and Mod-Enter follow + hover previews shipped (CodeMirror `wikilink-extension`, WS-209). Remaining: `[[` autocomplete (composer already has it), create-on-click for missing targets, rename-updates-backlinks (daemon-side refactor op)
- [ ] `NEXT` **Frontmatter/properties editor** — structured key/value UI over YAML frontmatter (Obsidian Properties); tags editable as chips
- [ ] `NEXT` **Transclusion rendering** — `![[Note#block]]` embeds render in preview (the parser already resolves block refs)
- [ ] `LATER` **AI-enhance selection** — select text in a note → improve/summarize/expand via a session-less agent call (Open WebUI Notes pattern)
- [ ] `NEXT` **Fix the note-write path** — `PUT /api/notes` currently skips hashing/embedding; web-written notes must be first-class graph citizens immediately (daemon pipeline call, not a frontend feature, but the UI depends on it)
- [ ] `LATER` Templates/note types (typed creation: meeting, book…), daily-note affordance

**Knowledge navigation**
- [x] Files/notes tree (workspace + kiln sections), click-to-open
- [x] `NOW` **Backlinks panel** — SHIPPED 2026-07-15 (WS-207): dockable right panel for the focused note; linked mentions via new `get_backlinks` RPC (reverse `note_links` index, stem/path/title candidate matching) + `GET /api/backlinks`; unlinked mentions via `suggest_links` over the note's content with one-click wikilink insertion into the open buffer. Scope note: unlinked = mentions *in this note*; Obsidian-style incoming unlinked mentions (kiln-wide text scan) deferred.
- [x] **Quick switcher** — note-open inside the omnibox (§2.2, `[[` prefix; fuzzy scored matching since 2026-07-12)
- [ ] `NEXT` **Knowledge Graph view** — interactive force-directed wikilink graph (global + local "this note's neighborhood"), filters by tag/folder/type, **sessions render as nodes** (sessions-as-notes means conversations are part of the graph — no competitor has this), semantic-similarity edges as an optional overlay from embeddings. This is the demo moment; treat it as a first-class panel, not a plugin afterthought.
- [ ] `NEXT` **Unified search panel** — one search UI over semantic (`search_vectors`), full-text, and property/tag search, with scope chips (kiln/sessions/files), result previews, and "open all in graph." Session search API already exists.
- [ ] `NEXT` **Outline panel** — headings of the focused note/session (the stub already sits in the default layout)
- [ ] `NEXT` **Related-notes sidebar** — embedding-similarity "related" panel for the focused note (LanceDB already answers this query; Obsidian needs a plugin for what Crucible has natively)
- [ ] `LATER` **Structured data views (Bases)** — table/kanban/gallery views over frontmatter queries, saved as `.base`-like note files; the query system exists daemon-side. The canonical "plugin, not core" feature once panel plugins land.
- [ ] `LATER` **Canvas** — infinite spatial workspace mixing notes + live session panels (JSON-canvas-compatible); explicitly P3

### 3.4 Sessions & history

- [x] Create/switch/resume with full history hydration; pause/resume/end/archive; search; export; title edit + auto-title
- [x] Honest `/clear` — DONE: the command response and /help state view-only semantics (server history preserved); an actual server-side history op stays deferred (TUI :clear ends + recreates the session; ACP sessions reject clear)
- [ ] `NEXT` **Session browser as notes** — sessions are markdown in the kiln; the note browser/graph should treat them as notes (tagging, linking a session from a note, "sessions that touched this note")
- [ ] `NEXT` **`cru share` in the UI** — export a session as a self-contained HTML artifact with one click. Default private, explicit opt-in, easy unshare — Amp *removed* public thread sharing after it leaked sensitive code; design for that failure up front.
- [ ] `NEXT` **Project grouping in the sessions panel** — group/filter by project/kiln with per-group collapse; auto-archive policy for ended sessions
- [ ] `LATER` Cross-device continuity affordances — "open this session in TUI" copy-command / deep link; QR for phone handoff (the daemon architecture makes this nearly free)

### 3.5 Extensibility (the Neovim payoff)

Nothing here is shipped for the web. It is the defining long-run bet: the panel registry exists but is compile-time; plugins cannot touch the browser yet.

- [ ] `NEXT` **Plugin panel hosting** — the one Rust primitive still missing (Wave 4): sandboxed iframe + postMessage protocol, so a Lua plugin can register a web panel (`crucible.web.register_panel{...}`) that appears in the panel registry, opens via palette, docks anywhere. Capability-scoped API bridge (which RPCs the panel may call) mirrors the plugin permission model.
- [ ] `NEXT` **Oil node renderer** — `<OilNode>` SolidJS component tree over the serialized Oil `Node` JSON (serialization already shipped). Then a plugin's *TUI* modal/panel renders in the browser for free — one UI DSL, two surfaces. Scripted UI docs currently overclaim this; make it true.
- [ ] `NEXT` **Content renderer registry** — Mermaid, LaTeX/KaTeX, ABC/music, CSV-as-table, image embeds… each a registered content-type handler usable in chat messages *and* the editor preview; plugins can add renderers
- [ ] `NEXT` **Agent artifacts** — when a turn produces a substantial output (document, diagram, HTML, SVG), render it in a side panel with version history per turn, "save to kiln" as the persistence action (Claude Artifacts, but artifacts are notes — plaintext-first)
- [ ] `LATER` **Agent-driven UI (generative panels)** — the agent itself emits Oil nodes / declarative panel specs as a tool call ("show me a table of X" → live panel). OpenClaw A2UI territory; Oil serialization is the enabling primitive.
- [ ] `LATER` **Lua playground panel** — in-browser REPL against the daemon (`lua.eval` RPC exists) with the LuaCATS stubs for completion; the plugin-dev story (LT-FULL-2) and the "inspect anything" escape hatch
- [ ] `LATER` **Plugin discovery UX** — browse/install from git URL (API exists), health status, README rendering; ratings/registry deferred until a registry exists
- [ ] `NEXT` **MCP connector management** — beyond the shipped status view: add/remove/reload upstream servers, browse each server's tools, per-server logs (AnythingLLM's management pattern; a connector *directory* is LATER)
- [ ] `NEXT` **Agent card & skills authoring** — SkillsPanel ships read-only; add create/edit for agent cards (persona + prompt + tools + model) and skills (`SKILL.md`) in the editor with schema-aware frontmatter forms — "custom GPTs," but as kiln files

### 3.6 Agent memory & learning (Crucible-unique supervision)

The learning system (kiln-notes-as-memory, reflection proposals, precognition) is the moat, and it currently has *zero* web surface. These features are cheap relative to their differentiation because the daemon side already exists.

- [ ] `NEXT` **Proposals inbox** — the reflection pass stages agent-proposed notes in `.crucible/proposals/`; `cru proposals {list,show,accept,reject}` is CLI-only. A review queue with note preview, diff-against-existing, accept/reject/edit-then-accept is a perfect web feature — human-in-the-loop learning as a first-class screen. Badge it in the Agent Inbox.
- [ ] `NEXT` **Precognition transparency & citations** — the enriched context is invisible today beyond a badge. Two surfaces: (a) click the badge → panel showing exactly which notes/blocks were injected with similarity scores, each linking into the editor/graph; (b) long-run, **answers cite their graph sources inline** (NotebookLM/Notion Q&A convention — clickable note citations in the response). Builds trust and doubles as a retrieval-quality debugger. ("Why does the agent know this?" answered in one click.)
- [ ] `LATER` **Misleading-knowledge feedback** — from the citation panel, mark an injected note as *misleading for this task*; the signal accrues on the note (frontmatter) and informs future retrieval/reflection. Devin's Session Insights pioneered the loop, but only Crucible has an editable graph to close it on. Uniquely defensible.
- [ ] `LATER` **Session → structured note distillation** — one-click "distill this session" producing a schema-tagged note (attendees/decisions/follow-ups style field extraction — Tana supertags pattern), staged through the proposals inbox like all agent writes
- [ ] `NEXT` **Memory browser** — a filtered note-browser view of agent-written knowledge (provenance frontmatter: agent-created vs user-authored), sortable by recency/session, editable in place. "What has my agent learned?" is a question no competitor can answer with editable plaintext.
- [ ] `LATER` **Skill self-creation review** — when agents distill skills from sessions (planned), the same proposals-inbox pattern gates them

### 3.7 Settings, config & ops

- [x] Settings panel — thinking budget, temperature, max tokens, precognition (+result count), plugins, MCP status, transcription, API access token
- [x] Auth — Bearer token (programmatic), localhost bypass, browser sign-in via `POST /api/auth/login` → HttpOnly session cookie (covers SSE; replaced the `?token=`/`?access_token=` URL flows 2026-07-16 — query-string tokens leak via history/logs/referrers), 401 → in-UI sign-in prompt, Settings sign-in, `cru web key` CLI
- [ ] `NOW` **Session-knob parity** — 9 of 14 session knobs are RPC-only (max_iterations, execution_timeout, context_budget, context_strategy, output_validation, validation_retries, autocompact_threshold, system_prompt…); mirror them in Settings (routes are macro-friendly)
- [ ] `NOW` **Agent selection at session create** — `create_session` hardcodes the internal agent; expose agent cards + ACP agents (claude/opencode/gemini) in the new-session flow. Unlocks "Crucible as command center for external agents" in the browser — a headline capability that works in the TUI today.
- [ ] `NEXT` **Config editor** — schema-driven form over `config.toml` (schema generated from config types), with raw-TOML escape hatch
- [ ] `NEXT` **Provider & model management** — per-provider API keys/base URLs (beyond the shipped web-key field), Ollama model pull/list with non-blocking downloads (Open WebUI/Jan pattern)
- [ ] `LATER` **Incognito sessions** — a session flag that skips precognition read *and* memory/session-note write; table stakes in memory-bearing chat UIs and a clean fit for the scoping model
- [ ] `NEXT` **System info panel** — daemon health, kilns, embedding/index stats, MCP server status, plugin provenance (`:plugins` parity)
- [ ] `NEXT` **Log viewer** — SSE-streamed daemon logs with level/module filters (also the plugin-debugging story)
- [ ] `NEXT` **First-run onboarding** — web equivalent of the TUI setup wizard: kiln selection, provider detection, model pick, connect-URL/QR for other devices
- [ ] `LATER` OpenAPI spec served at `/api/openapi.json` (ship the spec, skip the custom playground)

### 3.8 Platform & distribution

- [x] Single-binary serving (rust-embed), systemd-friendly (`cru web`), CORS restraint, localhost-gated shell endpoint
- [ ] `NEXT` `cru tunnel` (Cloudflare/Tailscale) — one command to phone-ready remote access; print/QR the connect URL
- [ ] `LATER` Tauri desktop wrapper — menubar agent status, native notifications, global quick-capture hotkey
- [ ] `LATER` Multi-user — profiles/auth beyond a single shared key (needed only when collaboration lands; P4). Hard requirement noted now: retrieval/precognition must enforce tenant isolation *at the query layer* (a 2026 LobeChat advisory disclosed cross-user RAG leakage; Crucible's `Scope::Workspace` filter is the right shape — extend it, don't bypass it).

---

## 4. Completeness check vs. other agent UIs

Cross-checked against Claude Code (web/desktop), claude.ai, ChatGPT/Codex, Cursor, Windsurf, Zed, OpenCode, Copilot Workspace, Devin/Jules, Open WebUI, LibreChat, LobeChat, AnythingLLM, and Obsidian/Logseq/Notion. Features are **[T]** table-stakes (multiple products) or **[D]** differentiator (worth stealing).

**We have it (or partial):** streaming/thinking/tool cards, permission approval with diffs [T], mode/plan-act switch [T], model switching [T], session list/resume/archive [T], export [T], voice input [D — most web UIs lack local Whisper], command palette [T], dockable IDE layout [D — only Zed/IDEs have real docking], PWA [T], self-host auth [T].

**Gaps that are table-stakes elsewhere (all catalogued above):**

| Feature | Who has it | Where covered |
|---|---|---|
| Multi-agent inbox / task queue | Codex, Cursor BG agents, Claude Code (web), Devin, Jules | §3.2 Agent Inbox |
| Regenerate / edit-and-branch | ChatGPT, claude.ai, LibreChat, LobeChat, Open WebUI | §3.1 branching |
| Checkpoints / restore | Cursor, Claude Code (/rewind), Windsurf | §3.1 turn undo, §3.2 diff review |
| Aggregated diff review, accept/reject | Cursor, Copilot Workspace, Codex | §3.2 diff review center |
| Plan approval card | Claude Code, Copilot Workspace, Devin | §3.1 plan-approval |
| Agent todo/progress checklist | Claude Code, Devin, Jules | §3.1 task display |
| Image/file attachments | every general chat UI | §3.1 attachments |
| Prompt templates/library | Open WebUI, LibreChat, LobeChat, Cherry Studio | §3.1 prompt library |
| Scheduled/recurring tasks UI | ChatGPT tasks, Devin, Claude Code (cron), OpenClaw heartbeat | §3.2 scheduled agents |
| Usage/cost dashboard | Open WebUI, LibreChat, Cursor, provider consoles | §3.2 usage |
| Web push for approvals | Devin, Cursor mobile, Claude (mobile apps) | §2.4 push |
| Session share links / HTML export | claude.ai, ChatGPT, Devin | §3.4 cru share |
| Memory management UI | ChatGPT memory, claude.ai projects | §3.6 memory browser (ours is strictly better: plaintext) |
| Knowledge-base workspaces / RAG doc mgmt | AnythingLLM, Open WebUI, LobeChat | native — the kiln *is* this; needs §3.3 surfaces |
| Graph view / backlinks / quick switcher | Obsidian, Logseq, Roam | §3.3 |
| Live-preview markdown | Obsidian, Notion, Typora | §3.3 editor |
| Bases / DB views + query language over notes | Obsidian Bases, Logseq DB, Dataview, Tana | §3.3 structured views |
| Source-cited answers over the KB | NotebookLM, Notion Q&A, Capacities | §3.6 citations |
| MCP connector management / directory | claude.ai connectors, ChatGPT, LobeChat, Cherry, AnythingLLM | §3.5 MCP management |
| Custom agents / personas UI | ChatGPT GPTs, GitHub custom agents, Msty, LibreChat | §3.7 agent cards |
| Provider keys + local model mgmt | Open WebUI, Jan, Cherry, LobeChat | §3.7 provider mgmt |
| Incognito / temporary chat | ChatGPT, Claude, LibreChat | §3.7 incognito |
| Best-of-N parallel drafts | Codex `--attempts`, Jules `--parallel`, Cursor `/multitask` | §5 compare-branches |
| TTS out | ChatGPT voice, Open WebUI, LobeChat | §3.1 TTS |
| Side-by-side model compare | LibreChat, LobeChat, Open WebUI, Msty | §3.1 compare |
| In-app browser preview of built apps | Codex, Jules, Devin, Copilot Workspace | deliberately out of scope for now (see §6) |

Two independent validations from the survey worth recording: Jan's and Cherry Studio's tool-approval cards are near-identical to Crucible's shipped permission modal, and LibreChat's "Deferred Tools" independently converged on Crucible's progressive tool disclosure — both confirm existing design bets rather than gaps.

**Verdict:** with §3 fully built, Crucible's web UI covers every table-stakes feature in the landscape except app-preview sandboxes (a deliberate non-goal), and holds six capabilities nobody else combines: live knowledge graph, plaintext agent memory with a review workflow, precognition transparency, real workspace-rollback undo, one-session-many-consoles (TUI↔web), and Lua-extensible panels.

---

## 5. High-value adds (brainstorm)

Beyond parity — ordered roughly by (differentiation × feasibility). The first four all exploit daemon capabilities that already exist.

1. **Proposals inbox** (§3.6) — human-in-the-loop agent learning as a screen. The reflection plumbing is done; this is pure UI. Nobody else has "review what your agent wants to remember."
2. **Precognition transparency panel** (§3.6) — turn the differentiator from invisible magic into an inspectable, trust-building surface. Also the retrieval debugger.
3. **Turn undo + diff review center** (§3.1/3.2) — chat-integrated *workspace* rollback; competitors' checkpoints don't tie into a knowledge graph or show what a turn touched across kiln + workspace.
4. **Sessions in the graph** (§3.3) — conversations as first-class graph nodes: "show me every session that touched this note." Retrieval, provenance, and a demo moment in one.
5. **Wikilink hover-preview everywhere** — SHIPPED for chat messages, editor decorations, and backlinks rows 2026-07-15 (one `data-note`-driven component, WS-208); remaining surfaces: graph tooltips, permission diffs.
6. **Graph trail during a turn** — live mini-graph in the session showing notes read/written as the agent works; precognition edges pulse. Uniquely honest "watch it think with your knowledge."
7. **One UI DSL, two surfaces** — Oil renderer means every plugin modal/panel works in TUI and browser. That's a plugin-ecosystem multiplier no TUI-or-web-only competitor can copy.
8. **Agent-driven panels (generative UI)** — agent tool emits a declarative panel (table/form/chart) rendered live; combined with kiln data queries this is "ask for a dashboard, get a dashboard."
9. **Kiln digest as inbox item** — proactive "you wrote about X twice this week — link them?" cards with one-click accept (auto-linking RPC exists). OpenClaw's heartbeat, but knowledge-grounded.
10. **Quick capture + share target** — frictionless "into the kiln from anywhere" (mobile share sheet, global hotkey in Tauri); capture becomes agent-visible immediately via the pipeline.
11. **Cross-surface handoff** — "continue in terminal" copies `cru chat --resume <id>`; QR opens the session on the phone. Cheap, and it *markets the architecture*.
12. **Compare-branches view** — fork a session to two models/approaches (fork RPC exists), review answers side-by-side, keep one. Judge-panel workflows made visual.
13. **Session replay scrubber** — training/demo/debugging value; the JSONL events and replay machinery already exist for tests.
14. **Command-palette knob editing** — `:set` parity in the palette with live autocomplete of every session knob (once knob parity ships), keeping power-user muscle memory identical across TUI and web.
15. **Misleading-knowledge feedback loop** (§3.6) — mark a cited note as "led the agent astray"; the flag lives in the note's frontmatter and tunes retrieval/reflection. Devin proved the loop; only a plaintext graph makes it *editable*.
16. **Note→session audit trail** (§3.2) — agent-written notes wikilink the session that wrote them; `git blame` for knowledge. Cheap (frontmatter at write time) and unmatched for trust.
17. **Deny-with-feedback** (§3.1) — rejection becomes steering; the hook contract (`{cancel, reason}`) already carries it end-to-end.

---

## 6. Non-goals (long run)

- **Cloud-hosted execution sandboxes / app preview VMs** (Codex/Jules territory) — Crucible is local-first; execution isolation is a *plugin* concern (`oci`), not a hosted-infra product.
- **A second state store in the browser** — no offline-first CRDT cache until P4 sync exists daemon-side; the SW caches the app shell only.
- **A web-only plugin API** — plugins target Crucible (Lua + Oil + capabilities); the browser is a render target, not a separate platform.
- **Custom API playground** — ship OpenAPI, let standard tools do the rest.
- **Paid multi-tenant hosting** — deferred per Product decision log; `cru tunnel` self-hosting is the story.

---

## 7. Sequencing summary

- **NOW (finish the shipped story):** omnibox `@`/`#` prefixes · Agent Inbox landing page · light theme · knob parity + agent selection · note-write embedding · message queueing/regenerate · turn-undo affordance · popouts *(done: editor data-safety bugs 5/6/7, honest `/clear`, omnibox fuzzy — 2026-07-12 · backlinks panel + wikilink hover previews + editor follow-links — 2026-07-15)*
- **NEXT (the knowledge-console arc):** graph view · unified search · live-preview editor + properties + transclusion · proposals inbox · precognition transparency/citations · memory browser · plugin panel hosting + Oil renderer + renderer registry · artifacts · diff review center · delegation tree · workflow runs · scheduled agents · attachments · branching + side chat · deny-with-feedback · MCP management · agent-card/skills authoring · mobile mode + push · config editor + provider mgmt · onboarding · `cru tunnel`
- **LATER (mature product):** bases/structured views · canvas · agent-driven UI · Lua playground · session replay + audit trail · usage dashboard + budget caps · misleading-knowledge feedback · session distillation · incognito · TTS/compare · Tauri desktop · community themes · multi-user

## Links

- [[Meta/Product]] — whole-product inventory (§Web & Desktop)
- [[Meta/Web User Stories]] — acceptance criteria + test tiers for shipped features
- [[Meta/FlexLayout-Spec]] — windowing behavioral contract + layout gap tiers
- [[Meta/Roadmap]] — wave sequencing (Waves 4/5/9/10 are the web arc)
- [[Meta/Plugin User Stories]] — personas the extensibility features serve
