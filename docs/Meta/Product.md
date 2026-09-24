---
title: Product
description: Product feature map — capabilities, status, documentation, and dependencies
type: product
status: active
updated: 2026-09-22
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
>
> Shipped and in-progress entries carry a **Gets you** sub-bullet (what a user observes).
> Planned `[ ]` entries carry no sub-bullets — there is nothing yet to observe.

## Vision

A **knowledge-grounded agent runtime**. Agents that draw from a knowledge graph make better decisions — memory and knowledge are too fundamental to be an afterthought. Your notes, sessions, and wikilinks form that graph. Everything beyond the knowledge core is extensible.

- **Knowledge + Agents** — the core. Agents draw from and contribute to a knowledge graph. [[Help/Concepts/Precognition|Precognition]] injects relevant context before the first turn of a conversation. Sessions persist as linked notes. The more you use it, the smarter it gets.
- **PKM as input** — notes, wikilinks, tags, and sessions-as-notes are how knowledge enters the system. Not an add-on; essential infrastructure.
- **Neovim-like architecture** — Lua extensibility, TUI-first, headless daemon with RPC, plugin-driven. Most behaviors beyond the knowledge core can be scripted.
- **Plaintext-first** — you own everything as markdown files. The daemon is an implementation detail. Simple at rest, powerful when running.

## User Progression

| Phase | Users | Interface |
|-------|-------|-----------|
| Now | Power users, developers | CLI (chat-focused) + web UI |
| Next | Plugin creators, agent developers | CLI + Lua scripting + messaging integrations |
| Later | Broader audience, mobile users | Web PWA (self-hosted via Tailscale/Cloudflare Tunnel) |

---

## Note-Taking & Authoring

- [x] **Wikilinks** `P0` — `[[note]]` linking with aliases · [[Help/Wikilinks]] · `crucible-core` (parser), `crucible-daemon`, `crucible-web`
  - **Gets you:** `[[note]]` renders as a clickable anchor resolving to the target note; the target's Backlinks panel lists the linking note; an alias displays but still resolves the real target. The anchor reads the same in the transcript and in a note, in both themes, and the title drops the extension: `[[Getting Started.md]]` is one link reading "Getting Started".
- [-] **Wikilink Heading & Block Fragments** `P0` — `[[Note#Section]]` and `[[Note#^id]]` fragment targets · `crucible-core` (parser)
  - **Gets you:** nothing beyond a plain note link — both forms open the note at the top.
- [x] **Link-Preserving Refactor** `P0` — renaming or moving a note rewrites every inbound wikilink; `fs.list_dir` / `fs.move` / `fs.mkdir` / `fs.trash` and `note.rename` / `note.move` back the web file tree's drag-and-drop · `crucible-daemon`, `crucible-web`
  - **Gets you:** you move a note and every link to it still resolves — with each author's original decorations kept (alias, heading, block-ref, embed marker, path style), ambiguous targets and code blocks left alone, and `.canvas` references re-pointed.
- [x] **Tags** `P0` — `#tag` and `#nested/tag` (stored as flat strings; no hierarchy rollup) · [[Help/Tags]] · `crucible-core` (parser)
  - **Gets you:** a tag in the body or in frontmatter lands in the note's indexed tag list, and `property_search {"tags": [...]}` returns the matching notes.
- [x] **Frontmatter** `P0` — YAML (`---`) and TOML (`+++`) metadata in note headers · [[Help/Frontmatter]] · `crucible-core` (parser)
  - **Gets you:** `property_search {"status":"draft"}` returns only the matching note, and `list_notes --include-frontmatter` returns the parsed block.
- [-] **Block References** `P0` — `^block-id` paragraph-level linking · [[Help/Block References]] · `crucible-core` (parser)
  - **Gets you:** nothing. `^block-id` is inert text everywhere — nothing defines, resolves, renders, or embeds a block.
- [x] **Callouts** `P0` — `> [!type]` admonition blocks · `crucible-web` (markdown-it plugin)
  - **Gets you:** `> [!note] Title` renders as a styled callout box with icon and title in the web reading view and live preview; `> [!tip]-` renders a collapsed `<details>`.
- [x] **LaTeX** `P0` — `$inline$` and `$$block$$` math notation · `crucible-web`
  - **Gets you:** `$$…$$` renders as a KaTeX widget in the editor's live preview and reading view, while a `$$` block inside a code fence correctly stays as source.
- [ ] **Footnotes** `P0` — reference-style footnote rendering, not implemented
  - **Gets you:** nothing — `[^1]` renders as literal text in both the web reading view and the TUI.
- [x] **Tables** `P0` — markdown tables · `crucible-core` (parser), `crucible-cli`, `crucible-web`
  - **Gets you:** a pipe table renders as a box-drawn table in the TUI (respecting terminal width and CJK cell widths) and as an HTML table in the web reading view; the editor can reformat pipe alignment.
- [x] **Task Lists** `P0` — `- [ ]` / `- [x]` checkbox items · `crucible-core` (parser), `crucible-web`
  - **Gets you:** checkbox list items render with their checked state and the literal brackets removed. The rendered checkbox is `disabled` — you cannot tick a box in the UI to update the file.
- [x] **Task Harness (`TASKS.md`)** `P0` — structured task files with phases, task IDs and a dependency graph · [[Help/Task Management]] · `crucible-core` (parser), `crucible-cli`
  - **Gets you:** `cru tasks list` / `next` / `pick` / `done` / `blocked` read and mutate checkbox tasks in a markdown task file, with dependencies resolved through `TaskGraph`.
- [x] **Kilns** `P0` — vault-like note collections with `.crucible/` config · [[Help/Concepts/Kilns]] · `crucible-core`, `crucible-daemon`
  - **Gets you:** a directory with `.crucible/kiln.toml` opens as a kiln over RPC; `kiln.list` shows it, `list_notes` returns its notes, `kiln.close` removes it.
- [x] **JSON Canvas File Format** `P0` — read and write `.canvas` (JSON Canvas 1.0, Obsidian's format) · `crucible-core` (canvas)
  - **Gets you:** an existing Obsidian vault opens without conversion, and a canvas Crucible saves is byte-identical to Obsidian's — tab indentation, one object per line, key order preserved. Unknown keys authored by third-party plugins round-trip verbatim.
- [x] **Plaintext First** `P0` — markdown files are always the source of truth · [[Help/Concepts/Plaintext First]]
  - **Gets you:** edits and refactors land as markdown bytes on disk; the editor's save carries the exact buffer, and a rename rewrites real files rather than an index.
- [ ] **Note Types** `P3` — templates and typed notes (book, meeting, movie) · `crucible-core`

## Knowledge Discovery

- [x] **Semantic Search** `P0` — vector similarity search over kiln notes · [[Help/Concepts/Semantic Search]] · `crucible-daemon` (storage, llm)
  - **Gets you:** a natural-language query is embedded and returns ranked note paths with similarity scores — via `cru search --type semantic`, the web Text|Semantic toggle, and the agent's `semantic_search` tool.
- [x] **Content Search (ripgrep)** `P0` — `search_grep` RPC + `POST /api/search/grep` + the agent's `grep_notes` tool · `crucible-daemon` (tools)
  - **Gets you:** searching for a word that appears only in a note's *body* returns that note with the matching line, line number and match offsets for highlighting; a `root` is accepted only if it canonicalizes inside a registered project or open kiln.
- [x] **`cru search` Text Mode & FTS5 Index** `P0` — index-backed full-text search from the CLI, BM25-ranked over titles and bodies · [[Help/CLI/search]] · `crucible-daemon` (storage), `crucible-cli`
  - **Gets you:** `cru search <word>` finds notes containing that word anywhere in their body, not just in the filename or title, ranked by relevance. The pipeline writes the index as notes are processed, deletes drop out of it, and a kiln indexed by an older build is backfilled once on open.
- [x] **Knowledge Graph** `P0` — wikilink-based graph structure and backlinks · [[Help/Concepts/The Knowledge Graph]] · `crucible-daemon` (storage)
  - **Gets you:** `kiln.graph` returns every visible note plus its resolved and dangling links, which the web renders as an Obsidian-style graph; `get_backlinks` drives the Backlinks panel.
- [x] **Backlinks API** `P0` — `get_backlinks` RPC + `GET /api/backlinks`, linked *and* filtered-unlinked mentions with byte spans · `crucible-daemon`, `crucible-web`
  - **Gets you:** the backlinks panel renders the line containing each wikilink, lists unlinked mentions of the note, and hovering scrolls the preview to that section.
- [x] **Canvases as Graph Citizens** `P0` — a `.canvas` contributes its file cards and the wikilinks inside its text cards as real graph links · `crucible-daemon` (pipeline)
  - **Gets you:** a note's backlinks list the canvases that reference it, and canvas references survive renames and moves. Containment redacts out-of-root references on the read path, so a client never receives a path it could not have asked for.
- [-] **Graph Traversal** `P0` — n-hop / neighbourhood queries over the graph · `crucible-daemon` (storage)
  - **Gets you:** nothing a user or agent can call. `kiln.graph` hands back a flat edge list; all traversal happens client-side in the web graph view.
- [ ] **Query System** `P0` — structured note queries with a composable pipeline · [[Help/Query/Query System]] · `crucible-daemon` (storage)
  - **Demoted `[-]` → `[ ]` 2026-08-18.** The subsystem this entry described is deleted, so there is no work in progress to be in the middle of. `storage/sqlite/query/` (~5,739 lines: an IR, four syntax front-ends, a SQLite renderer, 17 snapshots) was removed on 2026-08-11 — not for want of callers, but because the renderer targeted a schema that never existed. See [[Meta/Product Decision Log]] for the full reasoning and the deletion SHA.
  - **Documentation:** [[Help/Query/Query System]] already marks the query language unavailable and directs users to the live search commands.
- [x] **Property Search** `P0` — search notes by frontmatter properties and tags · `crucible-daemon` (tools)
  - **Gets you:** an agent calling `property_search {"status":"draft"}` or `{"tags":["urgent","important"]}` gets JSON listing only the matching notes with their paths and tags.
- [-] **Document Clustering** `P0` — heuristic clustering and MoC detection · `crucible-daemon` (storage)
  - **Gets you:** nothing. The implementation was deleted with `crucible-surrealdb` on 2026-02-23 and never reimplemented; the `cru cluster` command went earlier.
- [ ] **K-Means Clustering** `P2` — k-means implementation; from scratch (the stub this once referred to was deleted with `crucible-surrealdb` on 2026-02-23), and depends on Document Clustering being rebuilt first · `crucible-daemon` (storage)
- [x] **Block-level Embeddings** `P0` — paragraph-granularity semantic indexing · `crucible-daemon` (llm, storage)
  - **Gets you:** precognition injects the *passage* that matched, with its kind and byte span, instead of the whole file it sat in. A note contributes as many hits as it has relevant blocks. Re-indexing a kiln whose text has not changed pays for no new vectors: a forced re-index of the 237-note adversarial corpus falls from 6.7 s to 2.2 s.
  - **Known limit:** a heading under five words gets a row and no vector, so it is stored but never a hit. (`search_vectors` and `cru search` moved onto the block-first path on 2026-09-02: `tests/rpc_integration/notes.rs`::search_vectors_rpc_names_the_block_that_answered.)
- [x] **Session Search** `P0` — text search across past conversations · `crucible-daemon` (observe), `crucible-cli`
  - **Gets you:** `cru session search "<query>"` prints matching session ids with the line number and surrounding context from the session JSONL.

## Agent Learning & Memory

> Agents that get smarter over time. Learning is implemented as **notes in the kiln** — not opaque database stores. Entity facts, session summaries, and accumulated knowledge are all atomic zettelkasten-style markdown notes with wikilinks, tags, and frontmatter. This means agent memory is human-readable, editable, searchable via the existing knowledge graph, and available to precognition for future context injection.
>
> **Two-tier model**: Core Rust features (precognition, auto-linking) handle the fast path. Default runtime Lua plugins handle higher-level knowledge extraction. Both are toggleable and overridable. See [[#Core Agent Features]] and [[#Default Runtime Plugins]].
>
> **Informed by**: Agno framework analysis (2026-02). Agno uses six opaque DB-backed learning stores. Crucible's approach is strictly better — same learning capabilities but with human-readable, editable, wikilinked notes as the storage layer.

- [x] **Precognition** `P0` — Auto-RAG: inject relevant kiln context before the agent's first turn; the core differentiator — every conversation starts knowledge-graph-aware · [[Help/Concepts/Precognition]] · `crucible-daemon` (agent_manager/precognition), `crucible-cli`
  - **Gets you:** kiln notes matching your opening message are prepended to the message list the agent actually receives, with a `precognition_complete` event carrying the note list to the TUI and web. It fires on the **first user message of a session only**, not before every turn, and it is on by default.
- [x] **Precognition Toggle** `P0` — `:set precognition` turns injection off for a session, in all four spellings (`precognition=off`, `noprecognition`, `precognition`, `precognition!`) · `crucible-cli`, `crucible-daemon`
  - **Gets you:** turning precognition off in the TUI actually stops the daemon injecting kiln context — for every client on that session, and for the session's remaining life, not just this client's `:set` readout.
- [x] **Precognition Selection Seam** `P0` — a Lua `precognition_select` handler can filter or reorder the candidate set before the agent sees it · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a plugin can veto or re-rank which notes get injected, and the agent receives the filtered set.
- [x] **Memory Scoping** `P2` — `Scope::Workspace { path }` enforced at the storage query layer · `crucible-core`, `crucible-daemon`
  - **Gets you:** a kiln-bound `KnowledgeRepository` cannot return notes belonging to a sibling workspace — `list_notes`, `get_note_by_name` and `search_vectors` all drop them. Vector isolation follows the same rule: embeddings live on each kiln's own `notes` rows, and the scope filter is applied in the search SQL itself.

### Self-Improvement Avenues

> Two complementary ways an agent gets smarter. **Knowledge insertion** is the primary path and ships today; the **reflection pass** is the second avenue, and it is deliberately propose-only. That constraint is the lesson from the removed `session-digest` plugin, which auto-merged session summaries via LLM-judged dedupe and risked wrong merges and kiln pollution. Both avenues write to the same place — atomic kiln notes — so improvement stays human-readable and editable.

- [x] **Knowledge Insertion (primary)** `P1` — agents persist learning *during* work by writing kiln notes via `create_note` / `update_note`; those notes re-enter future sessions through Precognition · `crucible-daemon` (tools)
  - **Gets you:** the agent writes a real `.md` file into the kiln, the tool result reports its path, and a later session's precognition can retrieve it. The graph *is* the learning store — no opaque DB.
- [x] **Reflection Pass** `P2` — on `on_session_end`, a separate auxiliary-model agent in a `plugin` session reviews the finished transcript and proposes the kiln notes it earned, for a human to accept or reject in the Inbox · [[Help/Concepts/Reflection Pass]] · `crucible-daemon`, `crucible-lua`, `runtime/plugins/reflection`
  - **Gets you:** the reviewer reads the session's tool calls and results, an outcome block (turns, tool calls, tool errors, edits by the agent that remain, edits made outside the agent's tools) and the titles precognition injected. It also reads the titles and reasons of the recent rejected proposals. It runs in its own `plugin` session, in `propose` mode, narrowed to the six kiln tools, and writes each note it earned with `create_note` or `update_note`. It searches and reads the kiln first, so a note that already covers the idea gets an `update`, not a duplicate. In `propose` mode each write goes into a proposal, and the note on disk does not change. A human accepts or rejects the proposal in the Inbox, in the diff pane or with `cru proposal`; a reject keeps the proposal and its reason, and changes no file. The session is titled `Reflection: <the reviewed session>`, and the proposal names the plugin that ran the pass as its author. The pass is still inert until `plugins.reflection.model` is set.
- [-] **Consolidation Pass** `P2` — a periodic pass in a `plugin` session reads several finished sessions at once, problem sessions first, never a `plugin` session, and proposes pattern notes with a "problem; root cause; fix" description · [[Help/Concepts/Reflection Pass#The consolidation pass]] · `runtime/plugins/consolidation`, `crucible-lua`
  - **Gets you:** off until `[plugins.consolidation] enabled = true`, because a pass spends model calls unattended. Each pass samples ended session versions it has not yet sampled — those with a tool error or a rejected edit first — and runs its review through the reflection plugin, so its pattern notes are proposals from its own session, titled `Consolidation: <the day it ran>`, and the one Inbox holds the proposals of both passes.

## Context & Execution (Core Runtime)

> Runtime primitives that every reliable agent needs. These are too fundamental to be plugins — they govern how the agent manages its own context window, enforces execution boundaries, validates its output, and lets users recover from mistakes. Informed by competitive analysis (2026-03): Aider, CrewAI, LangGraph, and Semantic Kernel all treat these as core concerns.

### Prompt Caching

- [-] **Anthropic Cache Control** `P0` — `CacheControl::Ephemeral` on system prompts and the second-to-last turn · `crucible-daemon`
  - **Gets you:** unproven for the cache-control half. The token *reporting* half works — cache read/creation counts flow through `message_complete` and reach the statusline. Whether the outgoing request actually carries the cache breakpoints is watched by nothing.
- [x] **Cache Stats** `P1` — per-session cache hit/miss aggregate exposed via `session.cache_stats` RPC, `cru.session.cache_stats(id)` Lua binding, and the `sl.cache` statusline item · `crucible-daemon`, `crucible-lua`, `crucible-cli`
  - **Gets you:** once a completion reports cache counts, the statusline renders `cache: 75%`; before that it renders nothing rather than a false `0%`.

### Context Window Management

- [x] **Token Budget Tracking** `P0` — every session derives a `context_budget`: an explicit `chat.context_budget`, else the window the provider reports for the model, else the shipped fallback. `estimate_tokens` chars/4 heuristic · `crucible-daemon`, `crucible-core`
  - **Gets you:** the budget you set is the budget the agent handle enforces on every request, and it sizes the tool-schema deferral decision. Still unset by default, so `usage.budget` and `usage.percent` read `0` until you set one.
- [-] **Auto-Compaction** `P0` — compact the conversation when prompt usage crosses `context_budget * chat.autocompact_threshold` (config key, default 0.95); also reachable as `cru.context.compact` and `session.request_compaction` · `crucible-daemon`
  - **Gets you:** nothing is ever compacted. Crossing the threshold flips the session's state string to `"compacting"` and that is the entire effect — the messages sent on the next turn are unchanged.
- [x] **Context Strategies** `P1` — `ContextStrategy::{Truncate, Summarize}` · `crucible-core`, `crucible-daemon`
  - **Gets you:** the session's strategy and budget reach the handle, so an over-budget conversation is really trimmed before the request goes out. `:set context_strategy=summarize` changes what the model sees. `SlidingWindow` was removed on 2026-09-11: it drained exactly what `Summarize` drains and left no marker in the hole.
- [ ] **Lua Context Strategies** `P1` — `ContextStrategy::Lua { name }` registered via `cru.context.register_strategy`. The `OutputValidation::Lua { name }` seam it was going to mirror is gone (removed 2026-09-10), so this one builds the registry rather than copying it. Lets a plugin compact into a **kiln note** — which joins the graph and is retrievable by precognition later — instead of an inline recap. A strategy callback may not trigger a turn on its own session · `crucible-core`, `crucible-lua`, `crucible-daemon`
- [x] **Lua Context Operations** `P1` — `cru.context.{usage, messages, remove, estimate_tokens}` · `crucible-lua`, `crucible-core`, `crucible-daemon`
  - **Gets you:** `cru.context.remove(id, {type="last", n=2})` actually shortens the conversation path the next turn is built from; `usage` returns a populated table.
- [x] **`cru.context.attach`** `P1` — mid-turn context attachment from a Lua handler · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a handler that finds something useful partway through a turn (say from a `tool_result`) can put it where the agent's *next* LLM call **in that same turn** will see it — deduped by key so a repeated trigger attaches once, capped by a per-session character budget with a typed rejection reason.

### Execution Limits

- [x] **No turn cap** `P1` — a turn runs as many tool rounds as the model asks for, for as long as it takes · `crucible-daemon`
  - **Gets you:** nothing cuts a turn short. There is no round cap and no wall-clock timeout; the user's cancel is the only bound. A long refactor and a plugin's reviewer session both run to completion, and a `bash` tool holding a dev server open does not end the turn.
  - **Why:** `max_iterations` defaulted to 10 rounds — well inside a normal refactor — and on reaching it the runtime injected a "give your final answer" prompt, then failed the turn outright if the model called another tool. `execution_timeout_secs` had the same shape and no test ever watched it fire. Both are gone, along with `TurnEvent::DepthCapHit`, `StopReason::MaxToolDepth` and the agent-card `max_turns` field. OpenCode's `steps` defaults to `Infinity` and Pi's agent loop is `while (true)`, so this matches both.

### Agent Undo

- [x] **Turn Undo** `P1` — `/undo [N]` reverts the last agent turn(s): file rollback via `WorkspaceSnapshot` plus message truncation · `crucible-daemon`, `crucible-cli`
  - **Gets you:** workspace files are restored to their pre-turn bytes on disk and the conversation the next turn is built from is rewound. Git mode uses `write-tree`+`commit-tree` for untracked-file safety; non-git mode uses an in-memory journal capped at 5 MiB. Two caveats: the **TUI viewport is not truncated** — you get a toast saying the turn was reverted while the reverted turn is still on screen — and the `SnapshotMap` is in-memory, so `/undo` after a daemon restart rewinds the chat and silently leaves files alone. `/redo` is deferred (no `redo_turns` on `ConversationTree`).
- [x] **Undo Lua API** `P1` — `cru.session.{undo, can_undo, undo_depth, undo_history}` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** `cru.session.undo(id, n)` performs a real undo (files + tree) and returns the turn count; `undo_history` returns per-turn `{turn_index, messages_removed}` tables.

### Output Validation

- [~] **Output validation, removed 2026-09-10** — `OutputValidation`,
  `validate_output`, `validation_retries`, the stream loop's validate-retry
  branch, `cru.context.register_validator`, `cru.session.set_output_validation`,
  the four `session.*_output_validation` / `*_validation_retries` RPCs, the two
  HTTP routes and the web controls are all gone.
  - **Why:** the retry re-entry was never proven — every test in the repo set
    `validation_retries = 0`, so `ValidationOutcome::Retry` was never
    constructed under test and the recursive re-entry was never taken. The
    knob's default was `None`, so on a normal session it governed nothing, and
    `validation_retries` was a parameter of a setting nobody turned on.
  - **If it comes back**, it belongs to a request or a task, not to a session:
    a caller that wants JSON back asks for JSON on that call.

### Conversation & Sessions

- [x] **Interactive Chat** `P0` — conversational AI with streaming text, thinking, tool calls, and subagent events · [[Help/CLI/chat]] · `crucible-cli`, `crucible-daemon`
  - **Gets you:** the TUI renders streaming assistant text, graduated thinking blocks, tool-call rows and subagent rows as a turn progresses.
- [x] **Agent Cards** `P0` — configurable agent personas with system prompts, model, tool policy, mode and MCP servers · [[Help/Extending/Agent Cards]] · [[Help/Config/agents]] · `crucible-core` (config), `crucible-daemon`
  - **Gets you:** a discovered card layers its settings over the config defaults on the resulting session's agent.
- [x] **Agent Card Discovery & Model Resolution** `P0` — cards discovered from `~/.config/crucible/agents/`, kiln `agents/`, and project `.crucible/agents/` (later shadows earlier); model resolves card-explicit > `specialty:` through a `[llm.models]` config table > inherited · `crucible-daemon`
  - **Gets you:** you drop a card file in any of three places and the nearest one wins; a card can name a *specialty* instead of a model and the `[llm.models]` table maps it. Only `description` is required. `delegate_session` resolves targets card-first, then ACP profiles.
- [x] **Agent Card Selection from Chat and Web** `P0` — start a session on a named agent card · `crucible-cli`, `crucible-web`
  - **Gets you:** `cru chat --card <name>` starts interactive or one-shot chat with daemon-owned card resolution. `cru session create --agent <name>` keeps its card meaning; chat's `--agent` remains the ACP alias. The web composers offer no card field: a card names a subagent or an `@` callout in a message, not a draft, so `cru session create --agent` and the CLI are where a session starts on a card. Clients forward the name and never overwrite the composed agent with their own defaults. Unknown cards create nothing; `--card` cannot replace the agent of a resumed session.
- [x] **Session Persistence** `P0` — conversations saved as append-only JSONL in the kiln; markdown rendered on demand · [[Help/Core/Sessions]] · `crucible-daemon` (observe)
  - **Gets you:** every session leaves a `session.jsonl` on disk that reloads across daemon restarts, and `session.render_markdown` / `cru session show` render it as markdown when you ask. No markdown file is written eagerly. The log does not read the client bus: each emitted event also goes to an ordered queue that drops nothing, so a burst that overruns the bus loses no line and keeps the order (`server::tests::session_journal::a_burst_past_the_broadcast_ring_is_stored_whole_and_in_order`). A history read (`session.events_after`, `session.load_events`, resume, the conversation tree) first waits until the log holds each event already published, so a client that saw an event finds it (`a_history_read_after_an_event_sees_the_event`).
- [x] **Session Resume** `P0` — load and continue previous sessions with full history · [[Help/Core/Sessions]] · `crucible-daemon` (rpc)
  - **Gets you:** a previously-ended session reloads with its prior events and accepts new turns appended to the same log.
- [x] **Sessions Are Always Resumable** `P0` — lifecycle state never blocks continuing a conversation · `crucible-daemon`
  - **Gets you:** sending to an ended or evicted session transparently revives it — resident if it is, resumed from storage otherwise, with the kiln resolved via a `session_kilns` index. The session list is global rather than implicitly kiln-scoped, and the live/idle/ended axis is gone from the sessions surface.
- [x] **Session Hygiene — Auto-Titles and Auto-Archive** `P0` — plugin-generated titles, daemon-side title sweep and stale-session archiving · `crucible-daemon`, `crucible-web`
  - **Gets you:** an untitled session with content gets a topic-derived title without you doing anything, and stale stored sessions auto-archive (and unarchive) while keeping their files. Web session lists are recency-sorted with an archived section. The title itself comes from the bundled `auto-title` plugin — it owns the prompt, the clip and the sanitizer, and any plugin publishing `session_title` replaces it. The daemon decides *when* a session is titled, holds the single-flight guard, and truncates the first user message when nothing answers.
- [x] **Segmented Turn Convergence** `P0` — a persisted `segment_complete` event at each text→tool boundary; backend-canonical message ids · `crucible-daemon`, `crucible-web`
  - **Gets you:** a live viewer, a second pane, and a reload all render byte-identical transcripts — including turns where the agent narrates between tool calls, which used to render the narration twice.
- [x] **Session-Unique Scratch Workspaces** `P0` — a session created with no explicit workspace gets a private dir at `<[workspace] session_scratch_dir>/<session_id>` (default `~/.crucible/workspaces`) · `crucible-daemon`
  - **Gets you:** a kiln-less session has a real filesystem containment boundary of its own instead of silently falling back to the kiln. These scratch dirs carry no `.crucible` config, which is what stops a confidential kiln being downgraded to Public at delegation time. The folder is where that session's work goes, so the web file tree, `fs.write`, grep and the file editor admit it as a root the way they admit a registered project — matched by shape (`<scratch_dir>/<id>`) and by the session's own record, never a stranger folder under the same base.
- [x] **Conversation History** `P0` — clear history (`:clear`), resume with prior messages; TUI viewport hydrated from daemon session events · `crucible-cli`, `crucible-daemon`
  - **Gets you:** `:clear` empties the viewport and clears daemon-side history; on resume the viewport is repopulated from replayed session events.
- [-] **Message Queueing** `P0` — type and queue messages during streaming; Ctrl+Enter force-sends · `crucible-cli`
  - **Gets you:** no queue. Enter during a turn shows a toast ("Turn in progress — Esc cancels, then Enter to send") and keeps your draft; Ctrl+Enter **cancels the stream** and keeps the draft rather than sending it.

### Agent Runtime

- [x] **Internal Agent** `P0` — built-in agent with session memory and tool access · [[Help/Extending/Internal Agent]] · `crucible-daemon`, `crucible-core`
  - **Gets you:** the built-in agent runs turns through the real scheduler, dispatches tools, and its output text reflects the tool result.
- [x] **Multiple LLM Providers** `P0` — unified interface across 7 chat backends (Ollama, OpenAI, Anthropic, Cohere, OpenRouter, GitHubCopilot, ZAI) plus FastEmbed for embeddings; VertexAI has no chat adapter (`backend_to_adapter` maps it to `None` and its `supports_chat` is false) · [[Help/Config/llm]] · `crucible-daemon`, `crucible-core`
  - **Gets you:** a session's provider/model resolve to a genai client and the turn streams from that backend.
- [x] **Model Switching** `P0` — runtime `:model <name>` with autocomplete · `crucible-daemon`, `crucible-cli`
  - **Gets you:** `:model` opens a completion popup listing daemon-supplied models, and selecting one re-resolves the session agent — a cross-provider switch invalidates the handle cache.
- [x] **Extended Thinking** `P0` — Ctrl+T toggles the display; Crucible sets no cap on how much a model reasons · `crucible-daemon`, `crucible-cli`
  - **Gets you:** the display toggle changes whether thinking renders expanded or collapsed. There is no reasoning-token budget: the budget knob was deleted because a cap truncates a model mid-thought, and the provider default is what we want.
- [x] **System Prompt** `P0` — layered prompt composition at session creation · `crucible-daemon`, `crucible-core` (config)
  - **Gets you:** the prompt the provider receives is `Workspace:` / `Kiln:` / knowledge-base names / the card-or-config base prompt / the skills catalog, composed in that order, plus a deferral note when tools are deferred.
- [-] **Environment Overrides** `P0` — `--env KEY=VALUE` for per-session env vars · `crucible-cli`
  - **Gets you:** env on a spawned **ACP subprocess** only. On the default internal agent the flag is parsed, logged, threaded through `AgentInitParams` — and then dropped. On a *resumed* session the agent is not reconfigured at all, so `--env` is dropped for ACP too.
- [x] **Agent Cancellation** `P0` — Ctrl+C/Esc cancels the local stream and propagates to the daemon via `session.cancel` · `crucible-daemon`, `crucible-cli`
  - **Gets you:** Esc or Ctrl+C during a turn stops the local stream and fires `session.cancel`; ACP agents additionally receive `session/cancel`.
- [x] **Error Handling UX** `P0` — toast notifications, contextual messages, graceful degradation for DB lock / search / kiln fallback, retryability classification, and transparent retry for idempotent daemon RPCs · `crucible-cli`, `crucible-core`, `crucible-daemon` (rpc)
  - **Gets you:** toasts render and expire in the TUI, and a timed-out idempotent RPC retries transparently instead of surfacing an error.
- [ ] **Provider Error Classification** `P1` — `ChatError::RateLimited { retry_after }` plus `is_retryable`, matching the shape already on `EmbeddingError` and `StorageError` so the tree has one pattern rather than two. Today `ChatError` has no retryability axis and `genai_handle.rs` contains no retry at all; the retry that exists is RPC transport between CLI and daemon · `crucible-core`, `crucible-daemon` (provider)
- [ ] **Provider Fallback Chains** `P1` — `[llm] fallback = [...]` moves a turn to the next configured provider on a retryable failure, resolved at `agent_factory.rs` where every other generation setting already arrives. Fail-forward only: a turn that has already streamed tokens is never silently re-issued against a different model. Matters because the daemon runs unattended work — delegated children, the reflection pass, scheduled jobs — where a 429 fails the turn with nobody watching. Multi-key credential pooling is deliberately out of scope pending a terms-of-service decision · `crucible-daemon` (provider, agent_factory), `crucible-core`
- [ ] **Global Estop** `P1` — a daemon-wide, resumable "stop starting new work" sentinel under `data_home`, checked at session admission (`session_lifecycle.rs:87`), delegation spawn (`delegation.rs:340`) and the scheduled-job tick. Pauses new work and never kills a turn in flight; fail-safe, so an empty or unreadable sentinel counts as engaged; surfaced in `cru status`, the TUI statusline and the web session list, because a silent daemon that accepts nothing looks broken. Distinct from `session.pause`, which is per-session · `crucible-daemon`

### Tools & Permissions

- [x] **Tool Calls** `P0` — inline tool execution with streaming results; batched calls correlated by `call_id` · `crucible-daemon` (tools), `crucible-core`
  - **Gets you:** each tool call streams a `tool_call` then a `tool_result` keyed by `call_id`, and the TUI renders one row per `call_id` with the result landing in it. A batch of provider-emitted parallel calls is correctly correlated — but the stream loop dispatches them **sequentially**, one `.await` per call.
- [x] **Permission System** `P0` — an ordered layer stack decides allow / deny / prompt · [[Help/Concepts/Permission Precedence]] · `crucible-daemon`
  - **Gets you:** a tool call is allowed, denied with an agent-visible error, or prompted, and the decision changes the turn's output text. The real order is: `is_safe()` gate-entry check → `--permissions` CLI override → global `[permissions]` engine (deny absolute, allow short-circuits) → saved `PatternStore` patterns → Lua `on_request` hooks → mode rules then mode default stance (deliberately *after* hooks, so a user hook beats `cru.modes.auto`) → non-interactive immediate deny → user prompt with a 300 s deny timeout.
- [x] **Pattern Whitelisting** `P0` — "always allow" saves project-scoped patterns for future sessions · `crucible-daemon`
  - **Gets you:** choosing "always allow" writes a pattern into the project's `PatternStore` on disk, and a later call matching it skips the prompt entirely.
- [x] **Permission Hooks (Lua)** `P0` — custom Lua hooks can Allow/Deny/Prompt, with a 1 s budget · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a `cru.permissions.on_request` hook returning `{deny=true}` blocks the tool with an agent-visible error. The 1 s figure is a budget, not a timeout: hooks run synchronously and elapsed time is checked *after* they return, so a hook that blocks for 60 s blocks the turn for 60 s and its answer is then discarded. **Your hook is reached because the shipped ones decline** — the gate is first-match-wins in registration order and the defaults load before your file, so a shipped hook that decided would be final. `defaults/init.luau` answers `nil` for every mode but `plan`; in `plan` mode its deny stands.
- [x] **Permission Prompt Serialization** `P0` — prompts open one at a time in arrival order, with a 300 s deny timeout · `crucible-daemon`, `crucible-cli`
  - **Gets you:** a parallel ACP tool batch produces permission prompts **one at a time** rather than all at once, and an unanswered prompt denies after 300 s instead of wedging the turn. The documented tradeoff: a walked-away-from prompt blocks the queue for the full 300 s.
- [x] **Diff Synthesis in Permission Prompts** `P0` — a write/edit prompt shows the diff the tool would apply · `crucible-daemon`
  - **Gets you:** you approve a change by looking at the change, not at a tool name and an argument blob.
- [x] **Interaction System** `P0` — `InteractionRequest` carries permission requests from the agent to whichever client is attached · `crucible-core`, `crucible-daemon`
  - **Gets you:** a gated tool call opens a permission modal showing the command and any synthesized diff; `y`/`n` resolve it and the turn continues accordingly.
- [-] **Agent-Initiated Questions** `P0` — the agent asks the user a question (single-select, multi-select, free-text) mid-turn · `crucible-core`, `crucible-daemon`
  - **Gets you:** nothing. The modal renders one if handed one, but nothing in the agent path ever hands it one.
- [x] **Delegation** `P1` — `delegate_session` spawns a child agent reusing the ordinary session/task primitives · [[Help/Concepts/Delegation]] · [[Help/Delegation Patterns]] · `crucible-daemon`
  - **Gets you:** a real parent-linked child session; `subagent_spawned`/`completed`/`failed` events stream to the parent; the child's output comes back; and depth, allowlist, self-delegation, concurrency, timeout and data-classification trust are each enforced with agent-visible errors. Supervisor/router/broadcast are Lua recipes over `cru.session.*`, not built-ins.
- [x] **Hidden Child Sessions** `P1` — delegated children are real sessions, excluded from `session.list` unless asked for · `crucible-daemon`
  - **Gets you:** your session list is not polluted by every subagent, but `cru session list --include-children` shows them; they link via `parent_session_id` and are ended, archived, deleted and cancelled together with their parent.
- [x] **Background Bash Jobs** `P0` — `list_jobs`, `get_job_result`, `cancel_job` over a `BackgroundJobManager` · `crucible-daemon` (tools)
  - **Gets you:** an agent (internal or external over MCP) starts long shell work in the background, lists what is running, fetches a result, and cancels one. This is what the old "Subagent Spawning" entry actually described — the manager spawns bash only; delegated children are scheduler-driven sessions (see **Delegation**).
- [x] **Repeat-Failure Tool Blocking** `P0` — a tool that keeps failing within a stream is blocked for the rest of it · `crucible-daemon`
  - **Gets you:** the agent stops looping on a broken tool — further calls return "Tool 'X' is blocked for this stream after repeated failures." instead of executing.
- [x] **Security Enforcement** `P0` — permissions config, shell policy, filesystem containment, derived delegation trust · `crucible-daemon`, `crucible-core`
  - **Gets you:** `[permissions]` config is enforced for internal agents (not just ACP) and config `deny` beats an agent card's `allow`. `[security.shell]` policy applies to `bash`, checked per chained statement. File tools are contained to a default-deny allowlist of workspace + kilns + session dir with symlink, `..` and glob escapes blocked — and the note, search and kiln tools answer to the same allowlist as `read_file`, because every family reaches a path through one capability handle rather than its own check. Delegation trust derives from the target's actual provider, so a local-model card can serve a confidential kiln while cloud targets stay blocked. Non-interactive sessions deny would-prompt tools immediately instead of hanging.
- [ ] **Prompt-Injection Scanning** `P1` — a pure scanner in `crucible-core` over content that reaches the system prompt: rules files (`AGENTS.md`, `.rules`, `.github/copilot-instructions.md`) and `SKILL.md` bodies. Covers zero-width/bidi controls (reusing `text::is_display_hostile`), imperative text hidden in HTML comments, credential-file reads, and curl-exfiltration shapes. Findings become a visible `[FLAGGED: …]` marker rather than silence; local content annotates and fetched content blocks, never the reverse. Closes the gap that `skills/discovery.rs:244` already closed for cross-harness *home* directories but not for a cloned repository's workspace skill directories · `crucible-core`, `crucible-daemon` (rules_files, skills)
- [ ] **Verification Evidence Ledger** `P2` — a second record type in the session's append-only `review.jsonl` classifying bracketed `bash` calls as verification evidence (canonical command, exit code, changed paths covered), so a session can answer "what did this agent prove?" and not only "what did it change". Distinct from **Output Validation**, which checks response text rather than evidence of work. Coverage is asymmetric by construction: an ACP agent's commands are recorded from the pass-through arm with no `paths_covered`, because `stream.rs:464` never brackets them · `crucible-daemon` (review), `crucible-core`
- [ ] **Verify-on-Stop Nudge** `P2` — a turn-end prompt replay, beside the existing depth-cap replay, when a turn changed code and produced no verification evidence; suppressed for doc-only edits and off by default for one release · `crucible-daemon` (agent_manager)
- [x] **MCP Tool System** `P0` — `PermissionGate` trait, ACP integration, gateway tool definitions injected per session · `crucible-daemon` (tools, acp)
  - **Gets you:** unsafe tool calls prompt or are denied, and the decision the user makes is what executes.
- [x] **Per-session MCP Servers** `P0` — agent cards name MCP servers; `mcp_servers` propagates to `SessionAgent` and filters the gateway tool set · `crucible-daemon`
  - **Gets you:** an agent card naming server `gh` gets `gh`'s gateway tools and no others.
- [ ] **LSP Post-Write Diagnostics** `P3` (deferred) — run language servers as supervised subprocesses and pipe `publishDiagnostics` into a lint-delta report after `edit_file` / `write_file`, gated on git-workspace detection. Placement decided: `crucible-daemon/src/lsp/`, not a Lua plugin — `cru.shell` writes a child's stdin once and closes it (`shell.rs:295-313`) and `cru.service` supervises Lua functions, not live pipes, so a plugin cannot host a stateful JSON-RPC server. This does not contradict the "execution backends stay plugins" ruling: an LSP does not change where a tool runs. Deferred because the subprocess supervisor is the dominant cost and **Verification Evidence Ledger** captures the project's own checker — more authoritative, no supervisor — for a fraction of it. Diagnostics *policy* stays in config plus `post_tool_call` · `crucible-daemon` (lsp)

### Diffs & Proposals

- [x] **`cru diff`** `P1` — `cru diff branch` shows a branch's changes against its merge base; `cru diff comments` prints the open comments of a diffset · [[Help/CLI/diff]] · `crucible-daemon`, `crucible-cli`
  - **Gets you:** `cru diff branch [--base REF] [--head REF] [--root PATH] [--stat] [-f text|json]` lists each added, modified, deleted and renamed file, colored and side-by-side on a wide terminal, plain unified text in a pipe; a binary file or one over 1 MiB shows a line saying so instead of its text. `cru diff comments <diffset>` takes `session-<id>`, `proposal-<uuid>`, `branch` or `branch-<hex>` and prints one quickfix entry per open comment (`path:line: [start-end] text`), for `vim -q`, or the full record with the `outdated` flag as JSON. The TUI renders the same branch diff full-screen with `:diff [base]`.
- [x] **`cru proposal`** `P1` — list, show, accept, reject, dismiss and resolve the proposals a `propose`-mode write or pass leaves waiting for a decision · [[Help/CLI/proposal]] · `crucible-daemon`, `crucible-cli`
  - **Gets you:** `cru proposal list [--all]` lists the open, stale and conflicted proposals, or every stored one including the decided ones with `--all`; `show <id>` prints its title, author, state and the diff of each file, or with `--conflict <path>` only the conflict-marked text of one conflicted file; `accept <id>` writes every file, merging against a file that changed on disk since and, on a real conflict, writing nothing and turning the proposal `conflicted`; `reject <id> [--reason]` and `dismiss <id>` change no file; `resolve <id> <path> --from <file|->` gives the settled text of one conflicted file, and the daemon writes the whole proposal once every conflicted file has a settled text.

### Tool Discovery & Disclosure

> Agents shouldn't carry every tool schema in context. Discovery tools let an agent find tools on demand; progressive disclosure makes that automatic when the tool set is large.

- [x] **Tool Discovery** `P1` — `discover_tools` and `get_tool_schema` let an agent enumerate and inspect tools at runtime · `crucible-daemon` (tools)
  - **Gets you:** `discover_tools("glob")` returns a result naming `glob`; `get_tool_schema("glob")` returns its `pattern` parameter; an unknown name errors.
- [x] **Progressive Tool Disclosure** `P2` — automatic deferral when mode-filtered tool schemas exceed 15% of the effective context budget · `crucible-daemon` (tools)
  - **Gets you:** deferrable (gateway/user MCP) tools drop out of the request and are replaced by the `discover_tools` → `get_tool_schema` → `invoke_tool` bridge, with a deferral note added to the system prompt. Kiln and workspace tools are never deferred; `invoke_tool` is unwrapped to the inner tool before hooks and permissions, and plan mode cannot be escaped through it. An explicit active set (`cru.tools.set_active`) is applied *before* the budget check and does not override it: narrowing usually removes the reason to defer, and a narrowed set that is still too large is deferred like any other.
- [ ] **Tiered Tool Disclosure** `P3` — intermediate deferral tiers (names-only, then per-server counts) between "all schemas attached" and the current all-or-nothing drop at 15% of budget (`genai_handle.rs:30`). Only bites with very large MCP catalogs; the single threshold is the simpler correct thing below that · `crucible-daemon` (provider)

### Agent Skills

> Skills are markdown capability docs ([agentskills.io](https://agentskills.io)-compatible `SKILL.md` + optional `scripts/`, `references/`) that teach the agent procedures on demand. Discovery, parsing and daemon-side context injection all ship.

- [x] **Skill Discovery** `P1` — folder discovery across search paths, `SKILL.md` frontmatter parsing, scope precedence, `cru skills` CLI · [[Help/Concepts/Agent Skills]] · [[Help/CLI/skills]] · `crucible-daemon` (skills), `crucible-cli`
  - **Gets you:** skills under the personal / workspace / kiln search paths are found, parsed, shadowed by scope precedence, and listed by `cru skills list|show|search`. A symlinked `SKILL.md` is rejected and files are capped at 256 KB. Cross-harness discovery (`~/.claude/skills` and friends) is opt-in behind `CRUCIBLE_CROSS_HARNESS_SKILLS`.
- [x] **Bundled Help Skills** `P1` — help skills shipped at `runtime/crucible-help/skills` · `crucible-daemon` (skills)
  - **Gets you:** `cru skills list` shows the help skills however you got Crucible — a dev tree, `$CRUCIBLE_RUNTIME`, an installed `<prefix>/share/crucible/runtime/` layout, or none of those, in which case the daemon extracts the tree it carries inside the binary.
- [x] **Skill Context Injection** `P1` — the tier-1 skills catalog is rendered into the daemon's enriched system prompt; full `SKILL.md` loads on demand via `skill_view` · `crucible-daemon` (skills)
  - **Gets you:** the agent sees a name+description catalog in its system prompt and can pull a skill's full body when it decides to (list → view → use). It is injected **only when the session has a kiln**, because `skill_view` is kiln-scoped — so a session in a plain project dir advertises no skills at all.
- [~] **Skill Self-Creation, retired 2026-09-14** — the reviewer's `skill` kind and the old `cru proposals accept` path that turned it into a `SKILL.md` are both gone · [[Help/Concepts/Reflection Pass]] · `runtime/plugins/reflection`
  - **Why:** the whole feature stood on the staged proposal file. A pass now writes with `create_note` and `update_note`, and a note is not a skill. The proposals of 2026-09-21 hold note writes only, so nothing in the tree writes an agent-authored `SKILL.md` or the `crucible-source: reflection` provenance that told one apart from a user-authored one. Rebuilding it needs a skill-writing tool the pass can call; no such tool exists.

### Context & Knowledge

- [x] **File Attachment** `P0` — `@file` context attachment in chat, resolved daemon-side so every client gets it · `crucible-cli`, `crucible-daemon`
  - **Gets you:** `@`-picking (or typing) a workspace file puts its contents in front of the agent as a tagged system block for that turn, so the agent does not have to go read it. The path is relative to the workspace or to an attached kiln, and the session's containment decides which files it can read. `@a.rs:12` or `@a.rs:12-14` attaches only those lines. Both completers insert the root-relative path and keep a line suffix. Deduped, truncated past 64KB per file, and `user@example.com` is not a file.
- [x] **Rules Files** `P0` — project-level AI instructions (`AGENTS.md`, `.rules`, `.github/copilot-instructions.md` by default; `[context] rules_files` to change the set) loaded into the system prompt, hierarchically from the repo root down to the workspace · [[Help/Rules Files]] · `crucible-core` (config), `crucible-daemon`
  - **Gets you:** instructions in your project's `AGENTS.md` are in the agent's system prompt under `# Project rules`, after the agent card's own prompt, with a rules file nearer the workspace read later and so winning.
- [x] **Multi-Kiln Sessions** `P0` — extra knowledge kilns attach at creation or mid-session · `crucible-daemon`, `crucible-web`
  - **Gets you:** `session.connect_kiln` / `disconnect_kiln` change a live session's knowledge scope, and the kiln is optional everywhere — a kiln-less session resolves the home-kiln default daemon-side. Attaching re-runs data-classification trust checks *before* opening the kiln, so a rejected attach leaves no trace. The workspace is not part of the live scope: a session keeps the project it was created in, and `session.set_workspace` refuses every change with `AgentError::WorkspaceFixed` (2026-09-15).

### Core Agent Features

> Core capabilities implemented in Rust rather than as plugins, with hook points where Lua can override.

- [x] **Auto-Linking** `P1` — `suggest_links` detects unlinked mentions of existing notes via word-boundary matching · `crucible-daemon`, `crucible-web`
  - **Gets you:** the web backlinks panel's "unlinked mentions" list, where clicking **Link** rewrites the open editor buffer. Case-insensitive, skips already-linked targets. This is web-only — there is no TUI or CLI entry point and the internal agent has no autolink tool.

### Lua Session API

- [x] **Scripted Agent Control** `P0` — Lua control of `mode`, `model` and `system_prompt` from the TUI's session VM · `crucible-lua`, `crucible-cli`
  - **Gets you:** setting the mode changes which tools the agent can see — including on a cached live handle. `session.model = "x"` calls `switch_model` on the bound backing; in an `on_session_start` hook it picks the model the agent starts with (`on_session_start_can_pick_the_model`). Daemon getters read a local cache.
- [-] **Scripted Agent Control (daemon plugin VM)** `P0` — the same `cru.get_session()` surface from a daemon-side plugin · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a visible error. Setters now raise `"<field>: not supported on this session"` instead of reporting success and changing nothing; getters still return defaults, so `s.mode` reads `"chat"` — a mode id not in the registry.
- [x] **Session Event Handlers** `P0` — Lua hooks on `turn:complete` can inject follow-up messages · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a handler returning `{ inject = { content = "..." } }` causes the agent to run another turn with that content as the message — the user sees a second streamed response.

### Lua Session & Tool Primitives

> These fill gaps so autonomous loops, fan-out, and context control are trivial plugins — not bespoke features.

- [x] **`cru.ui.*` — Asking the User** `P1` — a Lua plugin constructs any of the 7 `InteractionRequest` variants and awaits the answer · `crucible-lua`, `crucible-daemon`, `crucible-web`
  - **Gets you:** `cru.ui.{ask, ask_batch, edit, show, permission, popup, panel}(session_id, opts)` opens a real modal in whichever client is attached — TUI or browser — and returns the response. Before this a plugin could not ask the user anything: `cru.oil.*` built nodes nothing consumed, and `crucible-lua` never constructed an `InteractionRequest`. `{ kind = "cancelled" }` is a successful call nobody answered, which on a headless daemon is the common case. Deliberately not serialized behind permission prompts — two plugins asking unrelated questions must not block each other.
- [x] **`cru.tools.call(name, args)`** `P1` — programmatic tool calling from Lua · `crucible-lua`, `crucible-daemon` (tools)
  - **Gets you:** a workspace tool executes and returns its output to Lua, subject to the operator's `[permissions]` rules. A `deny` is absolute; an `allow` runs; anything the rules leave at `ask` falls back to the read-only exemption, so a plugin can `read_file`/`grep` unconfigured but needs an explicit `allow` for `bash` or `write_file` — there is no prompt to fall back on from a Lua call.
- [x] **`cru.tools.batch({...})`** `P1` — concurrent multi-tool calls · `crucible-lua`, `crucible-daemon` (tools)
  - **Gets you:** N tools execute concurrently from one Lua call and per-entry `{result=…}` / `{err=…}` come back, with error isolation between entries.
- [x] **`cru.tools.list()`** `P1` — enumerate workspace tool definitions from Lua · `crucible-lua`, `crucible-daemon`
  - **Gets you:** name, description and parameters for every workspace tool, so a plugin can decide what to call.
- [x] **`cru.tools.set_active(id, names)` / `get_active(id)`** `P1` — a plugin narrows which tools one session offers · `crucible-lua`, `crucible-daemon` (provider, tools)
  - **Gets you:** glob patterns in the same language a mode's `tools` selector speaks, intersected with the set `visible_tools()` already computed. It **only narrows**: `set_active` cannot re-add what plan mode or a declared mode removed. Applied before progressive disclosure decides what to defer, so narrowing usually takes the session back under the 15% budget share and nothing defers — and when the narrowed set is still over budget, deferral runs as usual, because a deferred tool stays callable through the bridge. Enforced at dispatch as well as in the advertisement; the three disclosure bridge tools are never hidden. `nil` clears the set, `{}` is a set naming nothing, and a map or sparse table is an error rather than either. `get_active` answers `(nil, nil)` when no set is in force.
  - **Limits, all three of which the call itself reports or the docs state:** `set_active` errors on an id no live session has (nothing would read that entry and nothing would ever clear it) and on a session delegated to an external ACP agent (Crucible does not assemble that agent's tool list, so narrowing the MCP half would be a control in name only). `discover_tools`/`get_tool_schema` still enumerate excluded tools — the set governs what runs, not what can be found. Sets are in-memory: a daemon restart drops them and a resumed session comes back automatic.
- [x] **`cru.session.messages(id, opts)`** `P1` — read conversation history from Lua; opts `{role, limit, tools}` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** the session's real `{role, content, timestamp}` history, role-filtered and limited — enabling context windowing, summarization, checkpoint detection. With `tools = true` the list also carries `tool_call` rows (`id`, `name`, `args`) and `tool_result` rows (`id`, `content`, and `error` only when the tool failed), in transcript order; a `role` filter still excludes them.
- [x] **`cru.session.inject(id, role, content)`** `P1` — queue persisted context for an internal agent's next turn · `crucible-lua`, `crucible-daemon`
  - **Gets you:** system, user or assistant context accepted immediately, then inserted once at the next turn's assembly boundary. It does not modify an in-flight response or start a turn. User-role context does not count as a user turn for undo or initial retrieval, including after resume and fork. Acceptance is persisted with the preceding turn's message id, so delayed transcript writes cannot reorder it on resume. ACP sessions are refused: an external agent owns its history, and the current ACP adapter cannot deliver all three roles once. Use `cru.context.attach` for this-turn context. This is a plugin/RPC primitive, not a new TUI or web control; review rejection already calls it from both clients.
- [x] **`session.fork()`** `P1` — `cru.session.fork(id, opts)` for parallel exploration and A/B testing · `crucible-lua`, `crucible-daemon`
  - **Gets you:** one daemon implementation for Lua and RPC. A fork inherits the parent's agent configuration, workspace, attached kilns, isolation and session variables, with a fresh session identity. `up_to` counts persisted user/assistant/system messages, including injected context; injections retain their role but do not create undo turns. An unconfigured parent produces an unconfigured fork. Forks are independent sessions, not lifecycle-subordinate delegated children; they copy persisted history, not an in-flight provider's private state. This shares copying, not lifecycle dispatch: RPC runs required start hooks; Lua requires explicit `isolation = false` for workspace parents and refuses any active isolation claim or positive isolation request and directs the caller to RPC, rather than creating an unclaimed runnable fork from inside a plugin callback.
- [x] **`cru.session.collect_subagents(ids, timeout?)`** `P1` — await multiple subagents with an optional timeout · `crucible-lua`, `crucible-daemon`
  - **Gets you:** delegated child sessions and background bash jobs collected together, in input order, including duplicate ids. Completion wakes all collectors without polling. Completed, failed, cancelled, unknown and still-running-at-timeout results remain distinct; timing out does not cancel work. Zero inspects immediately, and invalid Lua timeouts fail before starting work.
- [x] **`cru.session.subscribe` / `unsubscribe`** `P1` — stream a session's events live from a Lua plugin · `crucible-lua`, `crucible-daemon`
  - **Gets you:** session-filtered real daemon events in Lua iterators. Unsubscribe closes this bridge's iterators for that session, is idempotent, and allows resubscription; other sessions remain subscribed. Already-buffered events may drain. Dropping an idle iterator releases its broadcast receiver. `send_and_collect` observes a submitted turn; timeout or iterator drop stops observation, not the turn.

## Terminal Interface (TUI)

### Modes & Input

- [x] **Chat Modes** `P0` — Lua-declared modes; `ask` / `plan` / `auto` ship as defaults. Badge, cycling and per-mode slash command are all derived from the daemon's list · [[Help/TUI/Modes]] · `crucible-cli`, `crucible-daemon`
  - **Gets you:** the statusline renders a coloured badge for the session's mode — including a mode the TUI has never heard of, because `session.list_modes` **replaces** the built-in defaults wholesale. Shift+Tab advances through whatever the daemon offers, wrapping. Every declared mode gets its own slash command for free, so a user-declared `review` gets `/review`. A mode change made in another client (web, second TUI) updates this client's badge. Plan mode offers only read-shaped tools and denies a mutating tool that reaches the gate, enforced in three layers, one of which is unconditional Rust.
- [-] **Auto Mode Approval** `P0` — in `auto` mode a tool runs without a permission modal · `crucible-daemon`
  - **Gets you:** unproven. The mechanism is implemented as data (the `auto` mode's stance is `Allow` in `runtime/defaults/init.lua`), which is a real improvement — auto mode was previously not implemented at all — but nothing watches the effect.
- [x] **Input Modes** `P0` — Normal (`>`), Command (`:`), Shell (`!`) input · [[Help/TUI/Commands]] · `crucible-cli`
  - **Gets you:** the input prompt glyph and background change between the three modes, and the prefix is stripped from the displayed text.
- [-] **Slash Commands** `P0` — `/mode`, `/default`, `/undo`, `/help`, one command per declared mode, plugin-registry commands, then forward-to-agent · `crucible-cli`
  - **Gets you:** local dispatch for the built-ins and for each declared mode, plugin commands routed through the registry, and anything else forwarded to the agent. Two corrections to the old text: **`/quit` does not exist** — there is no arm for it, so it is forwarded to the agent as a chat message (only `:quit`/`:q` quits) — and `/plan` `/auto` `/normal` are not fixed commands, they exist only because the daemon's mode list seeds them, so a Lua config omitting `plan` deletes `/plan`.
- [-] **REPL Commands** `P0` — `:quit`, `:help`, `:clear`, `:model`, `:set`, `:export`, `:messages`, `:mcp`, `:config`, `:palette`, plus `:lua`/`:=`, `:pick`, `:plugins`, `:reload`, `:undo` · [[Help/TUI/Commands]] · `crucible-cli`
  - **Gets you:** all of them dispatch — but only `:mcp` is proven to put anything on screen.
- [-] **Runtime Config (`:set`)** `P0` — vim-style `:set` with enable/disable/toggle/reset/query/history · [[Help/TUI/Commands]] · `crucible-cli`
  - **Gets you:** parsing and mutation of the config overlay, with more suffix forms than the entry ever documented: `??` query-history, `?` query, `&` reset, `^` pop-one-layer, `!` toggle, `inv` and `no` prefixes, bare `:set` for modified-only, `:set all`. **The `<` suffix this entry used to claim is not implemented** — a trailing `<` parses as `Enable { key: "foo<" }`. The `^` form is vim's `<`.
- [-] **Double Ctrl+C Quit** `P0` — first clears input or shows a warning; second within 300 ms quits · `crucible-cli`
  - **Gets you:** non-empty input is cleared first and two presses quit. The 300 ms window and the warning toast are both unproven.
- [x] **Undo of Agent Edits** `P0` — `/undo [N]` and `:undo [N]` from the TUI · `crucible-cli`
  - **Gets you:** the last N agent file edits revert, with a toast reporting turns and messages removed.

### Streaming & Display

- [-] **Streaming Display** `P0` — real-time token streaming with cancel (Esc/Ctrl+C) · `crucible-cli`
  - **Gets you:** tokens appear in the terminal as they stream, and a cancelled stream graduates cleanly with its partial text. The cancel *keys* are the unproven part.
- [x] **Streaming Graduation** `P0` — drain-based: completed containers render through Taffy and write to stdout (terminal scrollback); the viewport shows only live content · `crucible-cli`
  - **Gets you:** finished turns leave the viewport and land in real terminal scrollback, collapsed (thinking becomes `◇ Thought (~N tokens)`), spinner-free, and byte-identical to what the viewport rendered.
- [x] **Thinking Display** `P0` — streaming thinking blocks with a token estimate · `crucible-cli`
  - **Gets you:** thinking streams live as `Thinking…` and graduates to a collapsed `◇ Thought (~N tokens)`. The count is a ~4-chars/token **estimate** over the accumulated text — no provider reports per-thinking-block usage, so the `~` is the honesty.
- [-] **Thinking Toggle** `P0` — Ctrl+T and `:set thinking` show/hide thinking blocks · `crucible-cli`
  - **Gets you:** unproven on both routes.
- [x] **Markdown Rendering** `P0` — full markdown-to-node rendering with styled output · `crucible-cli`, `crucible-oil`
  - **Gets you:** bold/italic ANSI, bullets, blockquote bars, box-drawn tables, and syntax-highlighted code in the terminal.
- [-] **Context Usage Display** `P0` — token usage in the statusline, fed from the daemon's `message_complete` · `crucible-cli`
  - **Gets you:** the statusline shows `2k tok` / `3% ctx`. Whether the number is the *daemon's* number is not proven, and the source field is `total_tokens` — not prompt+completion as this entry used to claim.
- [-] **Lua UI Config Bridge** `P0` — `ui.config` RPC delivers colorscheme, highlight groups, geometry and statusline bars to every attached client, with diffed `ui_style_changed` pushes and `ui.set_theme` · [[Help/Lua/Configuration]] · [[Help/Extending/Scripted UI]] · `crucible-lua`, `crucible-daemon`, `crucible-cli`
  - **Gets you:** bar layout set from Lua reaches the frame. Colorscheme, highlight groups, geometry, hot reload and theme switching have no test proving any of them change anything on screen.
- [-] **Statusline Item Trees** `P0` — bars are lists of named items (`sl.mode`, `sl.model{}`, `sl.expr("git")`) with combinators (`sl.any`, `sl.when`) and multiple bars in ordered regions · `crucible-cli`
  - **Gets you:** named items, `sl.any`, `sl.when` and multiple bars all genuinely render. A pushed value now provably reaches the frame from the `ui.config` payload onward, and a RELEASED one provably leaves it; what is still unproven is the daemon's half, from `cru.statusline.set` to that payload. **"Closed anchors" no longer exist** — `Anchor` + `order` were replaced by ordered region lists.
- [x] **Terminal Palette Colours** `P1` — `term4` / bare index / `bright_*` address the user's own terminal colours · `crucible-oil`, `crucible-cli`
  - **Gets you:** every spelling reaches the slot it names. Set `blue` in your colorscheme and you get the slot 4 your terminal is configured with; `bright_blue` gets slot 12.

### Tool & Agent Display

- [-] **Tool Call Display** `P0` — per-tool rows with smart summarization and MCP prefix stripping · `crucible-cli`
  - **Gets you:** tool rows render with the `mcp_` prefix stripped and a collapsed result. **There is no spinner on the tool row** — the icon is a static `●`, and that is deliberately pinned by a test asserting the icon does not animate. The animation lives in the turn indicator.
- [x] **Tool Source Badges** `P0` — rows show `[mcp:gmail]` / `[plugin:oci]` · `crucible-cli`
  - **Gets you:** you can see where a tool came from, so an unexpected tool is traceable to its server or plugin.
- [x] **Turn Indicator** `P0` — an animated spinner at the turn level · `crucible-cli`
  - **Gets you:** the one animation in the chat view, showing the agent is working. Giving it its own entry is what lets the tool row stop claiming an animation it does not have.
- [-] **Tool Output Handling** `P0` — truncated tail display, buffer cap, parallel call tracking by `call_id` · `crucible-cli`
  - **Gets you:** a truncated tail with an `(N more lines)` footer. Correcting the numbers: the **display** shows 3 lines (`MAX_TAIL`), the **buffer** caps at 50 (`TOOL_OUTPUT_MAX_TAIL_LINES`, untested), and spill-to-file at >10 KB is a *daemon-side* behaviour — the TUI only suppresses the spill marker.
- [x] **Subagent Display** `P0` — spawned / completed / failed tracking with a truncated prompt preview · `crucible-cli`
  - **Gets you:** each subagent renders as its own row with a status glyph and a truncated prompt, including concurrent ones as separate rows, and delegation shows the target agent.
- [-] **MCP Server Display** `P0` — `:mcp` lists servers with live connection status · `crucible-cli`
  - **Gets you:** `:mcp` renders servers with filled/hollow status dots on a real screen. The runtime-update half is untested *and goes through different code* than the tested one.

### Interaction Modals

- [-] **Permission Modal** `P0` — Allow (y), Deny (n), Allowlist (a); diff toggle (**`h`**, not `d`); queued permissions auto-open · `crucible-cli`
  - **Gets you:** the modal opens on a real screen showing the command and the y/n/a options, deny surfaces "Permission denied", and queued permissions open in arrival order. **The diff toggle key is `h`** — `d` does nothing. (The same `d` error also appeared in the Keybindings entry and is fixed there.)
- [-] **Ask Modal** `P0` — single-select, multi-select (Space), free-text "other" · `crucible-cli`
  - **Gets you:** single-select selection state works. Nothing proves any of it renders, and two of the three named features have no test at all.
- [-] **Diff Preview** `P0` — syntax-highlighted, collapsible, unified and side-by-side **line** diffs · `crucible-cli`
  - **Gets you:** all of that, well-evidenced. **Word-level diffing does not exist** — that adjective is the entire demotion.
- [-] **Permission Session Settings** `P0` — `:set perm.show_diff`, `:set perm.autoconfirm_session` · `crucible-cli`
  - **Gets you:** both wires are complete in code and neither has a single test, at any level.
- [-] **Batch Ask / Edit / Show / Panel** `P0` — all 7 `InteractionRequest` variants have renderers and key handlers · `crucible-cli`
  - **Gets you:** 7/7 renderers and 7/7 key handlers. The old claim "fully implemented with key handlers, renderers, **and tests**" holds for 1 of 7. The browser reached 7/7 on 2026-08-18 — see **Interaction Rendering (web)**.

### Autocomplete & Popups

- [x] **Autocomplete** `P0` — 9 trigger kinds: `@files`, `[[notes]]`, `/commands`, `:repl`, `:model`, `:set`, command args, F1 palette, **`:pick`** · `crucible-cli`
  - **Gets you:** typing a trigger opens a completion popup painted at the right column without covering the line you are typing, and accepting inserts the right thing (`@file`, a closed `[[wikilink]]`, a full `:model` command). The count of 9 was always right; the enumeration used to list only 8 and omit `Pick`.
- [-] **`:set` Value Completion** `P0` — completing the *value* half of `:set option=value` · `crucible-cli`
  - **Gets you:** option *names* complete. Option *values* never do — typing `:set thinking=` shows nothing, and `:set thinking=hi` goes empty rather than falling back.
- [-] **Command Palette** `P0` — F1 toggle · `crucible-cli`
  - **Gets you:** a popup with **four hardcoded entries** — `semantic_search`, `create_note`, `/mode`, `/help` — two of which do nothing when selected. "Full command discovery" is false: it ignores the slash-command registry, every REPL command, and every plugin command.
- [-] **Model Lazy-Fetch** `P0` — model list state machine (NotLoaded → Loading → Loaded) · `crucible-cli`
  - **Gets you:** the state machine and "Loading models…" / "Failed to load models" placeholders exist, but nothing asserts either reaches a frame — and the fetch **is no longer lazy**: models are prefetched at startup, with the lazy path surviving only as a re-fetch after a failure.
- [x] **`:pick` Fuzzy Picker** `P0` — `:pick [notes|files|commands]` opens a fuzzy picker · `crucible-cli`
  - **Gets you:** a picker over notes, workspace files, or the command registry; accepting inserts `@file` / `[[note]]` / the command. It is advertised in the `:repl` completion list and in `:help`, so a user can find it today. `:pick sessions` is a dead branch that returns nothing.
- [x] **`:set completion_style`** `P0` — `auto` | `minimal` | `panel` · `crucible-cli`
  - **Gets you:** switches the completion popup between the nvim-pmenu-style anchored box (default for inline `@`/`[[`) and the classic full-width strip.

### Shell

- [x] **Shell Modal** `P0` — `!command` full-screen execution; scrollable (j/k/u/d/g/G/PgUp/PgDn) · [[Help/TUI/Shell Execution]] · `crucible-cli`
  - **Gets you:** `!cmd` takes over the screen, streams stdout, shows the exit code, and scrolls. `e` (open in `$EDITOR`) and `t` (insert truncated) also exist.
- [x] **Shell Output Insert (`i`)** `P0` — `i` inserts the command's output into the composer · `crucible-cli`
  - **Gets you:** pressing `i` closes the modal and puts the command's output in the composer, fenced and labelled; `t` does the same with the last 20 lines. `q` closes without inserting.

### Notifications

- [x] **Toast Notifications** `P0` — auto-dismiss after 3 s; INFO/WARN badge in the status bar · `crucible-cli`
  - **Gets you:** the newest toast appears in the status bar with count badges beside it, and stops showing after 3 s. **ERROR is unreachable at runtime** — `NotificationKind` has only `Toast | Progress | Warning` and both mapping sites produce Info or Warning, so that level has been dropped from this entry.
- [x] **Messages Drawer** `P0` — `:messages` toggles the full notification history panel · `crucible-cli`
  - **Gets you:** a bordered panel listing the whole notification history with timestamps; any key closes it.
- [x] **Warning Badges** `P0` — persistent count badge when warnings exist · `crucible-cli`
  - **Gets you:** after a toast fades, a persistent ` WARN 2 ` count badge stays in the status bar, surviving narrow widths.

### Rendering Engine

- [x] **Oil Renderer** `P0` — custom terminal rendering engine (replaced ratatui) · [[Help/TUI/Component Architecture]] · `crucible-oil`
  - **Gets you:** the whole TUI is painted by Crucible's own renderer; ratatui is gone.
- [x] **Taffy Layout** `P0` — flexbox-based terminal layout; one spacing system via `gap()` for both graduated and viewport content · `crucible-oil`
  - **Gets you:** consistent spacing between scrollback and viewport, from one mechanism rather than two.
- [x] **Theme System** `P0` — token-based theming · [[Meta/TUI Style Guide]] · `crucible-oil`
  - **Gets you:** theme tokens (mode colour, toast severity colours, syntax theme) come out as real ANSI in the painted frame.
- [-] **Theme Overrides** `P0` — user-supplied colours replace the defaults on screen · `crucible-cli`, `crucible-oil`
  - **Gets you:** overrides are proven to reach *getters*, not the frame.
- [-] **Viewport Caching** `P0` — cached messages, tool calls, shell executions, subagents · `crucible-cli`
  - **Gets you:** messages, tool calls, subagents **and shell executions** are cached and render — run `!cargo build`, close the modal, and the command, its exit code and its output tail are in the transcript. There is still no lazy line-wrapping.
- [x] **Drawer Component** `P0` — bordered expandable panels with title/footer badges · `crucible-cli`
  - **Gets you:** a bordered panel with a title badge and an `ESC/q close` footer, capped at `max_items`.

### Session & Export

- [x] **Session Export** `P0` — `:export <path>` saves the session as markdown · `crucible-cli`, `crucible-daemon` (observe)
  - **Gets you:** a markdown file with YAML frontmatter, collapsible thinking callouts, and tool call/result blocks, with tilde expansion on the path. A missing parent dir and no-active-session both warn rather than failing silently, and export is skipped in replay mode.
- [x] **Session Export to a Path** `P0` — `session.export_to_file` writes the rendered transcript to a caller path under write protection · `crucible-daemon` (observe)
  - **Gets you:** `cru session export` calls this RPC (the TUI `:export` renders the loaded events on the client side and writes the file itself); with no path the file lands beside the session under the daemon data root. The daemon refuses an output path a host process would execute (`.claude/settings.json`, `.crucible/plugins/*.lua`, `.git/config`) and a session directory that resolves outside the sessions root.
- [x] **Keybindings** `P0` — Enter, Esc, Ctrl+C, Ctrl+T, BackTab, F1, y/n/a and **`h`** (diff) in modals, plus a readline set · [[Help/TUI/Keybindings]] · `crucible-cli`
  - **Gets you:** every key named here dispatches, including Ctrl+A/E/W/U/B/F, Alt+B/F and Ctrl+J for a newline. Note the diff toggle is `h`, not `d`. Ctrl+Enter (cancel while preserving the draft) is bound and missing from both this list and `:help keys`.
- [x] **Bottom-Anchored Chat Layout** `P1` — composer and status bar pinned to the bottom; conversation fills the space above · [[Meta/TUI User Stories]] · `crucible-cli`
  - **Gets you:** the input box and status bar are the last rows of every frame, popups render above the input bar, and content graduates upward into scrollback.
- [x] **Input History** `P1` — Up/Down (and Ctrl+P/Ctrl+N) walk back through messages submitted this session · `crucible-cli`
  - **Gets you:** recall of anything you submitted this session, with your in-progress draft restored when you walk past the newest entry. **In-session only — no persistence across restarts**, and it is the generic input buffer, not the shell-history store.
- [ ] **Splash Screen** `P1` — a startup splash for the TUI · [[Meta/TUI User Stories]] · `crucible-cli` — grep for `splash` across `crates/crucible-cli/src` returns zero hits; the 2026-07-22 splash work was the *web* splash. There is also no splash story in the TUI User Stories doc, which the project's own rule makes a prerequisite.
- [ ] **Session Stats** `P1` — per-session token/turn statistics surface · `crucible-cli` — no `:stats` REPL command and no session-stats surface anywhere in the TUI. `cru stats` / `cru storage stats` report storage and index counts, not per-session numbers; the statusline's context usage is the adjacent shipped thing.

## Extensibility & Plugins

- [x] **Luau Scripting** `P0` — Luau runtime for plugins · [[Help/Lua/Language Basics]] · [[Help/Concepts/Scripting Languages]] · `crucible-lua`
  - **Gets you:** a plugin author's Luau evaluates in the daemon and returns values, with `--!strict` type annotations accepted and erased at runtime.
- [x] **Host module resolution** `P0` — one `require`, owned by the host · `crucible-lua`
  - **Gets you:** `require("<plugin>")` resolves over the plugin roots, a plugin's own `lua/` directory is private to it, a cycle raises instead of recursing, and a reload re-reads a changed private module. Luau has no `package.path`, so lookup — which is import authority — is the host's rather than a mutable global any plugin could widen.
- [x] **Luau stdlib compatibility** `P0` — `io` and the file half of `os`, from the host · `crucible-lua`
  - **Gets you:** `io.open`, `os.getenv`, `os.tmpname`, `os.remove`, `io.popen` and `os.execute` behave as they did under PUC Lua, so a plugin that reads its tasks file, reads its token or runs a command keeps working. `loadlib` and `os.exit` stay out: one loads native code no policy can read, the other ends the daemon.
- [x] **Typed plugin gate** `P1` — `cru plugin check` typechecks a plugin against generated `cru.*` declarations · `crucible-lua`, `crucible-cli`
  - **Gets you:** a strict-mode plugin is checked before it ships, against declarations generated from the Rust-side signature model rather than hand-written ones.
- [x] **Plugin System** `P0` — discovery, the spec, activation · [[Help/Extending/Creating Plugins]] · `crucible-lua`, `crucible-daemon`
  - **Gets you:** every shipped plugin is discovered from its `init.luau` and its `spec.luau` fragment, is named by the Builtin fragment in `runtime/defaults/init.luau`, executes once in the daemon VM at activation, and reports `state: Active` with no `last_error` in `plugin.list`. There is no manifest file: a directory that holds an `init.luau` is a plugin; the fragment supplies the name and the version, and the module table `init.luau` returns supplies the tools and the commands. The operator lists plugins with `cru.plugin.setup` in `init.lua`.
- [-] **Tool Annotations** `P0` — REMOVED. `@tool`/`@param`/`@handler` doc-comment discovery is gone · [[Help/Extending/Custom Tools]] · `crucible-lua`
  - **Gets you:** nothing now, deliberately. There were three separate annotation parsers and two parallel tool systems: the internal agent dispatched the spec table and ignored annotations, while `cru mcp` served an annotation-scanned set from a different directory by different rules. `cru mcp` serves the plugin registry now, so both surfaces advertise one set. A load-bearing comment fails silently, which is why the form went rather than being unified.
- [x] **Event Hooks (note lifecycle)** `P0` — `note:created`, `note:modified` and friends firing into Lua · [[Help/Extending/Event Hooks]] · `crucible-lua`, `crucible-daemon`
  - **Gets you:** `cru.on("note:created", …)`, `note:modified`, `note:deleted` and `note:renamed` firing when the note pipeline writes, with the kiln-relative path as the identifier `opts.pattern` globs against — so a handler narrows to `Daily/*`. The note store always returned these events and the pipeline always dropped them; `process_with_events` hands them back and the kiln manager broadcasts them. Three deliberate limits, all tested: a full kiln index announces nothing for the files it indexes (it reports `process_complete` for the run instead, and announces only a `note:deleted` per stale index row its reconciliation pass drops), an unchanged file announces nothing, and a rename announces `note:deleted` + `note:created` + `note:renamed` because the reindex really performs all three.
- [x] **Lua File-Watch Hooks** `P0` — `cru.on("FileChanged", …)` fires when the workspace changes · `crucible-lua`, `crucible-daemon`
  - **Gets you:** the trigger a daemon-computed statusline value like git status actually needs, since files change while you are not in a turn.
  - **Was marked `[x]` while dead at two layers, for months.** `cru.on` validated against a hand-written list of names, which did not include the file events, so `cru.on("FileChanged", ...)` raised "unknown event" and could not be registered at all. And the dispatcher read `handlers_for` — the annotation-discovered vec nothing writes — rather than `runtime_handlers_for`, which is where `cru.on` records. Registered nothing, matched nothing, silently. The old proof cited `to_internal_event` returning `Some`, which is the *translation*; every test in the module covered that half and stopped before delivery, which is how it survived.
- [x] **Custom Handlers** `P0` — event handler chains with interception and transformation · [[Help/Extending/Custom Handlers]] · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a handler a *plugin* registered runs on a real tool call and its return value becomes the tool result the model and the UI see; a handler can rewrite arguments before dispatch, patch a handled result, and an error in a gate handler blocks execution (fail-closed).
- [x] **Handler Registration Order** `P0` — a handler runs where it registered; there is no priority option · `crucible-lua`
  - **Gets you:** one ordering rule with nothing to negotiate. `priority` is DELETED. Neovim orders no autocommand either, and the option was not doing its job here: `replaces()` never had it in the key, so two session-scoped rows differing only by it already collapsed. Registration order is total — `runtime/defaults/init.luau`, then your `init.lua`, then the plugins alphabetically by name (`PluginManager::load_all` sorts its key set).
- [x] **Execution Backends as Plugins** `P1` — workspace tools intercepted via `pre_tool_call` and routed to alternate backends; the agent core stays backend-agnostic · [[Help/Extending/Container Isolation]] · `runtime/plugins/oci/` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** `{ handled = true, result = … }` from a plugin bypasses the default executor and its value becomes the tool result. The reference `oci` plugin runs workspace tools inside OCI containers via `podman`/`docker`/`nerdctl exec`; the runtime is auto-detected, isolation that cannot be established **fails closed** rather than falling back to the host, and any tool no handler took over is denied by name. The guarantee holds for **internal agents only** — the dispatch layer refuses to pair an isolation claim with an ACP agent, which executes tools in its own process. Sandbox isolation is a plugin, not a core concern; the daemon never grows a per-backend abstraction.
- [-] **Oil UI DSL** `P1` — Luau API for interaction modals (ask, popup, panel) · [[Help/Extending/Scripted UI]] · [[Help/Plugins/Oil Lua API]] · `crucible-lua`, `crucible-oil`
  - **Gets you:** nothing a Lua script declares ever reaches a rendered frame.
- [ ] **Agent Render Themes** `P1` — theme files supply per-event `render` functions that return full OIL trees (tool calls, shell, skills); `cru.render.*` exposes syntect/diff/markdown as composable fragments, not palette slots · [[Help/Extending/Scripted UI]] · [[Help/Plugins/Oil Lua API]] · [[Help/Extending/Event Hooks]] · `crucible-lua`, `crucible-cli`, `crucible-oil`, `crucible-daemon`
  - **Gets you:** agent-familiar transcript layouts (Codex inline `Ran cmd`, Claude bordered bash, OpenCode cards) by switching theme — node topology, not just accent color. Supersedes the hint-only `tool:display_*` path for layout; colorscheme files become one slice of a render-strategy bundle.
- [x] **Lua API Modules** `P0` — ~25 module tables plus 7 top-level helpers under the unified `cru.*` namespace (`crucible.*` retained as a long-form alias) · `crucible-lua`
  - **Gets you:** `cru.check`, `cru.config`, `cru.context`, `cru.emitter`, `cru.errors`, `cru.fs`, `cru.health`, `cru.http`, `cru.interaction`, `cru.json`, `cru.kiln`, `cru.oil`, `cru.oq`, `cru.paths`, `cru.ratelimit`, `cru.schedule`, `cru.service`, `cru.session`, `cru.shell`, `cru.statusline`, `cru.storage`, `cru.timer`, `cru.tools`, `cru.ws`, plus `fmt`, `get_session`, `inspect`, `log`, `retry`, `spawn`, `tbl_deep_extend`, `tbl_get`. `cru.graph` **now exists** — `register_graph_module` installs the bare global `graph` *and* registers it on `cru`, and `oq` and `paths` are registered the same two ways.
- [x] **Timer/Sleep Primitives** `P1` — `cru.timer.sleep(secs)`, `cru.timer.timeout(secs, fn)`; backed by `tokio::time` · `crucible-lua`
  - **Gets you:** sleep actually suspends and timeout actually expires, including from inside a plugin lifecycle hook (hooks are fired in an async context).
- [x] **Rate Limiting** `P1` — `cru.ratelimit.new({ capacity, interval })` token bucket · `crucible-lua`
  - **Gets you:** the bucket actually blocks and refills; `:acquire()` waits, `:try_acquire()` does not, `:remaining()` reports.
- [x] **Retry with Backoff** `P1` — `cru.retry(fn, opts)` exponential backoff with jitter · `crucible-lua`
  - **Gets you:** a failing function is retried the configured number of times and then raises; a non-retryable error stops immediately.
- [x] **Event Emitter** `P1` — `cru.emitter.new()` minimal pub/sub · `crucible-lua`
  - **Gets you:** `:on`/`:emit`/`:off`/`:once` fire, stop firing, and fire exactly once, in registration order.
- [x] **Argument Validation** `P1` — `cru.check.string()`, `.number()`, `.table()`, `.one_of()` with optional/range constraints · `crucible-lua`
  - **Gets you:** a bad argument raises a Lua error naming the parameter; a good one passes.
- [-] **`cru.storage`** `P1` — per-plugin persistent key/value store · `crucible-lua`, `crucible-daemon`
  - **Gets you:** the module is on the plugin VM and is upgraded with a real `PropertyStore` at daemon boot, so a plugin can namespace state by plugin name.
- [x] **Plugin Config** `P0` — per-plugin configuration schemas · [[Help/Lua/Configuration]] · `crucible-lua`, `crucible-daemon`
  - **Gets you:** one `opts` table per plugin, merged lowest first from the plugin's fragment, the shipped defaults' entry, the config leaves under `plugins.<name>` (`cru.config.set` in `init.lua`, or `settings.json`), and the operator's spec entry `opts`. The host passes it to `setup(opts)` once, after `init.lua` finishes; an entry's `config = function(m, opts)` replaces that call. A `require("<name>").setup{…}` line in `init.lua` still runs, first, and the host's call follows. A user `init.lua` that raises fails open onto the seed; one that does not parse names the line and stops the daemon.
- [x] **Lua Is The Config** `P0` — a user `init.lua` runs in the plugin runtime and authors the app config; `cru.config` reads and writes it from Lua · `crucible-lua`
  - **Gets you:** the Neovim model rather than a parallel store — the load-bearing fact for anyone writing an `init.lua`, and the prerequisite for the session default tier. `config.toml` was the seed under it until v0.30.0 and is not read now; `cru config migrate` moves one into Lua.
- [x] **Session Default Tier** `P0` — `chat.system_prompt` in the config store is the `:set`-global analogue; `session.x` is the buffer-local one · `crucible-lua`, `crucible-daemon`
  - **Gets you:** every new session inherits the configured value unless an agent card or an `on_session_start` hook overrides it, and the value is visible as session state rather than only applied at send time. The shipped prompt is `ChatConfig::default()`, so it sits on the store's `Default` layer and both `settings.json` and `~/.config/crucible/init.lua` outrank it. A failing start hook does not break the session.
  - **Was `cru.defaults`.** That was a second store: one key, its own lock, no provenance — so `settings.json`, `:set`, `config.origin` and the settings UI all passed it by. Folded into the config store on 2026-09-10.
- [x] **`cru.modes` — Lua-Declared Modes** `P0` — `cru.modes.x = { tools = …, permissions = { default/allow/deny/ask } }` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a mode is data. The shipped `ask`/`plan`/`auto` are themselves Lua declarations and can be removed or redefined; a user-invented mode is selectable, filters the advertised tool set, and supplies a permission stance plus a rule list evaluated by the same engine — so `bash:rg *` inherits chained-command handling and a mode can permit specific *commands*, not just whole tools. An unknown or vanished mode **fails closed** rather than becoming the most permissive. Use a static stance for the simple case and a hook for the conditional one.
- [x] **Plugin-Declared Commands** `P0` — a plugin's `spec.commands` become invocable commands, listed and reachable over RPC · `crucible-lua`, `crucible-daemon`, `crucible-cli`, `crucible-web`
  - **Gets you:** `/name args` in the TUI and the web palette invokes the plugin's command through the daemon registry instead of going to the agent as a chat message. Shadowing is deliberate: a plugin cannot shadow a built-in.
- [x] **Plugin-Published Session Status** `P0` — `cru.plugin.set_status{ session, key, plugin, text, level }` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a plugin can say something durable about a session that the TUI and web show — this is how `oci` reports "sandboxed: `<image>` (`<runtime>`)". It exists because a session's isolation state was otherwise unverifiable from the UI.
- [x] **Plugin Publications** `P0` — `cru.plugin.publish(key, value)` stores a value in the daemon; `plugin.publications` serves every key with the plugin that answered · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a plugin answers a named question once and every client reads the answer back over RPC. Two plugins that answer one key are both kept and attributed; a republish replaces that plugin's previous answer; a plugin release drops only its own keys. The auto-title plugin uses the `session_title` key this way.
- [ ] **Lazy Tool Collections** `P3` — a plugin declares a NAMED SET of tools that is absent from the agent's tool list until something loads it, the way an MCP server is a collection you connect to rather than a tool you always have. Today `spec.tools` registers into `plugin_registry` at activation (`daemon_plugins/mod.rs:1118`), so every declared tool is present for every session for the daemon's lifetime, and its JSON Schema — rendered from the declared types by `signature.rs` — sits in the prompt whether or not the turn could use it. The cost is proportional to the tool count, which is why a big plugin is expensive to have installed and cheap to have unused. **Three things that make this harder than it looks, all deliberate**: (1) `ToolSurface` is a closed enumerated table with no `Default`, and an unrecognised tool resolves to `ToolSurface::Unknown`, which the isolation gate REFUSES — so a lazily-loaded tool must arrive with a declared surface, not merely a name; (2) the permission layer and `PatternStore` key on tool names, so a tool that appears mid-session appears without a rule; (3) per-session tool surfaces do not exist — the per-session Lua VM was deleted in `f4b6001b8` for having no production subject, so "loaded for this session only" needs a per-session registry that is not there. A plugin wanting per-session behaviour keeps its own session map today, as `runtime/plugins/oci/init.luau:45` does · `crucible-lua`, `crucible-daemon`
- [x] **Plugin Options** `P0` — `cru.plugin.options{}` declares a settings tree once; `plugin.options`, `plugin.option_get`, `plugin.option_set` and `plugin.option_execute` serve it · `crucible-lua`, `crucible-daemon`, `crucible-web`
  - **Gets you:** a plugin declares its settings as data with a getter, a setter and an optional action per node; the web settings page reads the tree and writes a value through the plugin's own setter. A changed value persists under the daemon data root and replays through the setter on the next load; a stored value for an option the plugin dropped is skipped, not fatal.
- [x] **Provider Auth Hooks** `P1` — `cru.on_provider_auth(fn)` lets a plugin supply the headers a provider client sends · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a plugin that returns an `Authorization` header for a provider replaces the configured key for that client; a hook that returns nothing keeps the config fallback. A plugin reload clears only that plugin's hooks and does not register them twice.
- [x] **Lua Notifications** `P1` — `cru.log.notify` and `cru.log.notify_once` send notifications through the daemon's sink · `crucible-lua`, `crucible-daemon`
  - **Gets you:** level, progress and optional scope reach the notification hub; `notify_once` deduplicates only after successful delivery. The unconsumed `cru.log.messages.*` panel controls were retired in September 2026; panel visibility belongs to each client.
- [x] **Isolation Claims** `P0` — `cru.isolation.require{ session, plugin }` marks a session as sandboxed by a plugin · `crucible-lua`, `crucible-daemon`
  - **Gets you:** after a claim, a host tool that no `pre_tool_call` handler takes over is refused, a daemon-surface tool still runs, and the `exempt` list reopens named tools. A claim is scoped to one session; a delegated child inherits the parent's claim and releases it when the child ends. The `oci` plugin is the shipped claimant.
- [-] **Plugin File Watcher** `P1` — `[plugins] watch = true` reloads a plugin when its file changes · `crucible-daemon`
  - **Gets you:** the config key now exists (it previously had none and the watcher was hardcoded off), and the loaded-plugin directory list is tracked.
- [-] **Script Agent Queries** `P0` — `ask.agent(batch)` from Lua · [[Help/Extending/Script Agent Queries]] · `crucible-lua`
  - **Gets you:** nothing. The documented entry point does not exist.
- [x] **HTTP Module** `P0` — HTTP client for plugins · [[Help/Extending/HTTP Module]] · `crucible-lua`
  - **Gets you:** a plugin issues a real HTTP request from the daemon VM and reads the status, with connection failures surfaced rather than swallowed.
- [-] **Lua Integration (full)** `P1` — "complete scripting API for custom workflows and callout handlers" · `crucible-lua`
  - **Gets you:** undefined. The entry names no surface, no acceptance criterion and no consumer.
- [x] **Hook Documentation** `P1` — comprehensive guide on extending Crucible · [[Help/Extending/Event Hooks]]
  - **Gets you:** a plugin author gets the full hook contract — every dispatched event, its `ctx`/`event` fields, and its return-value table — plus companion guides for handlers, plugin creation, custom tools, container isolation, the HTTP module and the plugin manifest.

### Plugin Developer Experience

> The Discord plugin proved the plugin system can express a real integration (a ~1400-line multi-module plugin with WebSocket, REST and streaming). It now ships in `runtime/plugins/discord/` and its Lua suites run under `shipped_plugin_lua_suite_passes` inside `just test ci`, so the routing, quota and gateway-reconnect logic is tested; the transport itself still is not. These items close the gap between "works" and "easy to write".
>
> **Guiding insight**: Neovim's plugin ecosystem exploded when LuaLS type stubs + lazy.nvim hot reload made Lua plugins as ergonomic as TypeScript. Crucible needs the same inflection point.

- [x] **LuaCATS Type Stubs** `P1` — `StubGenerator::generate` emits `cru.lua` (EmmyLua/LuaCATS) plus `cru-docs.json` when you run `cru plugin stubs` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** `cru plugin stubs` writes `~/.config/crucible/luals/cru.lua` and `cru-docs.json`, which a plugin author points LuaLS at. The daemon wrote them on every start until 2026-08-24; it wrote a file only a plugin author reads, into a directory a test then edited.
- [x] **Type Stub Coverage** `P1` — the stubs describe the real `cru.*` surface · `crucible-lua`
  - **Gets you:** autocomplete for exactly the namespaces the plugin VM has — every one of them, and nothing else. A module registered tomorrow is stubbed without editing a list.
- [x] **Plugin Hot Reload** `P1` — `:reload <plugin>`; `plugin.reload` + `plugin.list` RPCs · `crucible-lua`, `crucible-daemon`, `crucible-cli`
  - **Gets you:** editing a plugin's Lua and reloading makes the new code take effect, its `on_unload` hook fires, its handlers are replaced rather than duplicated, and a failed reload leaves the plugin inert in state `Error` rather than half-registered.
- [x] **`:lua` REPL** `P1` — `lua.eval` RPC + `cru lua` CLI; `=expr` prints the result (the Neovim pattern) · `crucible-cli`, `crucible-daemon`
  - **Gets you:** the fastest way to falsify any Lua-API claim in this document — it is what caught the `cru.graph` and `ask.agent` gaps during the sweep.
- [x] **`cru plugin new`** `P1` — scaffold a plugin from a template · `crucible-cli`
  - **Gets you:** `init.lua`, `health.lua`, `.luarc.json` and `tests/init_test.lua`, and the path printed. **Its own printed next step is broken**: `cru plugin test .` run verbatim from the plugin dir prints `0 passed, 0 failed` and exits 0 (a silent false green — the relative path is resolved in the *daemon's* cwd), and with an absolute plugin-dir path the template's `require("init")` fails because the harness deliberately drops `<plugin_dir>/?.lua` from `package.path`. It only passes when pointed at the test *file*.
- [x] **Clean Error Messages** `P1` — `format_lua_error()` strips Rust FFI frames; errors carry plugin name, file path and line · `crucible-lua`
  - **Gets you:** a Lua error reaches you as plugin + file + line, not a Rust stack — including in RPC response bodies, where the line number is the assertion's line in your test file rather than a frame inside the runner.
- [x] **Plugin Test Harness** `P2` — `cru plugin test <path>` runs a busted-style suite against a mocked `cru.*` API · `crucible-lua`, `crucible-cli`
  - **Gets you:** `N passed, N failed` with per-failure suite / name / message / `file:line`, `describe`/`it`/`pending`/`expect.equal`/`expect.truthy`/`before_each`, mocked `cru.*` so tests need no daemon, and every shipped plugin's suite running in CI. Caveat: a **relative** `test_path` is resolved in the daemon's cwd, so `cru plugin test .` reports a false green.
- [x] **`.luarc.json` Generation** `P2` — scaffolded `.luarc.json` points at the type stubs for zero-config IDE setup · `crucible-cli`
  - **Gets you:** `cru plugin new` emits a `.luarc.json` whose `workspace.library` contains the stub directory the daemon writes to, and `diagnostics.globals` covers `cru`.
- [-] **`cru plugin` Lifecycle Subcommands** `P2` — `list`, `remove`, `update`, `health` · `crucible-cli`
  - **Gets you:** the subcommands exist and dispatch alongside the covered `new` / `test` / `stubs` / `add`.

### Plugin Abstractions

> Extracted from building the Discord plugin. These target the plugin types we expect to be most common: messaging bots, autonomous loops, content transformers, and long-running services.

- [x] **`cru.service`** `P1` — service lifecycle for long-running plugins: declarative descriptor with `start`/`stop`/`health`, config schema with validation, and `status`/`list`/`stop` · `crucible-lua`
  - **Gets you:** a plugin declares a service and gets back a spawnable descriptor; `status`/`list` report it and `stop` runs the stop hook. Each declared service is `tokio::spawn`ed at plugin boot. These are reachable **only from Lua** (or `cru lua`) — there is no RPC, CLI or TUI surface, so there is no operator visibility into running plugin services.
- [-] **Service Supervision & Secret Resolution** `P1` — supervised restart with backoff via `cru.retry`; `secret=true` resolves `CRUCIBLE_<PLUGIN>_<KEY>` from the env first · `crucible-lua`
  - **Gets you:** neither behaviour is proven. `Service.stop` also only flips a flag and calls the stop hook — it cannot cancel the spawned task, which keeps retrying.
- [ ] **`cru.messaging`** `P2` — adapter trait for chat platform integrations; normalizes the receive → should_respond → session → send_and_collect → format → reply loop across Discord/Telegram/Slack/Matrix; builds on `cru.service`; **extract from two concrete implementations**, don't speculate the shape. Gate unmet — `runtime/plugins/discord/` is still the only messaging plugin, so there is no second implementation to extract from · `crucible-lua`
- [ ] **`cru.transform`** `P2` — content transform pipeline; `register(name, fn)` + `pipeline({…})` composing pure text→text functions for table formatting, mermaid rendering, citation insertion, platform-specific markdown cleanup; the unit messaging adapters plug into for `format_response` · `crucible-lua`

## Agent Protocols (ACP & MCP)

### ACP Host (Crucible → External Agents)

Crucible acts as an **ACP host**, spawning and controlling external AI agents (Claude Code, Codex, Cursor, Gemini CLI, OpenCode) with Crucible's memory, context, and permission system.

- [x] **ACP Host** `P0` — spawn and control ACP agents over JSON-RPC 2.0 on stdio, with capability negotiation · [[Help/Concepts/Agents & Protocols]] · `crucible-daemon` (acp)
  - **Gets you:** `cru chat -a <agent>` spawns a real external agent process, completes the ACP handshake, and streams its reply back into the session. Agent-reported MCP capabilities select the transport, with a stdio fallback when HTTP is unsupported.
- [x] **Context Injection** `P0` — daemon-side Precognition results are prepended to the ACP prompt as a tagged System block · `crucible-daemon` (acp)
  - **Gets you:** an external agent sees knowledge-graph context it has no other way to reach, ordered before your own content. (The old entry's `PromptEnricher`, `<precognition>` XML block and `ContextConfig::inject_context` do not exist — the real marker is a `ContextMessage::system` tagged `PRECOGNITION_TAG`.)
- [x] **In-Process MCP Host** `P0` — an MCP server running in-process; agents discover Crucible tools without an external server · `crucible-daemon` (acp)
  - **Gets you:** an external agent hits Crucible's HTTP/SSE endpoint and gets back a real tool list and real tool results, with no separate process to run.
- [x] **MCP Transport Negotiation** `P0` — HTTP for agents that support it, stdio `cru mcp --stdio --standalone` for everything else · `crucible-daemon` (acp)
  - **Gets you:** every agent gets Crucible's tools regardless of which MCP transport it speaks, without you configuring anything.
- [x] **Agent Discovery** `P0` — parallel probing of the builtin profiles `opencode`, `claude`, `gemini`, `codex`, `cursor`, with per-agent env injection from `[acp.agents.*]` · `crucible-daemon` (acp)
  - **Gets you:** `cru chat -a <name>` finds the installed agent binary, applies its configured env to the spawned process, and a missing binary produces a clear error instead of a hang or panic. Custom profiles can `extends` a builtin. Known limitation: the agent cache is a process-wide static, so discovery results persist for the daemon's lifetime.
- [-] **Agent Discovery — daemon PATH false negatives** `P0` — the picker reports installed agents as unavailable when the daemon's `PATH` is narrower than the user's shell · `crucible-daemon` (acp)
  - **Gets you (the bug):** `is_agent_available` (`acp/discovery.rs:349`) probes with `which` in the **daemon's** environment. The daemon is long-lived and keeps whatever `PATH` its original parent had — reproduced 2026-08-15: a live daemon with `PATH=/usr/lib64/ccache:~/.cargo/bin:/usr/local/bin:/usr/bin` reported cursor, opencode, and gemini uninstalled while all three (plus `cursor-acp`, `claude-agent-acp`, and `npx`, which the claude/codex bridges need) were on the user's shell `PATH` under `~/.local/bin`, `~/.opencode/bin`, and nvm's bin dir. The process-wide discovery cache (noted above) then pins the false negatives for the daemon's lifetime. Launch has the same defect as probe: a spawn would use the same narrow `PATH`.
  - **Fix direction:** probe and spawn against the *client's* environment, not the daemon's — either (a) the CLI/web client sends its `PATH` (or pre-resolved absolute agent paths) with the discovery/launch RPCs, or (b) the daemon resolves the user's login-shell `PATH` once (`$SHELL -lc 'echo $PATH'`, the VS Code approach) and uses it for both probing and spawning. Either way, probe results must carry absolute paths so launch cannot disagree with discovery, and the cache must key on the resolved environment.
- [ ] **Sandboxed Filesystem** `P0` — path validation, traversal prevention, mode-based permissions for ACP agent file access · `crucible-daemon` (acp)
  - **Not started, and deliberately so.** Crucible does not mediate ACP agent filesystem access. It sends `InitializeRequest` with no `ClientCapabilities`, so `fs.readTextFile` and `fs.writeTextFile` are both false and conforming agents never route file access through us; one that asks anyway now gets JSON-RPC `-32601` rather than silence. The dead implementations this entry used to point at — `acp/filesystem.rs` and `acp/acp_client.rs` — have been deleted rather than wired.
  - **Why deleted, not wired:** every supported agent also has `Bash`, so containment on the ACP `fs/*` seam holds only against an agent that cooperates. The isolation that holds against one that does not is the OCI container plugin, and the path-sandboxed surface agents are meant to reach the kiln through is the MCP note tools. Wiring `fs/*` would have added neither — Crucible already sees *that* a read happened, via the agent's own `tool_call` frames. It would have added only the ability to refuse a read and the file contents in-process, at the cost of a permanent ACP conformance surface.
  - **If this is ever picked up**, the decision to revisit is read-only wiring (`readTextFile: true`, `writeTextFile: false`) for content capture. Full wiring is the worst of the options: writes are already gated by `session/request_permission` on the tool call.
- [x] **Permission Gate** `P0` — ACP `session/request_permission` routes through the session's resolved permission policy (agent profile → global → CLI override); no handler ⇒ deny · `crucible-daemon` (acp)
  - **Gets you:** when an external agent asks to run an unsafe tool, your policy decides, and the agent receives the corresponding `Selected`/`Cancelled` response — a denied tool does not run.
- [x] **Streaming Responses** `P0` — chunk processing, tool-call parsing from the stream, diff handling · `crucible-daemon` (acp)
  - **Gets you:** text, thinking and tool-call chunks appear incrementally in the chat as the external agent produces them, and file writes render as diffs; cancelling mid-stream aborts and closes the transport.
- [-] **ACP Session Management** `P0` — sessions carrying config (cwd, mode, context size), history with ACP roles, persistence across reconnections · `crucible-daemon` (acp)
  - **Gets you:** per-session isolation, and agent-side persistence across daemon restarts (added with the SDK 2.0 client, 2026-08-23): the handle persists the agent's own session id, and a handle rebuilt after a restart sends `session/resume` with it. An agent that answers `-32601` gets `session/new` instead, and the fallback is announced on the session's event stream as `acp_resume_fallback`. On shutdown the client sends `session/close` when the agent advertised `sessionCapabilities.close`; a `-32601` reply to it is not an error. Still absent: config carrying (mode, context size). "History with ACP roles" stays out by design — ACP agents own their history, so `turn()` sends only the new user content (`acp_handle/translate.rs`).
- [x] **ACP Model Switching** `P0` — `:model` on an ACP session switches the model on the **running** agent process · `crucible-daemon` (acp)
  - **Gets you:** the handle reads the model selector (the `select` option with category `model`) from `configOptions` in the `session/new` reply; `switch_model` sends `session/set_config_option`, so the agent keeps its history. An agent with no `configOptions` gets no model switching.
- [x] **ACP Session Recording & Replay** `P0` — `CRUCIBLE_ACP_RECORD_DIR` captures every ACP frame to a JSONL fixture replayable through the real client · `crucible-daemon` (acp)
  - **Gets you:** the whole ACP suite runs in CI with no agent binary installed — this is the mechanism that makes none of those tests `#[ignore]`d.

### ACP Agent (Crucible as Embeddable Agent)

- [x] **ACP Agent Mode** `P1` — Crucible as an embeddable ACP agent (`cru acp`); any ACP host (Zed, JetBrains, Neovim) spawns Crucible to get the knowledge graph plus memory · `crucible-cli` (commands/acp)
  - **Gets you:** each ACP session is a real daemon session (Precognition, kiln tools, persistence); prompts, thinking and tool calls stream as `session/update`, and the session id shows up in `cru session list`.
- [x] **ACP Schema Currency** `P1` — the pinned ACP schema and SDK are current · `crucible-daemon` (acp), `crucible-core`
  - **Gets you:** nothing left to bump. This entry used to ask for `0.10.6 → 0.10.7`; the repo is several releases past that in both directions.
- [ ] **ACP Registry Submission** `P1` — agent manifest for the [ACP Registry](https://github.com/agentclientprotocol/registry); one PR → available in all ACP clients · `crucible-daemon` (acp)

### MCP Server (External Agents → Crucible Tools)

- [x] **MCP Server** `P0` — expose the kiln as MCP tools for external AI agents · [[Help/Concepts/Agents & Protocols]] · `crucible-daemon` (tools)
  - **Gets you:** an external agent connected to `cru mcp` lists Crucible's kiln and delegation tools — 15 of them — and gets real results back. The surface is what Crucible uniquely has; it does not re-serve `bash` or file editing that the connecting harness already provides.
- [x] **Note Tools** `P0` — `create_note`, `read_note`, `read_metadata`, `update_note`, `delete_note`, `list_notes` · `crucible-daemon` (tools)
  - **Gets you:** an external agent creates, reads, updates and deletes real `.md` files in the kiln with frontmatter, and gets structured results — and these tools *are* path-sandboxed, unlike the ACP filesystem seam.
- [x] **Search Tools** `P0` — `semantic_search`, `grep_notes`, `property_search` · `crucible-daemon` (tools)
  - **Gets you:** an external agent searches the kiln semantically, by text, and by frontmatter property, and gets matching notes back — with folder traversal denied.
- [x] **Kiln Tools** `P0` — `get_kiln_info` · `crucible-daemon` (tools)
  - **Gets you:** an external agent asks what kiln it is attached to and gets a real note count and path back.
- [x] **Workspace Tools** `P0` — `read_file`, `edit_file`, `write_file`, `bash`, `glob`, `grep` — **Crucible's own agent, not the MCP surface** · `crucible-daemon` (tools)
  - **Gets you:** Crucible's internal agent reads/edits/writes real files, runs shell commands with real exit codes and timeouts, and globs/greps the workspace. They are deliberately **not** served over MCP: a harness that speaks MCP already has its own, Crucible enforced no permission checks on the copies it served, and `agent_factory` added the same six separately so every kiln session advertised each of them to the model twice. Removed from `CrucibleMcpServer`; the surface there is kiln + delegation.
- [-] **TOON Formatting** `P0` — token-efficient response formatting · `crucible-daemon` (tools)
  - **Gets you:** TOON for **Lua plugin tool results only**. Every built-in Crucible MCP tool still returns JSON, so this entry — filed under the server that exposes those tools — promises a token saving nobody using them gets.

### MCP Gateway (Crucible → Upstream MCP Servers)

- [x] **MCP Gateway** `P0` — connect upstream MCP servers with prefixed tool names; `McpGatewayManager` is shared daemon-wide so every session sees the same upstreams · [[Help/Extending/MCP Gateway]] · [[Help/Config/mcp]] · `crucible-daemon` (tools)
  - **Gets you:** intended — tools from a configured upstream appear in the agent's tool list under a `prefix_` name and are callable. Prefix validation, allow/block filtering and precedence are unit-tested; the wiring from config → daemon bind → LLM tool defs → `GatewayToolExecutor` is complete and coherent.
- [x] **Lua Plugin Tools** `P0` — dynamic tool discovery from Lua plugins · `crucible-daemon` (tools), `crucible-lua`
  - **Gets you:** a tool declared in a Lua plugin shows up in the agent's tool list and the agent can actually invoke it.
- [x] **MCP Auto-Reconnect** `P0` — recover a dropped upstream without restarting the daemon · `crucible-daemon` (tools)
  - **Gets you:** an upstream with `auto_reconnect = true` that dies comes back without a daemon restart. The server spawns `McpGatewayManager::start_reconnect_loop` at bind when at least one upstream asks for it (`crates/crucible-daemon/src/server/mod.rs`, "Reconnect loop"); the loop polls every 30 s and backs off per upstream up to 300 s (`ReconnectSchedule::default`). A call that times out, or whose transport is gone, marks the upstream `Disconnected` so the loop restarts it; a tool that answers with an error stays connected. Limit: the TUI "live" status is still one-shot — `chat_runner/runner.rs` builds a second gateway from config, sends one status message and drops it (plan section 5a, T3-A10).
- [-] **MCP `readOnlyHint`** `P1` — an upstream tool declaring `readOnlyHint` stops being classified unsafe · `crucible-daemon` (tools)
  - **Gets you:** fewer permission interruptions when using a gateway with read-only tools — intended, and the classification change is in the tree.

## Distribution & Growth

> How Crucible reaches users and spreads. Ordered by growth impact.
>
> **Insight from OpenClaw analysis (2026-02):** viral growth came from instant install, meeting users in apps they already use, and proactive behavior. Crucible's counter-position: "Your AI should live in your notes, not a chat app you don't control."

### Install & Onboarding (P0 — #1 adoption blocker)

- [x] **One-Line Install** `P0` — pre-built binaries via GitHub Releases; `curl -fsSL … | sh` · `crucible-cli`
  - **Gets you:** a working `cru` binary in well under a minute, with the web UI included — the release features are `["fastembed", "web"]` and CI builds `crucible-web/web/dist` with bun before the dist plan runs, so rust-embed has a real frontend to embed.
- [-] **Install Platform Coverage** `P0` — linux x86_64/aarch64, macOS Intel/Apple Silicon · `crucible-cli`
  - **Gets you:** two platforms, not four. `Cargo.toml:153-156` targets `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin` only, so a Linux-ARM or Intel-Mac user gets "no prebuilt binary for your platform" from the installer.
- [-] **Homebrew Tap & `cargo binstall`** `P0` — `brew install mootikins/crucible/crucible`, `cargo binstall crucible-cli` · `crucible-cli`
  - **Gets you:** unverifiable from this repo, and the in-repo evidence points the other way.
- [x] **Precognition Default-On** `P0` — changed from opt-in to on; knowledge-graph-aware context is the differentiator · `crucible-cli`, `crucible-daemon`
  - **Gets you:** a fresh session's first user message reaches the model with kiln notes injected, with no config from you.
- [x] **`cru setup`** `P0` — bootstrap the runtime (plugins and themes) into `~/.config/crucible/runtime`, and write a template `~/.config/crucible/init.lua` if you have none · `crucible-cli`
  - **Gets you:** a runtime tree you can edit, whether or not one shipped on disk — with no source to copy from it writes the copy compiled into the binary, and `--force` restores the shipped files over your edits. It deliberately does **not** copy `defaults/`: that file is read first-hit-wins, so a copy would freeze your defaults at the version you ran setup on and every default added later would reach you never. Override them in `~/.config/crucible/init.lua`, which runs after them.

### HTTP Gateway (P1 — platform layer for everything external)

> The daemon is Unix-socket-only (JSON-RPC 2.0). Messaging bots, webhook triggers, the web UI, and any external client all need HTTP access. This is the shared foundation — `crucible-web` wired to `DaemonClient`, exposing the daemon's RPC surface over HTTP + SSE. The method list is served at runtime by `daemon.capabilities` → `methods`; this document deliberately no longer carries a count, because the two it used to carry (35 and 55) disagreed with each other and with reality.

```
HTTP Gateway (crucible-web wired to daemon)
    ├── Messaging bots (Telegram, Discord)
    ├── Webhook endpoints (POST /api/webhook/:name)
    └── Web UI (SolidJS frontend on same server)
         └── Remote access (Tailscale / Cloudflare Tunnel)
```

- [x] **HTTP-to-RPC Bridge** `P1` — `DaemonClient` wired into `crucible-web` Axum routes; HTTP requests translate to daemon JSON-RPC · `crucible-web`, `crucible-daemon` (rpc)
  - **Gets you:** every browser action reaches the daemon — sessions, chat, filesystem, notes, skills, plugins all return daemon-sourced JSON — and a daemon RPC error surfaces as a 502 with an error body rather than a hang. The bridge is no longer purely a translation layer: it also does SSRF validation on session endpoints, path-traversal containment on kiln reads and note-upsert gating.
- [x] **SSE Event Bridge** `P1` — subscribe to daemon session events and stream them to HTTP clients; `EventBroker` fans out per-session · `crucible-web`
  - **Gets you:** chat tokens, thinking, tool cards and title renames stream into the browser live, and filesystem changes push tree updates without a refresh. Three SSE streams exist: `GET /api/chat/events/{session_id}`, `GET /api/fs/events` and `POST /exec` for shell output. This is **SSE only** — the sole WebSocket surface is the terminal PTY.
- [x] **Chat HTTP API** `P1` — session lifecycle and message send over HTTP · `crucible-web`
  - **Gets you:** create, list, pause, resume, end, archive, unarchive, cancel and export sessions; send chat messages; watch replies stream back; switch models and modes; connect and disconnect kilns; set the title (the workspace is fixed at creation). The shipped surface is much wider than any enumeration will stay current with — read the routes.
- [x] **Search HTTP API** `P1` — vector, semantic and grep search plus notes and kilns over HTTP · `crucible-web`
  - **Gets you:** `POST /api/search/vectors`, `POST /api/search/semantic` (embed-then-search, the same two-step the CLI uses, with per-hit similarity scores), `POST /api/search/grep` (ripgrep with char offsets for highlighting), `GET /api/notes`, `GET /api/notes/:name`, `PUT /api/notes/:name`, `GET /api/notes/resolve`, `GET /api/backlinks`, `GET /api/kilns`.
- [x] **API Auth** `P1` — Bearer token middleware with an auto-generated key; localhost bypass; `~/.config/crucible/api_key` persistence · `crucible-web`, `crucible-core` (config)
  - **Gets you:** a non-loopback caller without the right Bearer token or session cookie gets 401, loopback callers pass, and a 0600 key file is generated on first start. `X-Forwarded-For` defeats the localhost bypass, and the shell and PTY routes carry an *additional* localhost-only gate plus a WebSocket Origin allow-list.
- [x] **Cookie Session Auth** `P1` — the browser posts the API key once to `/api/auth/login` and gets an HttpOnly session cookie · `crucible-web`
  - **Gets you:** the key never appears in a URL — the security-relevant half of browser auth, which the Bearer-token entry above does not cover.
- [-] **Webhook API** `P1` — `POST /api/webhook/:name` receives payloads and broadcasts `webhook:received` for Lua handlers · `crucible-web`, `crucible-daemon`
  - **Gets you:** a signed delivery now reaches a Lua handler. `cru.on("webhook:received", { pattern = "ci" }, fn)` fires with `event.name`, `event.headers` (credentials and signature stripped) and `event.body` (the raw JSON string as signed). The dead half named below is fixed: the ingress and the dispatch both take the name from one table.
  - **Still `[-]`, for the second reason only:** the route is mounted **inside** bearer auth, which waves loopback callers through but not remote ones, so a service out on the internet gets a 401 before its signature is read. The ingress works today for senders terminating on the host — a local script, a reverse proxy that adds the bearer header, a tunnel landing on loopback. Moving the route outside bearer auth (with the per-webhook HMAC as its only credential) or gating that on a new opt-in config key is an open decision, deliberately not taken as a side effect of wiring the dispatch. `Help/Config/web.md` states the posture.

### Messaging Integrations (P1 — meet users where they are)

> 1–2 good messaging integrations reduce the need for a web UI substantially. Integrations can be daemon-side Lua plugins or thin adapters over the HTTP gateway.

- [ ] **Telegram Bot** `P1` — Bot API adapter over the HTTP gateway; lowest friction, enables proactive digest delivery · `crucible-telegram` (new crate) · depends: [[#HTTP Gateway|HTTP-to-RPC Bridge]]
- [-] **Discord Plugin** `P1` — Discord integration (REST + Gateway) as a daemon-side Lua plugin · [[Help/Extending/Discord]] · `runtime/plugins/discord/`
  - **Gets you:** a bundled plugin that ships inside every `cru` and loads by default, but dials nothing until `auto_connect` and a token are set, answers nobody until `allowed_users`/`allowed_guilds` name someone, caps each user at `quota_turns_per_day` turns, reconnects after a gateway drop, and can be switched off durably with `[plugins.discord] enabled = false`. Its four agent tools were removed before it shipped: `agent_factory` folds every plugin tool into *every* session's `tool_defs`, so `discord_send` to an arbitrary channel id was an exfiltration path in unrelated sessions.
- [ ] **Matrix Bridge** `P2` — Matrix protocol integration; strong overlap with the self-host/privacy audience · `crucible-matrix` (new crate) · depends: [[#HTTP Gateway|HTTP-to-RPC Bridge]]

### Remote Access (P2 — self-hosting for everyone)

> Agents can't be on every device. Self-hosting with easy remote access is more aligned with "local-first, your notes, your control" than paid cloud hosting.

- [x] **Remote Access Key Distribution** `P2` — `cru web key [--rotate]` prints or rotates the API key remote clients need · `crucible-cli`, `crucible-web`
  - **Gets you:** the connect URL is printed at startup, `cru web key` gives you the key to paste into another device (web UI → Settings → API Access), localhost never needs it, and `api_key = ""` disables auth explicitly. This is currently the **only shipped path** to using Crucible from another device.
- [ ] **`cru tunnel`** `P2` — one-command remote access setup wrapping `cloudflared tunnel` or `tailscale funnel`; exposes the HTTP gateway with auth to your devices · `crucible-cli`
- [ ] **Cloudflare Tunnel Integration** `P2` — `cru tunnel --cloudflare`; auto-configures `cloudflared` with API auth; free tier for personal use · `crucible-cli`
- [ ] **Tailscale Funnel Integration** `P2` — `cru tunnel --tailscale`; WireGuard encrypted, ACL-gated; zero-config for Tailscale users · `crucible-cli`
- [ ] **Paid Hosting** `P?` — multi-tenant hosted option; needs daemon isolation, user management, billing; deferred until clear demand · future

### Proactive Behavior (P2 — viral feature)

> OpenClaw's most praised feature was the heartbeat — the agent reaching out unprompted. Crucible can do this better because it has a knowledge graph, not flat memory. Heartbeat is time-based; webhook triggers are event-driven — Crucible can do both, though the webhook half currently reaches no handler.

- [x] **Scheduled Lua Hooks** `P2` — `cru.schedule({every=N}, fn)` interval callbacks with `cru.schedule.cancel(handle)` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a Lua callback that actually fires on a timer and actually stops when cancelled, on the daemon's own plugin VM. A `MAX_ACTIVE_SCHEDULES = 256` cap is enforced and undocumented elsewhere.
- [-] **Declarative `[[schedules]]` Config** `P2` — `[[schedules]]` blocks (`name`, `every`, `action = "lua:<code>"`, `enabled`) register interval callbacks at daemon boot with no plugin required · `crucible-daemon`, `crucible-core`
  - **Gets you:** the no-code half of the proactive story — intended, parsed, and evaluated onto the plugin VM at boot.
- [ ] **Durable Scheduled Jobs** `P2` — cron-expression jobs with a persistent store and execution history, surviving daemon restart; each run is a real session, and a workflow job resumes from its `WorkflowSnapshot` rather than restarting. Supersedes `[[schedules]]`, which compiles down to the same in-process interval timer as `cru.schedule` and keeps no history · `crucible-daemon` (jobs), `crucible-core`, `crucible-cli`
- [ ] **Skill Blueprints** `P2` — a `schedule:` key in `SKILL.md` frontmatter registers a scheduled automation, making it shareable through existing skill discovery, scope precedence and shadowing with no new object type. Gated on the same trust decision as **Skill Distribution** — a freshly fetched skill must not self-register a job · `crucible-daemon` (skills, jobs)
- [-] **Kiln Digest** `P2` — periodic scan of recent kiln changes surfacing missed connections ("You wrote about X in two notes this week — want me to link them?"); delivered via messaging or TUI notification. Prior art existed in the Discord plugin's `lua/digest.lua`, deleted when the plugin shipped (`aa40eced5`) because it was a second ungated `send_and_collect` entry point · `runtime/plugins/consolidation`, `crucible-lua`
  - **Gets you:** the periodic half now exists as the **Consolidation Pass** (see Self-Improvement Avenues): a `cru.schedule` timer, a sample of recent sessions, one `send_and_collect` reviewer, and pattern notes a human accepts or rejects in the Inbox. It scans sessions, not kiln changes, and it writes pattern notes rather than links between notes, so the digest as described is not what ships.
- [ ] **Daily Briefing Plugin** `P2` — reference plugin summarizing recent kiln changes, pending tasks and orphaned notes; delivered via messaging or shown on TUI startup · `crucible-lua`
- [ ] **Self-Continuation Recipes** `P2` — add goal-loop and heartbeat recipes to [[Help/Delegation Patterns]], built on `turn:complete` + `inject` and `cru.schedule`. The primitives ship and are tested; the recipe is what is missing, and its absence reads as a missing feature · docs

### Default Runtime Plugins (P1 — Neovim-style bundled plugins)

> Crucible ships a `runtime/` directory of Lua plugins alongside the binary, analogous to Neovim's `$VIMRUNTIME/plugin/`. Release tarballs carry no such directory, so the binary carries the tree itself and extracts it when no install put one on disk; see **Bundled Runtime Plugins in Releases**. These load automatically, are overridable, and their source code *is* the documentation for how to build plugins. The bundled set is `daily-notes`, `discord`, `graph-view`, `oci`, `reflection`, `review`, `todo-list`, `web-search` and `worktree`.
>
> **Plugin search path (priority order):**
> 1. `CRUCIBLE_PLUGIN_PATH` — env override (highest priority)
> 2. `~/.config/crucible/plugins/` — user global
> 3. `<entry>/plugins/` for each `runtimepath` entry — opt-in extra trees
> 4. `$CRUCIBLE_RUNTIME/plugins/`, else exe-relative — bundled default (lowest)
>
> Entries 3 and 4 are additive: a configured `runtimepath` adds trees ahead of
> the shipped one, it does not replace it.
>
> **No kiln, project or workspace directory is on this list.** Two entries here
> used to publish `KILN/.crucible/plugins/` and `KILN/plugins/`, which
> `daemon_plugin_paths` never implemented — but a second, divergent path list
> in `PluginManager::with_standard_paths` did, and because it *loaded* what it
> discovered, `session.create` executed every plugin in the kiln it was
> opening. Removed: `server/lua.rs`::a_kiln_plugin_is_neither_listed_nor_executed.
> A kiln's plugins load by putting the kiln on `runtimepath`.

- [x] **Runtime Plugin Path & Provenance** `P1` — `$CRUCIBLE_RUNTIME/plugins/` as a real lowest-priority search path with `PluginSource` tracking · `crucible-lua`, `crucible-daemon`
  - **Gets you:** setting `CRUCIBLE_RUNTIME` (or installing to `<prefix>/share/crucible/runtime`) makes that directory's `plugins/` a real search path tagged `PluginSource::Runtime`, and `source` and `version` reach the `plugin.list` RPC body.
- [x] **Plugin Shadow-by-Name** `P1` — a same-named user plugin wins over the runtime copy · `crucible-lua`
  - **Gets you:** intended — the headline promise of the whole priority table above.
- [-] **Provenance in `:plugins`** `P1` — `:plugins` shows `[path]`, `[user]`, `[runtime]` tags · `crucible-cli`
  - **Gets you:** nothing. No surface in the product displays a plugin's provenance.
- [x] **Bundled Runtime Plugins in Releases** `P1` — the bundled plugins reach an installed user · `crucible-cli`
  - **Gets you:** every bundled plugin loads for anyone who installed Crucible, not only for people who cloned the repo. The tree travels inside the `cru` binary (144K, 22 files) and the daemon extracts it on first start when no install put one on disk.
- [-] **`kiln-expert` Runtime Plugin** `P1` — REMOVED. It hand-rolled agent card + kiln + delegation as a plugin: spawn a subagent with a kiln attached, ask it, summarise, end the session. That is the composition of three intended features, and a card you author says it better. It also shipped with an empty kiln map, so it advertised `search_kiln`/`list_kilns` to every session and both returned nothing
  - **Replacement:** an agent card naming the model, prompt and tools, plus the kiln attached to the session that delegates to it. `session.create` resolves a card by `agent_card` (`server/session/create.rs`), and `cru.session.create` now sends its whole options table through the same path, so a *plugin* can start a card-backed session — `cru.session.create{ agent_card = "researcher" }`. That is what the Discord bot needs.

### Ecosystem & Shareability (P1-P2)

- [x] **Plugin Install** `P1` — `cru plugin add <git-url>` / `cru install`, and the `plugin.install`, `plugin.remove` and `plugin.run_command` RPCs; git-native distribution, lazy.nvim model · `crucible-cli`, `crucible-daemon`, `crucible-web`
  - **Gets you:** `plugin.install` clones the plugin (`--depth 1` unless pinned, `--branch` honoured, `--` terminator against argv injection), checks out the pin with rollback on failure, records it in `<data_home>/plugins.installed.json` under the manifest's sidecar lock, then loads the plugin into the running daemon; `plugin.remove` unloads it and deletes the record, with an optional purge of the clone — and refuses a plugin the operator's `init.lua` names with a git source, naming the `cru.plugin.setup` entry to edit; `plugin.run_command` invokes a plugin-declared command. `plugin_ops::install_at` and `remove_at` take the manifest path and the plugins directory as values, so the tests run under a temp directory.
- [ ] **Skill Distribution** `P1` — `cru skills install <url>` fetches a third-party `SKILL.md` bundle, records provenance, pins it by content hash in a `skills.lock`, and refuses content that **Prompt-Injection Scanning** rejects. Distinct from **Plugin Install**, which distributes Lua; this distributes instructions that reach the model's system prompt, which is why the scan is a hard block here rather than an annotation. Reuses `scm.rs` URL hardening and `plugin_ops.rs`'s flock'd declaration write, but takes `data_home` as a parameter so it is testable, as `plugin_ops::install_at` now does for **Plugin Install** · `crucible-daemon` (skills), `crucible-cli`
- [x] **Documentation Site** `P1` — a published Starlight site built from `docs/` in place, deployed to GitHub Pages · `docs-site/`
  - **Gets you:** the docs kiln as a browsable public site with a landing hero, a per-page neighbourhood graph, hover previews that promote to floating windows, and canvases published as read-only boards.
- [ ] **Agent Memory Branding** `P1` — rename "Precognition" to "Agent Memory" in user-facing docs; communicates the value proposition directly · docs
- [ ] **`cru share`** `P2` — export sessions as self-contained HTML or shareable artifacts; `:export` exists for local markdown, this adds shareable formats · `crucible-cli`
- [ ] **Graph Visualization** `P2` — shareable knowledge graph renders (SVG/HTML) you can send someone; creates viral demo moments. Deliberately still `[ ]`: the shipped **Knowledge Graph Visualization** (web) is an in-browser interactive view with no exportable artifact, and the docs site's neighbourhood graph is a published page rather than a shareable file. Worth deciding whether that second one supersedes this entry · `crucible-cli` or `crucible-web`

## Workflow Automation

- [x] **Workflow Markup** `P2` — DAG workflows in markdown: `@agent`, `->` data flow, `> [!gate]` · [[Help/Workflows/Workflow Syntax]] · `crucible-core` (parser + engine), `crucible-daemon`
  - **Gets you:** a note with `type: workflow` frontmatter parses `## Step @agent -> out [k:: v]` headings and `> [!gate]` callouts into an executable DAG, and `cru workflow start <note>` runs it. `cru workflow list` / `show` / `start` / `approve` / `status` / `cancel` all exist.
- [x] **Parallel Execution** `P2` — `(parallel)` heading suffix and `&` step prefix for concurrent steps; consecutive parallel siblings join before the next step · [[Help/Workflows/Workflow Syntax]] · `crucible-core` (parser + engine), `crucible-daemon`
  - **Gets you:** two parallel sibling steps genuinely execute concurrently and the next step does not start until both have joined; a failing member fails the workflow *after* the join, reporting all failures; a run of one degrades to plain sequential.
- [x] **Workflow Resume** `P2` — a workflow interrupted by a daemon restart picks up where it stopped · [[Help/Workflows/Index]] · `crucible-daemon`
  - **Gets you:** the non-terminal snapshot on disk is rehydrated on the next `workflow.status` / `approve_gate` / `cancel` call for that session, with parallel-group position preserved.
- [x] **Workflow Authoring** `P2` — guide for creating workflows · [[Help/Extending/Workflow Authoring]]
  - **Gets you:** the authoring guide the entry points at, plus [[Help/Workflows/Index]] and [[Help/Workflows/Workflow Syntax]].
- [ ] **Workflow Markdown Log** `P2` — render a workflow run as markdown. Persistence today is `serde_json::to_vec_pretty` into `WORKFLOW_STATE_FILE`; nothing renders a run · `crucible-daemon`
- [ ] **Session Learning** `P2` — codify successful sessions into reusable workflows. (The Reflection Pass is a different mechanism — it proposes notes, not workflows.)
- [ ] **Fan Step Dispatch** `P2` — implement the reserved `[type:: fan]` on the existing delegation subsystem rather than new machinery: each child of a fan step runs as a delegated child session (`delegate_session` already provides depth caps, trust resolution, `max_concurrent_delegations`, and result truncation), which gives parallel groups genuinely concurrent LLM turns and makes `@agent` hints actually route — the two gaps the engine warns about today. The other half is dynamic cardinality: one child per item of a prior step's output, which is what separates `fan` from `(parallel)` groups, whose fan-out is fixed when the document is written · `crucible-daemon`, `crucible-core` (workflow)
- [ ] **Ralph Step Dispatch** `P2` — implement the reserved `[type:: ralph]`: repeat a step's inline turn until the workflow's runnable `## Validation` entries pass or a bounded attempt count is hit, reusing the `workflow.assessed` command runner as the loop predicate. The syntax page already names Validation as ralph's default pass-criterion; only the loop is missing · `crucible-daemon`, `crucible-core` (workflow)
- [ ] **Typed Step Outputs** `P2` — an optional schema attribute on `-> name` steps, validated against the captured response with bounded retry before the value binds into the output scope; plus a warning when a `**bold**` interpolation token matches no scope key — today a typo'd output name silently passes through as ordinary bold text, the quietest failure in the engine · `crucible-core` (workflow), `crucible-daemon`
- [ ] **Adversarial Verification Step** `P3` — a `[type:: verify]` stdlib handler (built over fan once it lands): N independent child turns each prompted to *refute* the previous step's output; majority refutation fails the step with the refutations as the error. Encodes verify-before-trust as a workflow primitive instead of author discipline — a workflow engine that records a step complete because the agent produced text is trusting exactly the thing that most needs checking · `crucible-daemon`
- [ ] **Workflow Run Budget** `P3` — a frontmatter or start-time token ceiling for a run; the engine tracks per-turn usage in the `WorkflowSnapshot` and, at the ceiling, pauses at a synthetic gate rather than hard-failing, so a human decides whether to spend more. Matters most alongside fan, where fan-out is no longer bounded by the document · `crucible-daemon`

## Storage & Processing

- [x] **SQLite Backend** `P0` — the default and only storage backend · [[Help/Config/storage]] · `crucible-daemon` (storage)
  - **Gets you:** notes written into a kiln are parsed into SQLite and returned by `list_notes` / `get_note_by_name` over the daemon socket, across the notes, note_links, entities, properties, relations, blocks, tags and entity_tags tables.
- [x] **Vector Embeddings** `P0` — FastEmbed (ONNX) local embedding generation · [[Help/Config/embedding]] · `crucible-daemon` (llm)
  - **Gets you:** a real 384-dim finite embedding vector per text, with `batch_size` reaching the actual inference call. **Not on by default**, despite `EmbeddingProviderConfig::default()` being FastEmbed: `CliAppConfig.enrichment` is an `Option` with no `Some(default)` fallback, so a config with no `[enrichment]` section gives the daemon no embedding provider at all — and neither `cru init` nor the setup wizard writes one. The error you then hit tells you to set `[embedding]`, a section the loader **hard-rejects as legacy**, so following the message makes the config unloadable. A first-run trap worth fixing at both ends.
- [-] **Embedding Reranking** `P0` — search result reranking for relevance · `crucible-daemon` (storage)
  - **Gets you:** nothing — the feature was removed. The dead `FastEmbedReranker` module (formerly `llm/reranking/`) had zero call sites outside its own module and no `rerank` RPC method, so nothing in any surface could reach it; it was deleted in the 2026-08 dead-code sweep. Reranking would need to be built fresh if wanted.
- [x] **File Processing** `P0` — parse, enrich and index notes via a pipeline · [[Help/CLI/process]] · `crucible-daemon`
  - **Gets you:** `cru process` / `process_file` / `process_batch` parse a markdown file and land it in the index. Parse and index are unconditional; "enrich" is conditional on an embedding provider being configured (see **Vector Embeddings**), which by default it is not.
- [x] **Hash-based Change Detection** `P0` — content-addressable block hashing · `crucible-core`
  - **Gets you:** re-processing an unchanged file is skipped, and `force_reprocess` overrides the skip.
- [-] **Transaction Queue** `P0` — batched database operations with consistency · `crucible-daemon` (storage)
  - **Gets you:** nothing by that name. Writes are **per-note transactional** — `SqlitePool::with_transaction` is a plain `BEGIN TRANSACTION` closure wrapper with three call sites, each one note per transaction. `process_batch` batches *file processing*, not database operations.
- [-] **Task Storage** `P0` — task records, history, dependencies, file associations · `crucible-daemon` (storage)
  - **Gets you:** nothing in storage. Tasks are a markdown harness — see **Task Harness (`TASKS.md`)** under Note-Taking & Authoring, which is the real, working capability.
- [x] **Kiln Statistics** `P0` — `cru stats` file and size metrics · [[Help/CLI/stats]] · `crucible-cli`
  - **Gets you:** total files, markdown files, total size in KB and the kiln path, as text or with `-f json`.
- [-] **Indexed Note & Link Metrics** `P0` — note counts from the index and link analysis in `cru stats` · `crucible-cli`
  - **Gets you:** neither. `KilnStats` has exactly three fields (`total_files`, `markdown_files`, `total_size_bytes`) and its collector is a plain recursive directory walk — so a kiln with 500 unindexed files reports 500, and no link analysis happens anywhere in `cru stats`.
- [x] **Daemon Server** `P0` — Unix socket JSON-RPC server; `daemon.capabilities` → `methods` is the live method list · `crucible-daemon`
  - **Gets you:** every `cru` subcommand and every web route talking to one process over a socket. The method count is served at runtime rather than carried here — the two counts this document used to hold (35 and 55) were both wrong and disagreed with each other.
- [x] **Daemon Client** `P0` — auto-spawn, version check, RPC client library · `crucible-daemon` (rpc)
  - **Gets you:** any `cru` subcommand transparently spawns the daemon if it isn't running, and shuts down and respawns one whose build SHA does not match. Note "reconnect" is **web-only** — `crucible-web` does a generation-guarded reconnect that also rewires the SSE router; the CLI/TUI client has none, so a daemon restart mid-session leaves the TUI on a dead socket.
- [x] **Event Subscriptions** `P0` — per-session and wildcard event streaming · `crucible-daemon`
  - **Gets you:** a client subscribing to a session id (or `"*"`) receives that session's events over the socket as they fire; an event addressed to `"*"` reaches everyone.
- [x] **Notification RPC** `P0` — add, list and dismiss notifications via the daemon · `crucible-daemon`
  - **Gets you:** `session.add_notification` / `list_notifications` / `dismiss_notification` add, return and remove notifications across toast, progress and warning kinds, and the TUI renders them.
- [x] **File Watching** `P0` — native file change detection (notify, debouncing, daemon bridge), with one OS watcher shared per watch group and auto-reprocessing: `file_changed` events trigger `pipeline.process()` via the daemon reprocess task; enrichment disabled for now (parsing + storage only) · `crucible-daemon` (watch)
  - **Gets you:** a note you create, edit, or delete while the daemon is running is indexed on its own, without `cru process`.
- [-] **Storage Maintenance Commands** `P0` — `storage.verify`, `storage.cleanup`, `storage.backup`, `storage.restore` and the `cru storage` command module · `crucible-daemon` (storage), `crucible-cli`
  - **Gets you:** the RPCs are dispatched and reachable from the CLI, which is a real user-visible capability set with no prior entry at all.
- [x] **Git / SCM Project Integration** `P0` — `scm.clone` in the daemon; branches and worktrees in the bundled `worktree` Lua plugin · `crucible-daemon`, `crucible-lua`, `crucible-web`
  - **Gets you:** create a project from a remote repo URL, and back a workspace-target picker on the web composer — pick a branch to jump to its worktree or create one from the `[plugins.worktree] template`. N sessions across N worktrees without leaving the composer. Config knobs are `[workspace] root_dir` / `session_scratch_dir`, plus `[plugins.worktree] template` (worktrees moved to the plugin).
  - **Corrected twice on 2026-08-18.** First pass flagged that this entry cited a test named `collect_branches_and_add_worktree_against_real_git` which is in no file, and concluded the two RPCs were "shipped and unproven". That conclusion was wrong: `scm.branches` and `scm.worktree_add` **no longer exist**. They were removed from the daemon and reimplemented in the bundled `worktree` plugin, which states it in its own header (`runtime/plugins/worktree/init.lua`:15-18 — "Zero Rust git knowledge … What this replaces: `scm.rs`'s `add_worktree` and `collect_branches`, the `scm.branches` / `scm.worktree_add` RPCs"). The web stopped parsing git too (`web/src/lib/api.ts`:623). The missing test was missing because the code it tested was deleted — a citation surviving its subject, which is the failure mode this map's proof lines exist to catch, caught one layer late.
  - **What is left in Rust, and why:** `scm.clone` plus `normalize_clone_url` (the one git URL validator, which the plugin bootstrap also uses), `sanitize_repo_name`, `validate_clone_dest` and `resolve_workspace_root_dir`. These are URL hardening and path containment — `validate_clone_dest` refuses a symlink hop out of the workspace root dir — and belong beside `fs_scope.rs` and `protected.rs` rather than in a plugin a user can shadow. The git *invocation* could move; the guard around it should not, and splitting the two buys nothing.
- [ ] **Burn Embeddings** `P?` — Burn ML framework for local embeddings. **Removed, not stubbed**: the `EmbeddingProviderConfig::Burn` variant and `BurnEmbedConfig` are deleted with the other backend-less embedding configs (Cohere, VertexAI, Custom), so a `type = "burn"` config now fails at load with an error that lists the supported types · `crucible-daemon` (llm)
- [ ] **LlamaCpp Embeddings** `P?` — GGUF model inference for embeddings · `crucible-daemon` (llm)
  - **Gets you:** nothing — settled and removed. The formerly-orphaned `llm/embeddings/gguf_model.rs` and `inference.rs` were dead code left by the Burn removal (nothing constructed them outside their own tests) and were deleted in the 2026-08 dead-code sweep.
- [ ] **Session Compaction** `P?` — compact sessions with cache purge for memory efficiency. **Worse than unimplemented — an active hazard that fires automatically**: auto-compaction trips at 0.95 of `context_budget` and calls `request_compaction`, which sets `SessionState::Compacting`; because nothing consumes that state the session is stuck in it — `session.list` reports `compacting`, `session.pause` then fails its `state != Active` guard, and a later `session.compact` returns `InvalidState`. See **Auto-Compaction** · `crucible-daemon`

## Configuration & Setup

- [-] **Config System** `P0` — TOML config with `{file:}` / `{dir:}` / `{env:}` value references and CLI-flag overrides · [[Help/Configuration]] · `crucible-core` (config)
  - **Gets you:** substitution **inside values** (`{file:}`, `{dir:}`, `{env:}`) plus three CLI-flag overrides — priority is CLI flags → config file → defaults, with env absent as a layer. `ValueSourceMap` (`cru config show --trace`) tracks File/Cli/Default and is live.
  - **Was claimed, now deleted rather than fixed:** profiles, `[include]`, and environment overrides never ran. `profiles` lived on a `Config` struct reachable only through a `ConfigLoader` with no production caller; `merge_includes` was called from exactly one place, inside that same dead loader; there was no environment-override pass anywhere. That cluster — `loader.rs`, `Config` and its unreachable `validate_*` methods, `profile.rs`, and `IncludeConfig` — was ~1,500 lines and is gone. Note also `crucible_core::config::AppConfig`, which CLAUDE.md names as the canonical config type, does not exist as a struct or alias — that doc still needs the same correction.
- [x] **Provider Config** `P0` — `[llm.providers.<name>]` type / endpoint / api_key / default_model across nine backends · [[Help/Config/llm]] · `crucible-core` (config)
  - **Gets you:** which backend a new session talks to, and the model list users pick from. Nine backend types exist in `[llm.providers]` (Ollama, OpenAI, Anthropic, Cohere, VertexAI, OpenRouter, GitHubCopilot, ZAI, FastEmbed); VertexAI and FastEmbed serve embeddings only — no chat adapter maps to them. A provider carries no `temperature` and no `max_tokens`: both are per-model inference settings and genai defaults them for the model being called. Legacy `[providers]`, `[embedding]` and `chat.provider` are hard-rejected at load with actionable errors.
- [x] **Embedding Config** `P0` — `[enrichment.provider]` type, model and batch size · [[Help/Config/embedding]] · `crucible-core` (config)
  - **Gets you:** which backend embeds and how many texts go per request — `batch_size` sizes the real HTTP request for Ollama (and switches to the legacy single-request endpoint at `<= 1`) and threads into `model.embed` for FastEmbed.
- [x] **Pipeline Tuning Knobs** `P0` — `[enrichment.pipeline]` holds one knob · `crucible-core` (config)
  - **Gets you:** `max_precognition_chars` (default 3000) bounds the precognition context budget. The eight inert knobs (`worker_count`, `batch_size`, `max_queue_size`, `timeout_ms`, `retry_attempts`, `retry_delay_ms`, `circuit_breaker_threshold`, `circuit_breaker_timeout_ms`) are deleted; an old config that still sets them loads, and the values are ignored.
- [ ] **Storage Config** `P0` — backend selection, embedded vs daemon mode. **Removed**: the daemon is the only storage backend, so the section configured nothing · [[Help/Config/storage]] · `crucible-core` (config)
  - **Gets you:** nothing to configure. `StorageConfig` and `CliAppConfig.storage` are deleted; an old config with a `[storage]` section loads, and the daemon ignores it. `cru init` no longer writes a `[storage]` section into new kiln configs.
- [-] **MCP Config** `P0` — upstream MCP server connections · [[Help/Config/mcp]] · `crucible-core` (config)
  - **Gets you:** upstream servers in the TUI's `:mcp` list with a tool count. Their tools reach no agent, and web sessions get not even the list.
- [-] **Project Config** `P0` — attached-kiln declarations in `.crucible/project.toml` · [[Help/Config/Workspaces]] · `crucible-core` (config)
  - **Gets you:** nothing from the `kilns = [...]` table. It is parsed, test-asserted, and ignored.
- [-] **Agent Config** `P0` — default agent · [[Help/Config/agents]] · `crucible-core` (config)
  - **Gets you:** nothing yet. "Default agent" does not reach the model. An ACP agent runs its own turn loop, so the daemon refuses daemon-side generation settings outright rather than caching a value the agent process never sees.
- [x] **Project Registry** `P0` — directories register as projects (`project.register` / `list` / `get` / `unregister`); `.crucible/project.toml` carries attached kilns and `[security]` policy · `crucible-daemon`
  - **Gets you:** sessions, the web root dropdown, and search containment are all project-scoped — a grep root outside every registered project is rejected, and a subdirectory of one is allowed. This is the third of the three load-bearing terms (Project / Kiln / Workspace) and had no entry at all before. A registered kiln root is never a project: registration refuses it, `list` and `get` leave an existing entry out, and a session whose workspace is a kiln still gets created.
- [x] **CLI Commands** `P0` — top-level subcommands, notably `chat`, `session`, `search`, `process`, `stats`, `config`, `daemon`, `web`, `mcp`, `acp`, `lua`, `plugin`, `skills`, `tasks`, `workflow`, `agents`, `models`, `storage`, `init`, `setup` · [[Help/CLI/Index]] · `crucible-cli`
  - **Gets you:** all of them dispatch and produce output. The count is no longer carried in the description because it drifts every release — the previous "16 command modules" was stale by 1.75×.
- [x] **Init Command** `P0` — `cru init` project initialization with path validation · `crucible-cli`
  - **Gets you:** `.crucible/` created with `kiln.toml`, `project.toml` and a Lua `init.lua`, refusing hard-blocked paths. It writes no TOML config: the kiln name goes to `kiln.toml`, the provider selection to the daemon's `llm.json`.
- [-] **Setup Wizard** `P0` — first-run wizard on `cru chat` when no kiln exists · `crucible-cli`
  - **Gets you:** a first-run prompt, but not the one described. Three of the four clauses are wrong: it is a `dialoguer` stdio wizard, **not** an Oil TUI one; it triggers on bare `cru`, **not** on `cru chat`; and it keys off a missing *config file*, not a missing kiln. `cru chat` does prompt for a kiln separately, and that path has no provider detection or model selection — it hardcodes `("ollama", "llama3.2")`.
- [x] **Kiln Discovery** `P0` — git-like upward `.crucible/` search · `crucible-cli`
  - **Gets you:** running `cru` inside a directory under a kiln finds that kiln by walking up, with `$CRUCIBLE_KILN` as a fallback. The **effective** order is config file → ancestor walk → `$CRUCIBLE_KILN`, not the "CLI flag → ancestor walk → env var → global config" this entry used to claim: both production callers pass no CLI flag or global path, there is no top-level `--kiln-path`, and `ensure_valid_kiln` returns early on a configured kiln *before* calling `discover_kiln`.
- [x] **Kiln Path Validation** `P0` — hard blocks (root, nested kiln), strong warnings (git repo, source project, home dir, tmp), mild warnings (cloud sync) · `crucible-cli`
  - **Gets you:** `cru init` refuses `/` and nested kilns outright and *prints* the warning cases. Two real holes: it never asks for confirmation — the tiered design's "strong warning = default deny" gate was never built, so a warning is informational and init proceeds regardless; and the `cru chat` auto-create path creates a kiln at a user-supplied path **without calling the validator**, so the first-run route bypasses every hard block and warning — a user can be walked into creating a kiln at `~` or inside a git repo by the flow that is supposed to guard it. "Shared validation layer" also overstates it: there is exactly one caller.
- [x] **CLI Help & Discoverability** `P0` — `long_about` with examples on every top-level subcommand; `infer_subcommands` so unique prefixes resolve; clap's "did you mean" on typos · `crucible-cli`
  - **Gets you:** `cru --help` lists every subcommand with example-bearing help, `cru con show` resolves to `cru config show`, and typos get suggestions. All 28 top-level variants carry `long_about`; nested subcommands were not counted, so read the claim as "every top-level subcommand".
- [x] **Getting Started** `P0` — installation and first steps · [[Guides/Getting Started]] · [[Guides/Your First Kiln]]
  - **Gets you:** both guides present in the docs kiln.
- [x] **Platform & Provider Guides** `P0` — Windows setup, GitHub Copilot, OpenRouter, Z.AI, Basic Commands, Session Search · [[Guides/Windows Setup]] · [[Guides/GitHub Copilot Setup]]
  - **Gets you:** six guides, not the two this entry used to name — `docs/Guides/` also ships `OpenRouter Setup.md`, `Z.AI Setup.md`, `Basic Commands.md` and `Session Search.md`.
- [x] **Plugin Loading Errors** `P0` — `:plugins` shows load status; failures surface as toast notifications with error details · `crucible-lua`, `crucible-cli`
  - **Gets you:** each plugin printed with a state glyph and, on failure, `✗ name v0.1.0 (error: <message>)`, plus a notification you actually see. (It does not show provenance — see **Provenance in `:plugins`**.)

## Web & Desktop

> Builds on the HTTP gateway. The web UI is a **mostly-thin client to the daemon** — but no longer purely one: `crucible-web` holds SSRF validation on session endpoints, path-traversal containment on kiln reads, note-upsert gating, layout persistence to its own file, PTY lifecycle, and the wire-shape normalization without which no permission prompt renders at all. Serve over Tailscale/Cloudflare Tunnel for self-hosted remote access; PWA for mobile without app-store friction.
>
> **Design principles**, as they stand after the 2026-07-30 reconciliation:
> 1. **Gateway-centric** — daemon owns state; the web is a thin view layer for everything it can be.
> 2. **Multi-session supervision** — the Inbox, attention markers and notification routing exist so work in a non-focused tab is not silently lost. (The original "agent inbox is the landing page" is not what shipped: a fresh load lands on an empty center pane by design, and Inbox is one panel among many.)
> 3. **Knowledge graph is the differentiator** — visual graph exploration no competitor has in-browser.
> 4. **Web extensibility is TypeScript, not Lua** — reversed from the original principle by the 2026-07-26 asymmetric-extensibility decision: Lua covers behavior and the TUI, TypeScript covers the web, and the shared contract is data rather than widgets. Panels register in `web/src/lib/register-panels.tsx`.
> 5. **Good API docs** — an interactive playground is unstarted; the honest version is tracked as `OpenAPI Spec` below. The de-facto contract lives in `crates/crucible-web/tests/route_contract_tests/` (15 files across ~22 route modules).

### Foundation UI

- [x] **Static File Serving** `P1` — Axum serves the SolidJS bundle (PWA manifest + service worker) from `dist/` via rust-embed; static routes are public · `crucible-web`
  - **Gets you:** browsing to the server returns the app, and unknown non-asset paths fall back to `index.html` so deep links work. Two nuances: the embedded bundle is the default in **every** build profile (asset source is configuration, not optimization level — the old `debug_assertions` branch baked the build machine's absolute path into the binary), and `--static-dir` resolves **relative to cwd**, a foot-gun that has previously caused a dev server to silently serve a stale bundle.
- [x] **Web Chat UI** `P1` — SolidJS chat: streaming, markdown, tool cards, permission modals · `crucible-web`
  - **Gets you:** you type in the browser and see streamed tokens, a thinking block that streams OPEN — the reasoning text visible as it lands, a caret at its growing edge, folded back to a quiet "Thought for N tokens" line when it settles — tool-call cards with args and results, token counts, and a permission card you can answer — plus subagent and delegation cards, a precognition badge, per-segment message bubbles, a context-usage meter, a mode control and export. Since 2026-09-18 a message typed while a turn streams is a QUEUE, not a refusal: the composer stays live, the prompt renders below the streaming block marked queued, and the queue dispatches one turn at a time when the stream goes idle; a send the daemon refuses as concurrent re-queues instead of erroring, a cancelled turn closes its thinking block, and a mid-turn user_message echo lands at the end of the streaming block instead of inside it. Since 2026-09-15 the permission or ask card DOCKS on the prompt instead of sitting in the transcript, which keeps a one-line record of the open request; a request the daemon still holds comes back after a reload; each turn's footer is always visible and carries copy, regenerate and the time the turn took; and the chat column measures 64rem.
- [ ] **Trajectory Inspector** `P2` — a second tab beside Chat that renders the session's own event log as an inspectable list · `crucible-web`
  - **Gets you:** the answer to "what actually happened" without leaving the session. The Chat tab is the readable story; this is the record it was built from — system prompts, context injections, tool calls with their arguments and results, and the token ledger of every request. Prior art is DeepSeek Harness's Trajectory tab, whose stated rule is *model-visible means logged*: anything that reached a model request must be reconstructable from the log.
  - **The backend already ships.** `GET /api/session/{id}/history?limit=&offset=` returns the raw `SessionEventMessage` records with `seq` and `timestamp`, plus `total_events` (`crates/crucible-web/src/routes/session/mod.rs:88`, handler at `:608`; client type `DaemonHistoryEvent` at `web/src/lib/api.ts:991-1031`). The payloads already carry what the view needs: `tool_call` has `call_id, tool, args, description, source`; `tool_result` pairs by `call_id`; `message_complete` carries `prompt_tokens, completion_tokens, total_tokens, cache_read_tokens`. **No new route and no new type.**
  - **One row per event does not work, and the data says why.** A 1.8 MB session on disk (`chat-2026-07-30T0011-1xrk52`) holds 9,748 events of which **9,403 are `thinking` deltas — 96%**. Coalescing each delta run into one collapsed row per `message_id` leaves ~345 rows, which needs no virtualization. Pair each `tool_call` with its `tool_result` by `call_id` into a single row (`args → result`); a split pair is what makes these logs hard to read. Note `seq` reaches 15,315 for 9,748 lines, so the daemon already drops events at write time — the log is a reduced stream, not a raw one, and the view must not claim otherwise.
  - **Shape:** master-detail. A dense single-line list on the left, each row badged by kind (`SYSTEM`, `USER`, `CONTEXT`, `ASSISTANT`, `TOOL`) and addressed as `Turn N · Step N`; a detail pane on the right with Summary / Payload / Result / Schema / Timing, where Schema shows the tool description the model was actually given and Timing names its own source. A three-lane timeline (Input / Model / Tools) sits above the list as a minimap, which is what "inspect by source" looks like when drawn. Filter chips over kind and `source`, and a search box over the summary line.
  - **Deliberately not:** replay, fork-from-here, or editing. It is an inspector. `session_resume_from_storage` already backs replay if that is ever wanted.
- [x] **Flexible Panel System** `P1` — dockable, splittable, poppable panel layout with server-side persistence · `crucible-web`
  - **Gets you:** you drag tabs between left/right/bottom/center zones, split and nest center panes, pop a tab out to a floating window and dock it back, and the layout survives a reload — persisted **server-side** through `GET/POST/DELETE /api/layout` to a file on disk, so it follows you across browsers. The model is a binary split tree in a versioned layout (v10), not a fixed 4-edge dock. A session opens in the centre pane beside the sessions rail and a file beside the files rail, so the centre reads sessions | editor whichever side the rails sit on, and reopening a session moves its tab back beside the rail. On a fresh shell the one empty centre pane IS that pane, so the first session occupies it rather than splitting it, and the editor pane appears with the first file. Swap Side Panels carries every field with the pane it moves, collapse included, and no longer remounts: the shell row and each split render as keyed lists, so a flip is a reorder and a transcript keeps its state.
- [x] **Navigator / Scope Switching** `P1` — ribbon-hosted panel with a projects/kilns/sessions swapper and an inline search takeover · `crucible-web`
  - **Gets you:** you switch between projects, kilns and sessions and start a new session from one place. **There is no breadcrumb and no header bar** — the shell controls live in the ribbons, and a test asserts the header bar's absence, so re-adding one would now break it.
- [x] **File Tree** `P1` — accessible tree over project files and kiln notes with drag-to-move and context actions · `crucible-web`
  - **Gets you:** you browse a single tree under a root dropdown that selects among registered projects and kilns, open files into center tabs, drag files to move them (backed by the link-preserving `fs.move`), and get rename/delete/new-note context actions with extension-based icons. Move is deliberately drag-only, not a menu entry.
- [x] **CodeMirror 6 Editor** `P1` — multi-file tabs with dirty indicator, language detection, save via API · `crucible-web`
  - **Gets you:** you open a file in a tab, edit it, see a dirty indicator, save with Ctrl-S or `:w`, and the bytes land on disk. Also shipping in this surface: **vim keybindings**, wikilink autocompletion and hover previews, frontmatter card rendering, and table auto-formatting.
- [x] **Live Preview & Reading View** `P1` — Obsidian-style live preview as the markdown editing default, with a matched reading view · `crucible-web`
  - **Gets you:** callouts, fenced-code highlighting, task-list checkboxes, tables, images, sanitized embedded HTML, a Properties card for YAML *and* TOML frontmatter, and wikilink following with hover previews that tear off into floating editor windows.
- [~] **Inline Diff Review of Agent Edits, retired 2026-09-21** — the "Open in editor" merge overlay on a tool card, with its per-hunk Accept/Reject in the editor gutter, is gone · `crucible-web`
  - **Why:** the overlay changed only the browser's copy of the file, and the daemon no longer holds a write for review. The tool card's **Open diff** opens the session record in the diff pane, at the file of the call (see **Diff Pane**). A write that needs a decision is a proposal.
- [x] **Review Comments** `P1` — `diff.comment` and `diff.resolve_comment`, naming their diffset by `source` · `crucible-daemon`, `crucible-web`
  - **Gets you:** a comment anchors to a root and a line range and comes back with the diff; a comment without a body names the missing field; `diff.resolve_comment` marks a comment and refuses an unknown id. A message attaches stored comments: `session.send_message` takes `comments` (id and diffset source), and an `@comment:<id>` mention in the text does the same for a client with no comment UI, such as the TUI. The daemon builds one `<system-message kind="review-comment" source="…">` block for each, where `source` is `human` or `agent`, the author of the comment — the file, the root when it is not the workspace, the range (with "(before)" for a base-side range), the text and a unified hunk — and injects it as context before the user turn, so replay and fork keep it. An unknown or resolved comment refuses the message. The web routes forward each call to the daemon. The session record is read only: no file offers Accept or Reject, because the daemon no longer holds a write for review or reverts a change. A write that needs a decision is a proposal, decided in the Inbox, in the diff pane or with `cru proposal` (see **`cru proposal`** and **Diff Pane**).
  - **Retired 2026-09-21:** the RPCs were `review.comment` and `review.resolve_comment`, and comments came back beside the hunks `review.list_hunks` listed; all three names are gone. The **Session / Turn** control that narrowed the listing to the current turn, and the per-hunk read-only merge view each expanded hunk used to mount, are also gone — the diff pane now expands one whole file at a time (see **Diff Pane**).
- [~] **Bulk Review Decisions, Undo and Rebase, retired 2026-09-21** — `review.set_state`, `review.set_states`, `review.undo_reject` and `review.rebase`, with the web Accept/Reject, Accept all/Reject all, Undo and rebase controls, are all gone · `crucible-daemon`, `crucible-web`
  - **Why:** the daemon no longer holds a write for review, reverts a hunk, or undoes a reject. A write that needs a decision uses `propose` mode instead: the Inbox, the diff pane or `cru proposal accept|reject` decide it, and a rejection keeps its reason without touching a file. The **Session / Turn** scope control this entry used to name is gone too (see **Review Comments**).
- [x] **A Review Root Outside Git** `P1` — a kiln or workspace the daemon cannot ask git about is snapshotted into a plain content store, so it gets a ledger like any other root · `crucible-core`, `crucible-daemon`
  - **Gets you:** a root outside a git repository used to be skipped, and a session with only such a root had no review at all — which is exactly the shape a kiln takes, and exactly what a plugin pass writing kiln notes needs. A root is now snapshotted as a manifest of one content hash per file, blobs stored once by hash under the daemon data root, with a `(size, mtime, inode)` stat key so an unchanged file is not read again. One id type names both stores: a bare hex id is a git tree, a `plain:` prefixed one is the plain store, so every journal already on disk still replays and a git call handed a plain id answers an error rather than panicking. The daemon claims a plain root's snapshots with one keep file per root and sweeps the store on the same tick as the git keep refs, because nothing outside the daemon collects them.
- [x] **Diff Pane** `P1` — one `DiffPanel` renders a branch diff, a session's review record, or a proposal, each a `DiffsetSource` the daemon computes and the panel fetches lazily · `crucible-daemon`, `crucible-web`
  - **Gets you:** "Open branch diff" in the Files panel, a proposal row in the Inbox, on its note, or in the Changes panel, and a session's own review record each open the same pane with the matching source. A file in the Changes panel and **Open diff** on an Edit/Write tool card open the session record at that file: the pane expands the file and scrolls to it, also when the pane is open already. Every file section shows its added/removed counts and starts expanded or collapsed by size; large unchanged regions fold to 3 lines of context (at least 4 lines before folding), and "Collapse all" closes every section at once. Clicking a line number starts a comment range; the box that opens offers one action, **Comment**, which stores the comment through the daemon and attaches it to the chat the pane header names — a session record names its own session, another diffset names the session of the caller that opened it, else the session that was active then. The composer of that chat shows a chip (`server.rs L17–19`, or `lib.rs L1 (before)` for a base-side range), and the next message carries the reference. A pane with no chat stores the comment and says that no chat takes it. The "Send to chat" button that inserted an `@path:line` mention is gone: it stored nothing, it lost the text when no chat was focused, and it sent current-side numbers for a base-side range. "Copy comments" puts every open, unresolved comment on the clipboard in the quickfix form (`path:line: text`), for `vim -q`. A proposal's pane adds Accept all / Reject all in the header and Accept/Reject on each file, and a conflicted file shows a merge view with "Accept resolution" in place of its diff.
- [x] **Model Picker** `P1` — Cursor-style dropdown below the textarea; switch model during a conversation · `crucible-web`
  - **Gets you:** the picker opens, shows available models, and switching one calls the API mid-conversation.
- [x] **Session Auto-Naming** `P1` — an untitled session renames itself once the daemon titles it · `crucible-daemon`, `crucible-web`
  - **Gets you:** a fallback label until the title arrives, then a rename that propagates everywhere in the UI. Titling is **daemon-triggered** (on the first completed turn) and **plugin-generated** (`auto-title`); the browser only renders the `title_changed` SSE event — so this is not SolidJS work as the entry used to claim.
- [x] **Agent Inbox** `P1` — one panel listing every session waiting on you, answerable in place · `crucible-web`
  - **Gets you:** pending permissions answered without switching tabs, a PROPOSALS section with every proposal that waits for a decision, recent sessions by activity, an archived section, and an all-clear state when nothing is pending. It is **one panel among many**, not the landing page.
- [x] **Permission Approval UI** `P1` — approve or deny from the browser with a diff preview and scope choice · `crucible-web`
  - **Gets you:** the agent asks, the browser shows a card docked on the prompt with the old-vs-new diff for a write, and Allow/Deny with a scope choice posts back and clears it. The transcript keeps a one-line record of the open request, the card survives a page reload while the daemon still holds the request, and the answer sits where the user types. Queued requests open one at a time, and the same requests are answerable from the Inbox.
- [x] **Session Management** `P1` — list, create, open, resume, archive, unarchive, delete and export sessions from the browser · `crucible-web`
  - **Gets you:** all of the above plus session scope changes (attach/detach kiln; the project is fixed at creation and its chip is static) and draft-surface lazy creation including ACP agents and kiln-less sessions. Note the lifecycle redesign **removed** the End and "Continue as new session" buttons — tests now assert their absence.
- [x] **Interaction Rendering (web)** `P1` — all 7 `InteractionRequest` variants render in the browser, not 3 · `crucible-web`
  - **Gets you:** `ask_batch`, `edit`, `show` and `panel` are answerable from the browser instead of appearing as nothing. Before this the browser rendered `ask`, `popup` and `permission` only, so a request of any other kind left its caller parked until the timeout with no modal on screen. Responses now state their own `kind`: server-side tag inference cannot separate a panel result from an ask response, because both carry `selected`.
- [x] **Web Terminal** `P1` — xterm.js over a WebSocket-attached PTY, in a bottom panel · `crucible-web`
  - **Gets you:** a real interactive shell in the browser — WebGL renderer, vector-drawn powerline/box glyphs, `COLORTERM=truecolor`, configurable font that applies live, starting in the server's launch directory, reconnecting after a socket drop, with a concurrency cap on upgrades. Remote access is opt-in and **fail-closed**: `cru web --remote-shell` / `[web] remote_shell = true` serves it to authenticated non-localhost clients, and without an API key the opt-in is ignored. This is the only WebSocket surface in the web app and the most security-load-bearing one.
- [x] **Composer Autocompletion** `P1` — `/` command and `[[` wikilink completion in the chat composer · `crucible-web`
  - **Gets you:** typing `/` opens a popup of the daemon's real command set, narrowing as you type and inserting the command (leaving a space for commands taking an argument); typing `[[` completes to a closed wikilink. A path separator is not treated as a trigger, and a failed command fetch keeps the popup closed and retries on the next keystroke.
- [x] **Command Palette & Note Switcher** `P1` — Ctrl+P for panels/files/actions, Ctrl+O for a note quick switcher · `crucible-web`
  - **Gets you:** the primary navigation surface of the web app. Every registered panel has an "Open …" command, so a closed graph/terminal/backlinks window can always be brought back; the note switcher is recency-sorted with path subtitles and scored subsequence fuzzy matching, and `[[` and `>` cross between the two mid-typing.
- [x] **Notifications & Attention Routing** `P1` — background sessions that need you raise a toast and a per-session attention marker · `crucible-web`
  - **Gets you:** a pending permission, error or completion in a non-focused tab is not silently lost — this is the multi-session glue the Inbox alone does not provide.
- [x] **Precognition, Subagent & Delegation Surfaces** `P1` — the browser shows which notes precognition injected, and renders subagents and delegations as their own cards · `crucible-web`
  - **Gets you:** the stated differentiator is visible rather than invisible — you can see what context the agent was given, and watch spawned subagents and cross-agent delegations with completion/failure state.
- [x] **Voice Input / Transcription** `P1` — record audio from the composer and get it transcribed into the message box · `crucible-web`
  - **Gets you:** dictation into the chat composer, configurable from Settings.
- [x] **Design Tokens & Style Gate** `P1` — a test fails the build if a component reaches for a raw palette class, a raw visual value, an unringed focus or a size under the floor · `crucible-web`
  - **Gets you:** a UI that reads as one system in light and dark, with structural surfaces animating through shared motion primitives. It is also why the visual playwright baselines can be as tight as 0.3–4% diff ratios. Since 2026-09-15 the whole palette is a public `--cru-*` contract in `@layer cru-theme`, so a plugin restyles the app with one stylesheet and no `!important` — see [[Help/Extending/Web Theme Tokens]]. One `EmptyState` component draws every empty pane, with no icon and an error tone for a failed fetch; a focus ring reaches every interactive element; nothing renders below 11px; a badge colour carries one meaning; a tab title caps at 200px and elides from the middle; a wikilink reads the same in both themes and in both surfaces.
- [x] **PWA Support** `P1` — manifest + service worker; installable from the browser, mobile access without an app store · `crucible-web`
  - **Gets you:** an installable app whose update surfaces as a prompt rather than reloading mid-turn (`registerType: 'prompt'`, `skipWaiting: false`), with the service worker forbidden from intercepting `/api/*` including the SSE stream. Debug builds ship a self-destructing SW.

- [ ] **Mobile Shell** `P2` — a second app shell on the same origin: editor as the main area, sessions and files as tabs in the left drawer (with a project switcher), backlinks in the right drawer, no terminal, vim mode off by default with its own setting · `crucible-web`
  - **Gets you:** a phone surface that is not the desktop layout squeezed — the desktop shell assumes a wide viewport (measured: at 780px the centre column collapses to 29px and the terminal renders one column), and making it responsive was explicitly rejected in favour of a separate shell.
  - **Why it is cheap:** the seam already exists. `web/src/lib/panel-registry.ts` registers panels as bare components and `Pane.tsx` renders them through `<Dynamic>` with no pane or tab context, so all 15 panels are directly mountable by another shell; only two files in the tree import `windowing/`. Deliberately NOT a `/m` route or a second bundle — two PWAs on nested paths of one origin is what Chromium's web-apps team calls "strongly not recommended" (two installs, link capture, misattributed notifications), and `vite-plugin-pwa` cannot emit two scoped service workers.
  - **Scope:** no longer online only. The offline store shipped beside it — a kiln kept whole, a mirror, an outbox with one entry per note, and the Offline settings group — and a note write is merged rather than refused when it goes stale (see **Note Merge and Conflict Resolution**).
  - **Design:** [[Mobile Shell]] (`docs/Meta/Architecture/`), a draft.

- [ ] **Offline Kiln Cache** `P3` — keep a whole kiln on the phone, chosen per kiln, so the mobile shell reads it with no connection · `crucible-web`
  - **Gets you:** notes readable on a phone that is out of signal, which is most of the value of installing the PWA at all.
  - **The decision it was blocked on was made 2026-09-11:** a whole kiln, chosen per kiln, kept until the user turns it off; nothing wiped on a lapsed key. See the decision log and `docs/Meta/Architecture/Mobile Shell.md` section 11. What remains is the work, and the note-write primitive it depends on.
  - **The original block, for the record:** the service worker deliberately has **zero** `runtimeCaching` and precaches only the app bundle, so no API response is ever stored. Caching kiln content means putting authenticated responses in a same-origin-writable store, which is the threat `web/src/test/pwa-scope.test.ts` exists to pin. Needs an explicit answer on what may be cached, for how long, and what happens to it on sign-out — not an incidental config addition.

- [ ] **Offline Note Capture** `P3` — edit and create notes while disconnected, sync on reconnect · `crucible-web`
  - **Gets you:** the actual reason to want an editor on a phone — capture a thought in a tunnel, have it land in the kiln later.
  - **The hard part is not the editor:** it is a second writer. Crucible is plaintext-first with a daemon that owns writes, so a phone queue means an outbox, replay on reconnect, and an answer for a note that changed server-side meanwhile. Scope the conflict behaviour explicitly — the failure mode of getting this wrong is silent data loss, not a broken screen. Sequence it last, after the shell and after read-caching.
  - **The conflict half is answered and shipped (2026-09-14):** see **Note Merge and Conflict Resolution** below. The live drain now has a lost-reply/reload/competing-writer test in `web/e2e/live/conflict.live.spec.ts`, on desktop and mobile, using the real IndexedDB outbox. Both shells show the shared unsent-edit/conflict status and drain on reconnect. Creating a note offline remains open (WS-318 in [[Meta/Web User Stories]]).

- [x] **Note Merge and Conflict Resolution** `P1` — a note write carries the text it was made from, so a write the daemon would refuse as stale is merged against the disk under a per-path lock; what the merge cannot settle waits as a conflict the user resolves region by region · `crucible-core`, `crucible-web`
  - **Gets you:** two writers on one note cost neither of them their text. A save, a task tick and an outbox replay all carry a base hash AND the base text, so the ordinary case — two people changing different lines — merges silently and both changes land. When both changed the SAME lines, nothing is written: the write waits as a conflict holding the merged text and one region per disputed line group, counted apart from the unsent edits, and opened from the phone's badge, its More sheet or the Changes panel. The region blocks offer Keep mine, Keep theirs and Keep both, with the differing words marked. An open buffer also hears the kiln watcher: a clean one re-reads, a dirty one gets a banner with Reload and Merge. This retires the **conflict copy** — a dated second note that nothing listed and that users met by accident or never.
  - **Docs:** [[Help/Concepts/Note Sync]]; design in `docs/Meta/Architecture/Mobile Shell.md` section 11 ("The conflict rule").

- [ ] **Deep Links / URI Actions** `P3` — open a note or start a capture from a shortcut or link · `crucible-web`
  - **Gets you:** a home-screen shortcut that jumps straight to a note or a blank capture.
  - **Forward-compatibility constraint, worth honouring before anything ships:** keep the path at `/` and carry the action in the hash (`/#note=…`). `navigateFallbackAllowlist: [/^\/$/]` serves the cached shell for `/` alone, and the manifest `id` is pinned to `/`, so a path-based deep link would work online and fail offline — exactly when a shortcut matters — while widening the allowlist re-opens what `pwa-scope.test.ts` pins. Published URLs are permanent, so the shape has to be right the first time.

### Knowledge & Search (web)

- [x] **Knowledge Graph Visualization** `P2` — interactive force-directed wikilink graph · `crucible-web`
  - **Gets you:** the Graph panel renders an Obsidian-style map of the kiln — drag nodes, hover to highlight, filter by query, toggle orphans and tags — and clicking a node opens the note. Phantom nodes are synthesized for unresolved links.
- [x] **Note Reading & Backlinks** `P2` — read a note with frontmatter, working wikilinks, hover previews and a backlinks panel · `crucible-web`
  - **Gets you:** the note's frontmatter as a card, its wikilinks as working anchors with hover previews, and linked *and* unlinked backlinks in a side panel where one click wraps a mention as a wikilink in the open buffer. A failed request draws an error with the HTTP status and a Retry, not the empty state that used to report a 404 as "Linked mentions (0)". The old entry's "custom columns/sort/filters" framing does **not** ship — that idea now lives entirely in `Structured Data Views`.
- [x] **Search UI** `P2` — one query fanned out over notes, files and sessions, with a Text|Semantic toggle · `crucible-web`
  - **Gets you:** results from all three sources in one panel, a toggle that swaps note results between literal grep and vector similarity, matched spans highlighted, and scoping that drops the other sections. Reachable from the Navigator's search takeover and the command palette. **Property search does not ship** — there is no frontmatter-field query path in the UI.
- [ ] **Structured Data Views** `P3` — Obsidian Bases-style tables and kanban from frontmatter. If built it is a TypeScript panel over the storage query layer, not a Lua extension · `crucible-web`, `crucible-daemon` (storage)

### Artifacts & Rich Content

- [x] **Rich Content Renderers** `P2` — mermaid, KaTeX, syntax highlighting, callouts and copy buttons · `crucible-web`
  - **Gets you:** a ```` ```mermaid ```` fence renders as a diagram (falling back to source on failure and never invoked for a plain code block), `$…$`/`$$…$$` render as KaTeX surviving DOMPurify without treating currency as math, code blocks get shiki highlighting and a copy button, and Obsidian callouts render — in chat, in the reading view, and live in the editor. Mermaid and shiki are lazily imported to keep the bundle down; math and diagram rendering have user-facing toggles in Settings.
- [x] **Skills Panel** `P2` — browse, search and read agent skills in the browser · `crucible-web`
  - **Gets you:** skills grouped by scope, a debounced search that switches to the search endpoint, a drawer with the skill body, a shadow badge when a skill is shadowed, and copy-to-clipboard for its `/name` invocation. **Enable/disable of an individual skill is not exposed.**
- [x] **Plugin Manager Panel** `P2` — browse plugins with load state and last error; install, reload and remove them · `crucible-web`
  - **Gets you:** rows from the rich plugin info response, a `last_error` row for a broken plugin and none for a healthy one, an install modal taking a git URL, and an uninstall confirmation that passes the purge flag through.
- [ ] **Agent Artifacts** `P2` — promote a response fragment into its own persistent side panel. Much of the *motivation* is already covered — tool output renders as cards, agent file edits render as reviewable inline diffs, and mermaid/LaTeX/code render inline — so what remains genuinely missing is **extraction** · `crucible-web`

### Configuration & System (web)

- [x] **Settings Panel** `P2` — session model config plus editor, font, API access and transcription settings · `crucible-web`
  - **Gets you:** precognition and results-per-query for the active session (gated on there being one, and on the daemon saying the session supports each); the settings an external ACP agent advertises for itself; editor behaviour (vim keys, autosave, line width, math/diagram rendering, floating save button); fonts for UI, code and terminal; API access for a remote device; and transcription. Two distinct stores: **session** settings go to the daemon over RPC, **UI** settings are client-side. The app config is a third, saved to `settings.json` through `POST /api/config`.
- [x] **System Info** `P2` — daemon health, kilns, MCP status and plugin status · `crucible-web`
  - **Gets you:** `/health` (liveness) and `/ready` (readiness: one `ping` round trip to the daemon, answered `503` when it cannot reply) over HTTP, the kiln list, MCP server status and plugin health (state, source, last error) in the Settings panel. Narrowed from the original entry: **embedding stats do not ship**, and there is no single "System Info" panel — the pieces are scattered across Settings sections and bare HTTP endpoints.
- [-] **SCM Operations (web)** `P2` — list git branches, add a worktree and clone a repo from the browser · `crucible-web`
  - **Gets you:** the routes exist (`GET /api/scm/branches`, `POST /api/scm/worktree`, `POST /api/scm/clone`) over the tested daemon SCM layer.
- [ ] **Embedding & Index Stats** `P2` — surface embedding counts and index health in the browser. Split out of the old System Info entry, which claimed it; no route or panel exposes it today · `crucible-web`
- [-] **Config Editor** `P2` — schema-driven form for the app config. The transport ships: `GET /api/config` serves the daemon's effective config plus a `{key, value, source, file?, line?}` row per leaf, and `POST /api/config` saves through `config.save`, which refuses a leaf the user's `init.lua` holds and names the file and line that hold it. Still missing: a control tree per leaf, and the form that renders it · `crucible-web`, `crucible-core` (config)
- [ ] **OpenAPI Spec** `P2` — machine-readable API spec generated from routes; ship the spec and let users bring Swagger UI / curl / httpie. No `utoipa`, `aide`, `okapi` or `schemars` anywhere in `crates/`. A generator would have a well-tested surface to describe · `crucible-web`
- [ ] **Log Viewer** `P2` — real-time daemon log streaming. No log route exists; the three SSE streams are chat events, filesystem events and per-command shell output. The web terminal lets a user tail logs manually, which lowers the urgency · `crucible-web`

### Canvas & Desktop

- [x] **Canvas** `P3` — infinite spatial workspace over a JSON Canvas file · `crucible-web`, `crucible-core` (canvas)
  - **Gets you:** the Canvas panel opens a `.canvas` file as an infinite surface — text, file, link and group nodes, labelled edges, marquee select, drag and resize with corner and edge handles, connector drawing with snap-to-target, and live sandboxed web embeds on link cards — saved back to disk. Out-of-root references are quarantined without revealing the path, and embeds are sandboxed into an opaque origin. Two things the old entry implied that do **not** ship: agent *sessions* on the canvas, and labelled canvas edges surfacing as graph relations.
- [ ] **Workflow Visual Editor** `P3` — DAG editor for workflow markup. The shipped canvas surface is a plausible substrate if this is ever built · `crucible-web` · depends: [[#Workflow Automation]]
- [ ] **Tauri Desktop** `P3` — native desktop app wrapping the web UI; menu-bar agent status, system notifications. Its stated blocker (a working web chat UI) is now satisfied, so this is genuine open work rather than blocked work; PWA install covers part of the motivation · `crucible-web`

### Browser Extension

> The browser is the one surface the daemon cannot reach. A page behind a login, a rendered
> app, a document in a web editor — `WebFetch` sees none of them, and a headless browser is a
> second identity with none of your cookies. An extension solves this because the **user**
> already has the page open and the browser already trusts them.
>
> **The split that makes this work:** the extension owns the *permissions* — host access, tab
> read, selection, screenshot — and grants them per site and per session. The HTTP gateway
> owns the *tools* — it accepts a grant, and from the grant alone it knows which tools to
> register on the session and what each one may touch. Neither half decides on its own. The
> extension cannot invent a tool; the API cannot reach a page it holds no grant for. This is
> the same shape as [[#Tools & Permissions]] and the ACP host: capability in, tool surface out.

- [ ] **Extension Permission Grant** `P2` — the extension posts a signed grant (origin, tab or iframe id, scope, expiry) to the gateway; the gateway validates it and derives the session's browser tool surface from it. The enumerated-table rule applies — one table maps a grant scope to the tools it admits, with an exhaustive match and no `Default` · `crucible-web`, `extension/` (new) · depends: [[#HTTP Gateway|API Auth]]
- [ ] **Page Sharing to a Session** `P2` — share a tab or a single iframe with a running session; the agent reads the rendered DOM text, the URL and the title through a `browser_read` tool the grant admitted. Read-only by default; a write scope (click, type, navigate) is a separate grant a user must give per site · `crucible-web`, `extension/` (new) · depends: **Extension Permission Grant**
- [ ] **Context-Menu Actions** `P2` — right-click a selection to quote it into the active session, send the whole page, or capture a region as an image. The menu entries are the low-friction path; they post through the same grant as the tools do, so a quote from an ungranted origin fails the same way a tool call does · `extension/` (new)
- [ ] **Session Picker & Live Status** `P2` — the extension popup lists the daemon's sessions over the gateway, shows which ones hold a grant on the current origin, and streams turn status so you see the agent work without leaving the page · `extension/` (new), `crucible-web` · depends: [[#HTTP Gateway|SSE Event Bridge]]
- [ ] **Remote-Agent Iframe Host** `P2` — the reverse direction: an external agent embeds a Crucible session as an iframe in its own page, and the extension brokers the permissions that a bare cross-origin iframe cannot get. Distinct from **Plugin Panel Hosting** below, which was about *Crucible* hosting plugin panels and is superseded · `crucible-web`, `extension/` (new)

### Superseded Web Plans

> Kept as entries rather than deleted so the next reader does not re-propose them. Both were foundations for a rendering approach the web UI did not take.

- [-] **Oil Node Serialization** `P1` — `impl Serialize for Node`, Oil nodes to JSON for browser rendering · `crucible-oil` (behind the `serde` feature)
  - **Gets you:** nothing — a serializer with no reader.
- [ ] **SolidJS Oil Renderer** `P1` — an `<OilNode>` component tree for the browser. **Out of scope, not pending**: the web UI went a different route entirely — native SolidJS components composed through a panel registry, with markdown/mermaid/KaTeX/shiki rendering in TypeScript. Nothing depends on it, contradicting this entry's original "everything else depends on it" · `crucible-web`, `crucible-oil`
- [ ] **Plugin Panel Hosting** `P1` — iframe sandbox + message-passing protocol for Lua-registered web panels. **Superseded, not pending**: per the 2026-07-26 asymmetric-extensibility decision, web panels are TypeScript, registered in `web/src/lib/register-panels.tsx`. `PluginPanel.tsx` is a plugin *manager*, not a host · `crucible-web`, `crucible-lua`

## Collaboration & Scale

- [ ] **Sync System** `P4` — Merkle diff + CRDT for multi-device synchronization
- [ ] **Concurrent Agent Access** `P4` — multiple agents accessing a kiln simultaneously · `crucible-daemon`
- [ ] **Shared Memory** `P4` — Worlds/Rooms for collaborative cognition
- [ ] **Federation** `P4` — A2A protocol for cross-kiln agent communication

---

## Archived / Cut

Removed and cut features live here **only** — there are no inline tombstones. Where a removal
carries a live design lesson, that lesson stays as section prose (the `session-digest` auto-merge
failure is why the Reflection Pass is propose-only; see Self-Improvement Avenues).

| Item | Date | Reason |
|------|------|--------|
| Session-to-note indexer | 2026-09-14 | Removed the unused adapter after `cru session reindex` was retired. Transcript replay, text search and export remain |
| Separate shell-history store | 2026-09-14 | Removed the unread 100-entry store and cursor. Up/Down still recall `!` commands through general input history |
| `cru.log.messages` | 2026-09-14 | Removed the inert panel-action API. Notifications still deliver through `cru.log.notify` and `notify_once`; each client owns its panel visibility |
| `crucible-desktop` (GPUI) | 2024-12-13 | Cut — using Tauri + web instead |
| `add-desktop-ui` OpenSpec | 2024-12-13 | Archived — GPUI approach abandoned |
| `add-meta-systems` | — | Too ambitious (365 tasks), overlaps with the focused Lua approach |
| `add-advanced-tool-architecture` | — | Overlaps with the working MCP bridge |
| `add-quick-prompt-features` | — | Nice UX, not core — revisit later |
| `refactor-clustering-plugins` | — | Nice feature, not core |
| Ratatui TUI | 2025-01-17 | Removed — migrated to the oil-only TUI |
| SurrealDB Backend | 2026-02-23 | Removed — SQLite is the default and only backend; the crate was deleted (17K LOC). Document Clustering and the K-Means stub went with it |
| Team Patterns (supervisor / router / broadcast) | 2026-05-12 | Removed (~1984 LOC) — the hardcoded types each picked one delegation shape and shut out variants. Delegation *primitives* are infrastructure, delegation *patterns* are user code: they are now 5–20 line Lua recipes over `cru.session.*`, documented in [[Help/Delegation Patterns]] |
| Grammar + Lua Integration (`cru.grammar` GBNF bindings) | 2026-05-12 | Removed — shipped briefly in Wave 2 with no working backend. Revisit if llama-cpp is integrated |
| `session-digest` Runtime Plugin | 2026-05-12 | Removed — LLM-judged dedupe risked wrong merges and kiln pollution, and users preferred prompted refinement over automatic digests. Replaced by the propose-only Reflection Pass |
| hermit plugin | 2026-03-29 | Removed — capabilities belong in chat/messaging integration plugins |
| Deferred message queue (TUI) | 2026-06-10 | Removed — typing during a turn preserves the draft instead; Ctrl+Enter cancels |
| User-facing `temperature` / `max_tokens` knobs | 2026-06-28 | Removed from `:set` and `cru set` — the genai turn path never applied them. `d9894b729` later made the config path work |
| Session-knob `temperature` / `max_tokens` / `system_prompt` | 2026-09-06 | Removed the RPC, web controls and `SessionKnobs` methods (−1363 lines). The values stayed settable in config until the row below |
| `temperature` and `max_tokens` entirely | 2026-09-07 | Removed from `[llm.providers.*]`, `[chat]`, agent-card frontmatter, the session default tier, `SessionAgent` and `ChatOptions`. Both are per-model inference settings, so genai picks the right default for the model actually being called. Crucible's `DEFAULT_PROVIDER_MAX_TOKENS = 4096` had been capping replies for any user with `[llm] default` set who did not name their own `max_tokens` — genai allows 64000 for claude-sonnet/haiku/3-7-sonnet/opus-4-5, so a reply over 4096 output tokens was truncated. claude-3-opus and claude-3-haiku were unaffected: genai defaults those to 4096 too |
| `max_iterations` / `execution_timeout` | 2026-09-06 | Removed, not demoted. `max_iterations` defaulted to 10 tool rounds, which cut a normal refactor short, and it capped plugin reviewer sessions too. `execution_timeout` would have ended a turn holding a dev server open. `TurnEvent::DepthCapHit`, `StopReason::MaxToolDepth` and the agent-card `max_turns` field went with them. OpenCode defaults `steps` to `Infinity`; Pi's loop is `while (true)` |
| Session-knob `context_window` | 2026-09-06 | Removed — one `Option<usize>` with two readers and two units. `enforce_context_budget` read message pairs, `visible_tools` read tokens, and the documented usage silently dropped every deferrable tool. Sliding-window and summarize keep 10 pairs |

## Links

- [[Meta/Architecture/Index]] — Architecture entry points, boundaries and dated audits
- [[Meta/Product Decision Log]] — Dated product decisions, with reversals annotated
- [[Meta/TUI User Stories]] — TUI requirements
- [[Meta/Web User Stories]] — Web requirements
- [[Meta/Plugin User Stories]] — Plugin requirements
