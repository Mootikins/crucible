---
title: Product
description: Product feature map — capabilities, status, documentation, and dependencies
type: product
status: active
updated: 2026-07-30
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
> Shipped and in-progress entries carry two sub-bullets: **Gets you** (what a user observes)
> and **Proof** (what demonstrates it). A proof line rendered in italics as
> `_none — …_` means the entry claims something nothing in the tree demonstrates; that
> italic is the signal, and it is why several `[x]`s became `[-]`s in the 2026-07-30
> reconciliation. `_undetermined — …_` means the sweep could not settle it and names what
> would. Planned `[ ]` entries carry no sub-bullets — there is nothing yet to observe.

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
  - **Gets you:** `[[note]]` renders as a clickable anchor resolving to the target note; the target's Backlinks panel lists the linking note; an alias displays but still resolves the real target.
  - **Proof:** `crates/crucible-web/web/e2e/stories/backlinks-panel.story.spec.ts`::shows linked + unlinked mentions and one-click links a mention; `crates/crucible-web/web/src/lib/__tests__/markdown.test.ts`::aliased wikilinks display the alias but resolve the target
- [-] **Wikilink Heading & Block Fragments** `P0` — `[[Note#Section]]` and `[[Note#^id]]` fragment targets · `crucible-core` (parser)
  - **Gets you:** nothing beyond a plain note link — both forms open the note at the top.
  - **Proof:** _none — the fragment is parsed into `Wikilink.block_ref` and never read; the rendered anchor discards everything after `#` (`crates/crucible-web/web/src/lib/markdown.ts`:157-159), and no consumer scrolls to a heading or block._
- [x] **Link-Preserving Refactor** `P0` — renaming or moving a note rewrites every inbound wikilink; `fs.list_dir` / `fs.move` / `fs.mkdir` / `fs.trash` and `note.rename` / `note.move` back the web file tree's drag-and-drop · `crucible-daemon`, `crucible-web`
  - **Gets you:** you move a note and every link to it still resolves — with each author's original decorations kept (alias, heading, block-ref, embed marker, path style), ambiguous targets and code blocks left alone, and `.canvas` references re-pointed.
  - **Proof:** `crates/crucible-daemon/src/server/note_refactor.rs`::rename_rewrites_all_link_forms_preserving_decorations, `::rename_never_touches_ambiguous_links`, `::rename_leaves_code_blocks_alone`, `::move_into_and_out_of_folder_keeps_links_resolving`, `::moving_a_canvas_reindexes_it_at_the_new_path` — all assert file bytes on disk; `crates/crucible-daemon/src/server/fs.rs`::move_rejects_escapes_in_either_path
- [x] **Tags** `P0` — `#tag` and `#nested/tag` (stored as flat strings; no hierarchy rollup) · [[Help/Tags]] · `crucible-core` (parser)
  - **Gets you:** a tag in the body or in frontmatter lands in the note's indexed tag list, and `property_search {"tags": [...]}` returns the matching notes.
  - **Proof:** `crates/crucible-daemon/src/tools/search.rs`::test_property_search_tags_or_logic (asserts the tool's result JSON for an OR query over two notes)
- [x] **Frontmatter** `P0` — YAML (`---`) and TOML (`+++`) metadata in note headers · [[Help/Frontmatter]] · `crucible-core` (parser)
  - **Gets you:** `property_search {"status":"draft"}` returns only the matching note, and `list_notes --include-frontmatter` returns the parsed block.
  - **Proof:** `crates/crucible-daemon/src/tools/search.rs`::test_property_search_single_property; `crates/crucible-web/web/src/lib/__tests__/frontmatter.test.ts`
- [-] **Block References** `P0` — `^block-id` paragraph-level linking · [[Help/Block References]] · `crucible-core` (parser)
  - **Gets you:** nothing. `^block-id` is inert text everywhere — nothing defines, resolves, renders, or embeds a block.
  - **Proof:** _none — there is no anchor-side parse at all; `parser/types/blocks.rs` blocks are content-hash units for the merkle tree, not addressable anchors. `docs/Help/Block References.md` is a spec for an unbuilt feature._
- [x] **Callouts** `P0` — `> [!type]` admonition blocks · `crucible-web` (markdown-it plugin)
  - **Gets you:** `> [!note] Title` renders as a styled callout box with icon and title in the web reading view and live preview; `> [!tip]-` renders a collapsed `<details>`.
  - **Proof:** `crates/crucible-web/web/src/lib/__tests__/callouts.test.ts`::renders a titled callout with icon, title, and body and `::foldable-collapsed renders a closed <details> with <summary>`. Not rendered in the TUI; the Rust-side `ParsedNote.callouts` extraction has no consumers.
- [x] **LaTeX** `P0` — `$inline$` and `$$block$$` math notation · `crucible-web`
  - **Gets you:** `$$…$$` renders as a KaTeX widget in the editor's live preview and reading view, while a `$$` block inside a code fence correctly stays as source.
  - **Proof:** `crates/crucible-web/web/e2e/live-preview-blocks.spec.ts`::renders display math but leaves a $$ block inside a code fence as source; `crates/crucible-web/web/src/lib/__tests__/math.test.ts`
- [-] **Footnotes** `P0` — reference-style footnotes · `crucible-core` (parser)
  - **Gets you:** nothing — `[^1]` renders as literal text in both the web reading view and the TUI.
  - **Proof:** _none — `parser/footnotes.rs` populates a `FootnoteMap` whose only reader is a parser self-check; neither markdown-it (`markdown.ts:328-341`) nor the TUI renderer has any footnote handling, and there is no `docs/Help/Footnotes.md`._
- [x] **Tables** `P0` — markdown tables · `crucible-core` (parser), `crucible-cli`, `crucible-web`
  - **Gets you:** a pipe table renders as a box-drawn table in the TUI (respecting terminal width and CJK cell widths) and as an HTML table in the web reading view; the editor can reformat pipe alignment.
  - **Proof:** `crates/crucible-cli/src/tui/oil/markdown/tests.rs`::table_respects_width_constraint, `::table_with_cjk_content_respects_width`; `crates/crucible-web/web/src/lib/__tests__/table-format.test.ts` (asserts realigned pipe output including the alignment row). TUI column alignment specifically is not asserted anywhere.
- [x] **Task Lists** `P0` — `- [ ]` / `- [x]` checkbox items · `crucible-core` (parser), `crucible-web`
  - **Gets you:** checkbox list items render with their checked state and the literal brackets removed. The rendered checkbox is `disabled` — you cannot tick a box in the UI to update the file.
  - **Proof:** `crates/crucible-web/web/src/lib/__tests__/markdown.test.ts`::renders GFM task lists as checkboxes with state
- [x] **Task Harness (`TASKS.md`)** `P0` — structured task files with phases, task IDs and a dependency graph · [[Help/Task Management]] · `crucible-core` (parser), `crucible-cli`
  - **Gets you:** `cru tasks list` / `next` / `pick` / `done` / `blocked` read and mutate checkbox tasks in a markdown task file, with dependencies resolved through `TaskGraph`.
  - **Proof:** `crates/crucible-cli/src/commands/tasks.rs`:12 over `crucible_core::parser::{CheckboxStatus, TaskFile, TaskGraph}`. This is a markdown harness, not a storage subsystem — see **Task Storage** under Storage & Processing.
- [x] **Kilns** `P0` — vault-like note collections with `.crucible/` config · [[Help/Concepts/Kilns]] · `crucible-core`, `crucible-daemon`
  - **Gets you:** a directory with `.crucible/kiln.toml` opens as a kiln over RPC; `kiln.list` shows it, `list_notes` returns its notes, `kiln.close` removes it.
  - **Proof:** `crates/crucible-daemon/tests/rpc_kiln_e2e.rs`::test_kiln_lifecycle_open_query_close and `::test_list_notes_returns_seeded_notes` (real daemon over a socket)
- [x] **JSON Canvas File Format** `P0` — read and write `.canvas` (JSON Canvas 1.0, Obsidian's format) · `crucible-core` (canvas)
  - **Gets you:** an existing Obsidian vault opens without conversion, and a canvas Crucible saves is byte-identical to Obsidian's — tab indentation, one object per line, key order preserved. Unknown keys authored by third-party plugins round-trip verbatim.
  - **Proof:** `crates/crucible-core/src/canvas/tests.rs`::saving_an_untouched_obsidian_canvas_produces_no_diff, `::the_written_form_uses_tabs_and_one_object_per_line`, `::round_trip_is_idempotent`, `::unknown_keys_survive_a_round_trip_at_every_level`
- [x] **Plaintext First** `P0` — markdown files are always the source of truth · [[Help/Concepts/Plaintext First]]
  - **Gets you:** edits and refactors land as markdown bytes on disk; the editor's save carries the exact buffer, and a rename rewrites real files rather than an index.
  - **Proof:** `crates/crucible-daemon/src/server/note_refactor.rs`::rename_rewrites_all_link_forms_preserving_decorations (reads files back with `std::fs::read_to_string`); `crates/crucible-web/web/e2e/stories/editor-roundtrip.story.spec.ts`::open → type → dirty ● → save → clean, with exact PUT body
- [ ] **Note Types** `P3` — templates and typed notes (book, meeting, movie) · `crucible-core`

## Knowledge Discovery

- [x] **Semantic Search** `P0` — vector similarity search over kiln notes · [[Help/Concepts/Semantic Search]] · `crucible-daemon` (storage, llm)
  - **Gets you:** a natural-language query is embedded and returns ranked note paths with similarity scores — via `cru search --type semantic`, the web Text|Semantic toggle, and the agent's `semantic_search` tool.
  - **Proof:** `crates/crucible-daemon/src/storage/lance/vector_index.rs`::upsert_then_search_returns_match (asserts ordering and relative similarity); `crates/crucible-daemon/tests/acp_delegation_e2e.rs`::call_semantic_search asserts the live `tools/call` response body
- [x] **Content Search (ripgrep)** `P0` — `search_grep` RPC + `POST /api/search/grep` + the agent's `text_search` tool · `crucible-daemon` (tools)
  - **Gets you:** searching for a word that appears only in a note's *body* returns that note with the matching line, line number and match offsets for highlighting; a `root` is accepted only if it canonicalizes inside a registered project or open kiln.
  - **Proof:** `crates/crucible-daemon/src/server/grep.rs`::greps_notes_with_offsets_and_rel_path, `::searches_all_files_when_glob_omitted`, `::limit_is_clamped_and_truncation_flagged`, `::root_outside_every_registered_root_is_rejected`
- [x] **`cru search` Text Mode & FTS5 Index** `P0` — index-backed full-text search from the CLI, BM25-ranked over titles and bodies · [[Help/CLI/search]] · `crucible-daemon` (storage), `crucible-cli`
  - **Gets you:** `cru search <word>` finds notes containing that word anywhere in their body, not just in the filename or title, ranked by relevance. The pipeline writes the index as notes are processed, deletes drop out of it, and a kiln indexed by an older build is backfilled once on open.
  - **Proof:** `crates/crucible-daemon/tests/text_search.rs`::text_search_matches_words_in_note_bodies (sentinel present only in the body; asserted through the `search_text` RPC the command calls, not through the agent tools' ripgrep walk, which already worked), `::text_search_still_matches_titles`, `::punctuation_in_a_query_does_not_error`; `crates/crucible-daemon/src/kiln_manager/tests.rs`::opening_a_kiln_backfills_a_text_index_that_was_never_written
- [x] **Knowledge Graph** `P0` — wikilink-based graph structure and backlinks · [[Help/Concepts/The Knowledge Graph]] · `crucible-daemon` (storage)
  - **Gets you:** `kiln.graph` returns every visible note plus its resolved and dangling links, which the web renders as an Obsidian-style graph; `get_backlinks` drives the Backlinks panel.
  - **Proof:** `crates/crucible-daemon/src/storage/sqlite/link_index.rs` graph-edge test (asserts self-links excluded, duplicates deduped, resolved vs dangling edge shape); `crates/crucible-web/web/e2e/stories/backlinks-panel.story.spec.ts`
- [x] **Backlinks API** `P0` — `get_backlinks` RPC + `GET /api/backlinks`, linked *and* filtered-unlinked mentions with byte spans · `crucible-daemon`, `crucible-web`
  - **Gets you:** the backlinks panel renders the line containing each wikilink, lists unlinked mentions of the note, and hovering scrolls the preview to that section.
  - **Proof:** `crates/crucible-web/tests/route_contract_tests/kilns.rs`::backlinks_returns_linked_and_filtered_unlinked, `::backlinks_missing_note_file_degrades_to_empty_unlinked`, `::backlinks_rejects_path_traversal_in_note`
- [x] **Canvases as Graph Citizens** `P0` — a `.canvas` contributes its file cards and the wikilinks inside its text cards as real graph links · `crucible-daemon` (pipeline)
  - **Gets you:** a note's backlinks list the canvases that reference it, and canvas references survive renames and moves. Containment redacts out-of-root references on the read path, so a client never receives a path it could not have asked for.
  - **Proof:** `crates/crucible-daemon/src/pipeline/canvas_index.rs`::file_nodes_become_links, `::wikilinks_inside_text_nodes_become_links`, `::a_redacted_file_node_contributes_no_link`, `::an_uncontained_reference_never_reaches_the_link_index`
- [-] **Graph Traversal** `P0` — n-hop / neighbourhood queries over the graph · `crucible-daemon` (storage)
  - **Gets you:** nothing a user or agent can call. `kiln.graph` hands back a flat edge list; all traversal happens client-side in the web graph view.
  - **Proof:** _none — the `GraphView` trait's two implementations (`storage/graph.rs::InMemoryGraph`, `storage/sqlite/graph_view.rs::SqliteGraphView`) are never constructed outside their own test modules, and `rpc/dispatch.rs::METHODS` has no traversal method._
- [-] **Query System** `P0` — structured note queries with a composable pipeline · [[Help/Query/Query System]] · `crucible-daemon` (storage)
  - **Gets you:** nothing. There is no way to run a query — no CLI command, no RPC method, no tool, no Lua binding.
  - **Proof:** _none — `storage/sqlite/query/` is a full subsystem (IR, pipeline, transforms, two syntaxes, a SQLite renderer with snapshot tests) whose only reference outside itself is a `pub use`. `docs/Help/Query/Query System.md` documents a feature with no entry point._
- [x] **Property Search** `P0` — search notes by frontmatter properties and tags · `crucible-daemon` (tools)
  - **Gets you:** an agent calling `property_search {"status":"draft"}` or `{"tags":["urgent","important"]}` gets JSON listing only the matching notes with their paths and tags.
  - **Proof:** `crates/crucible-daemon/src/tools/search.rs`::test_property_search_single_property, `::test_property_search_tags_or_logic`, `::test_property_search_multiple_properties_and`, `::test_property_search_no_frontmatter` — both the indexed and filesystem-scan backends are covered
- [-] **Document Clustering** `P0` — heuristic clustering and MoC detection · `crucible-daemon` (storage)
  - **Gets you:** nothing. The implementation was deleted with `crucible-surrealdb` on 2026-02-23 and never reimplemented; the `cru cluster` command went earlier.
  - **Proof:** _none — `rg -i cluster crates/` matches only a doc comment and a test fixture string. Nothing in `crucible-daemon` took the capability over. This is closer to unbuilt than in-progress._
- [ ] **K-Means Clustering** `P2` — k-means implementation; from scratch (the stub this once referred to was deleted with `crucible-surrealdb` on 2026-02-23), and depends on Document Clustering being rebuilt first · `crucible-daemon` (storage)
- [-] **Block-level Embeddings** `P0` — paragraph-granularity semantic indexing · `crucible-daemon` (llm, storage)
  - **Gets you:** whole-note search granularity, not paragraph. Per-block vectors are computed and then destroyed: two or more are averaged component-wise into a single document-level vector before storage.
  - **Proof:** _none — `pipeline/note_pipeline.rs:420-450` collapses the block embeddings ("Average all embeddings for document-level vector") and `:257` upserts one row per note path. There is no block-granularity vector table, and averaging a long note's paragraph vectors degrades recall versus embedding it once._
- [x] **Session Search** `P0` — text search across past conversations · `crucible-daemon` (observe), `crucible-cli`
  - **Gets you:** `cru session search "<query>"` prints matching session ids with the line number and surrounding context from the session JSONL.
  - **Proof:** `crates/crucible-daemon/src/server/session/list.rs`:119 (`session.search` returns `{matches:[{session_id,line,context}],total}`); `crates/crucible-cli/src/commands/session/tests/search.rs`::test_search_sessions, `::test_search_in_memory`, `::test_search_with_ripgrep_fallback`
- [-] **Session Semantic Indexing** `P0` — sessions indexed for semantic search via a session indexing pipeline · `crucible-daemon` (observe)
  - **Gets you:** nothing semantic. There is no pipeline — nothing runs on session end, pause, or a watcher event. Only a manual `cru session reindex` writes rows, and it writes them without embeddings.
  - **Proof:** _none — `extract_session_content` has exactly one non-test caller (`server/observe.rs:315`), and that path stores `content.to_note_record(None)` where `None` is the embedding, so reindexed sessions can never surface from `search_vectors`._

## Agent Learning & Memory

> Agents that get smarter over time. Learning is implemented as **notes in the kiln** — not opaque database stores. Entity facts, session summaries, and accumulated knowledge are all atomic zettelkasten-style markdown notes with wikilinks, tags, and frontmatter. This means agent memory is human-readable, editable, searchable via the existing knowledge graph, and available to precognition for future context injection.
>
> **Two-tier model**: Core Rust features (precognition, auto-linking) handle the fast path. Default runtime Lua plugins handle higher-level knowledge extraction. Both are toggleable and overridable. See [[#Core Agent Features]] and [[#Default Runtime Plugins]].
>
> **Informed by**: Agno framework analysis (2026-02). Agno uses six opaque DB-backed learning stores. Crucible's approach is strictly better — same learning capabilities but with human-readable, editable, wikilinked notes as the storage layer.

- [x] **Precognition** `P0` — Auto-RAG: inject relevant kiln context before the agent's first turn; the core differentiator — every conversation starts knowledge-graph-aware · [[Help/Concepts/Precognition]] · `crucible-daemon` (agent_manager/precognition), `crucible-cli`
  - **Gets you:** kiln notes matching your opening message are prepended to the message list the agent actually receives, with a `precognition_complete` event carrying the note list to the TUI and web. It fires on the **first user message of a session only**, not before every turn, and it is on by default.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/precognition.rs`::test_precognition_enriched_content_reaches_agent (seeds a real note + embedding, runs a turn against a `PromptCapturingAgent`, asserts the captured messages carry the enrichment), plus `::test_precognition_complete_event_emitted_when_enrichment_runs`, `::test_precognition_emits_note_info_in_event`, `::test_precognition_runs_only_on_first_user_message_of_session`
- [x] **Precognition Toggle** `P0` — `:set precognition` turns injection off for a session, in all four spellings (`precognition=off`, `noprecognition`, `precognition`, `precognition!`) · `crucible-cli`, `crucible-daemon`
  - **Gets you:** turning precognition off in the TUI actually stops the daemon injecting kiln context — for every client on that session, and for the session's remaining life, not just this client's `:set` readout.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/precognition.rs`::precognition_disabled_mid_session_stops_enriching (control turn enriches, then the toggle is applied through the daemon API and a rewind makes the next turn eligible again — it does not enrich); `crates/crucible-cli/src/tui/oil/chat_runner/tests/knob_rpc.rs`::interactive_set_knob_reaches_matching_rpc (`precognition` case: real keystrokes reach `AgentHandle::set_precognition`); `crates/crucible-cli/src/tui/oil/chat_app/command_handling_tests.rs`::value_less_precognition_spellings_carry_the_value_to_the_daemon
- [x] **Precognition Selection Seam** `P0` — a Lua `precognition_select` handler can filter or reorder the candidate set before the agent sees it · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a plugin can veto or re-rank which notes get injected, and the agent receives the filtered set.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/precognition.rs`::test_precognition_select_handler_reaches_the_agent, `::test_transform_context_handler_dropping_precog_triggers_reprepend`
- [x] **Memory Scoping** `P2` — `Scope::Workspace { path }` enforced at the storage query layer · `crucible-core`, `crucible-daemon`
  - **Gets you:** a kiln-bound `KnowledgeRepository` cannot return notes belonging to a sibling workspace — `list_notes`, `get_note_by_name` and `search_vectors` all drop them. Vector isolation is structural rather than filtered: each kiln gets its own index directory at `<kiln>/.crucible/crucible-vectors.lance/`.
  - **Proof:** `crates/crucible-daemon/src/storage/sqlite/repository.rs`::precognition_does_not_see_sibling_workspace_notes (inserts two scoped notes, asserts the sibling is absent from all three read paths); `storage/sqlite/adapters.rs::sqlite_client_handle_as_knowledge_repository_is_kiln_scoped`. Note the write side *stamps* a missing scope but preserves an already-bound one — that hides a note rather than leaking it, but it is not validation.

### Self-Improvement Avenues

> Two complementary ways an agent gets smarter. **Knowledge insertion** is the primary path and ships today; the **reflection pass** is the second avenue, and it is deliberately propose-only. That constraint is the lesson from the removed `session-digest` plugin, which auto-merged session summaries via LLM-judged dedupe and risked wrong merges and kiln pollution. Both avenues write to the same place — atomic kiln notes — so improvement stays human-readable and editable.

- [x] **Knowledge Insertion (primary)** `P1` — agents persist learning *during* work by writing kiln notes via `create_note` / `update_note`; those notes re-enter future sessions through Precognition · `crucible-daemon` (tools)
  - **Gets you:** the agent writes a real `.md` file into the kiln, the tool result reports its path, and a later session's precognition can retrieve it. The graph *is* the learning store — no opaque DB.
  - **Proof:** `crates/crucible-daemon/src/tools/notes/tests/crud.rs` (asserts the tool result body `{"path":"test.md","status":"created"}` and reads the content back through `read_note`), `notes/tests/path_safety.rs::test_create_note_path_traversal_parent_dir`; re-entry via `agent_manager/tests/precognition.rs::test_precognition_enriched_content_reaches_agent`, which reads a real kiln file
- [x] **Proposal Review (`cru proposals`)** `P2` — the human disposition surface for staged proposals · [[Help/Concepts/Reflection Pass]] · `crucible-cli`
  - **Gets you:** `cru proposals list|show` reads the staging dir; `accept <id>` moves the staged file into the kiln with provenance frontmatter stripped and your own fields kept; `reject` deletes it. Staged proposals live in `KILN/.crucible/proposals/`, outside the index, so an unreviewed proposal never reaches Precognition or search.
  - **Proof:** `crates/crucible-cli/src/commands/proposals.rs`::accept_moves_note_into_kiln_and_strips_provenance, `::accept_respects_target_frontmatter`, `::accept_strips_target_from_promoted_note`, `::accept_rejects_absolute_target`
- [-] **Reflection Pass** `P2` — on `on_session_end`, a forked cheap-model subagent reviews the finished transcript and proposes kiln notes · [[Help/Concepts/Reflection Pass]] · `crucible-daemon`, `crucible-lua`, `runtime/plugins/reflection`
  - **Gets you:** unproven. The `on_session_end` hook does fire into the plugin VM, but nothing shows a proposal file ever appearing in the staging dir, and the auxiliary-subagent fork is entirely unexercised.
  - **Proof:** _none — the shipped Lua suite covers `count_user_turns`, `build_transcript`, `parse_proposals`, `render_proposal` and the recursion guard, but `M.stage_proposals` and `M.run`'s happy path have no test at all; the `enabled`/`min_turns`/`model` knobs are covered only as config round-trips. Compounding it, release tarballs ship no `runtime/` directory, so an installed user has no `reflection` plugin to fire (see **Bundled Runtime Plugins in Releases**)._

## Context & Execution (Core Runtime)

> Runtime primitives that every reliable agent needs. These are too fundamental to be plugins — they govern how the agent manages its own context window, enforces execution boundaries, validates its output, and lets users recover from mistakes. Informed by competitive analysis (2026-03): Aider, CrewAI, LangGraph, and Semantic Kernel all treat these as core concerns.

### Prompt Caching

- [-] **Anthropic Cache Control** `P0` — `CacheControl::Ephemeral` on system prompts and the second-to-last turn · `crucible-daemon`
  - **Gets you:** unproven for the cache-control half. The token *reporting* half works — cache read/creation counts flow through `message_complete` and reach the statusline. Whether the outgoing request actually carries the cache breakpoints is watched by nothing.
  - **Proof:** _none for `apply_prompt_caching` — the symbol has exactly two hits in the repo, its definition (`provider/genai_handle.rs:46`) and its one call site (`:980`). It is subtle enough to warrant a test: it marks `messages.len() - 2` before prepending the system message, and deliberately routes the system prompt as a system-role `ChatMessage` because genai's `with_system()` drops `MessageOptions`. A silent regression there costs money and is invisible locally._
- [x] **Cache Stats** `P1` — per-session cache hit/miss aggregate exposed via `session.cache_stats` RPC, `cru.sessions.cache_stats(id)` Lua binding, and the `sl.cache` statusline item · `crucible-daemon`, `crucible-lua`, `crucible-cli`
  - **Gets you:** once a completion reports cache counts, the statusline renders `cache: 75%`; before that it renders nothing rather than a false `0%`.
  - **Proof:** `crates/crucible-cli/src/tui/oil/components/status_items.rs`::cache_renders_a_percentage_once_known and `::cache_renders_nothing_before_any_rate_is_known` (both assert the rendered row); arithmetic in `crates/crucible-daemon/src/agent_manager/cache_stats.rs`::hit_rate_aggregates_across_completions, `::miss_recorded_when_read_tokens_zero_or_absent`, `::hit_rate_none_before_any_data`

### Context Window Management

- [x] **Token Budget Tracking** `P0` — `context_budget` on `SessionAgent`, settable via RPC; `estimate_tokens` chars/4 heuristic · `crucible-daemon`, `crucible-core`
  - **Gets you:** the budget you set is the budget the agent handle enforces on every request, and it sizes the tool-schema deferral decision. Still unset by default, so `usage.budget` and `usage.percent` read `0` until you set one.
  - **Proof:** `crates/crucible-daemon/src/agent_factory.rs`::session_generation_and_context_settings_reach_the_agent_handle (the hop that used to drop it); enforcement in `provider/genai_handle.rs` `mod tests` (`enforce_context_budget`). See **Context Strategies**.
- [-] **Auto-Compaction** `P0` — compact the conversation when prompt usage crosses `context_budget * autocompact_threshold` (default 0.95); also reachable as `cru.context.compact` and `session.request_compaction` · `crucible-daemon`
  - **Gets you:** nothing is ever compacted. Crossing the threshold flips the session's state string to `"compacting"` and that is the entire effect — the messages sent on the next turn are unchanged.
  - **Proof:** _none — `SessionManager::request_compaction` sets `entry.state = SessionState::Compacting` and returns; its own doc comment says the agent performs the compaction "when it sees this state", and no agent ever sees it. `SessionState::Compacting` has zero consumers. `should_autocompact` is a well-tested pure predicate wired to nothing, and the RPC's `{"compaction_requested": true}` reply is actively misleading. Worse, the session is then stuck: `session.pause` fails its `state != Active` guard — see **Session Compaction** under Storage & Processing._
- [x] **Context Strategies** `P1` — `ContextStrategy::{Truncate, SlidingWindow, Summarize}` · `crucible-core`, `crucible-daemon`
  - **Gets you:** the session's strategy and budget reach the handle, so an over-budget conversation is really trimmed before the request goes out. `:set context_strategy=sliding_window` changes what the model sees.
  - **Proof:** `crates/crucible-daemon/src/agent_factory.rs`::session_generation_and_context_settings_reach_the_agent_handle — the factory chained only `with_deferrable_tools`/`with_plugin_tools`/`with_modes`, so `context_budget` was permanently `None` and `enforce_context_budget` returned `NoChange` on every request; strategy behaviour itself in `provider/genai_handle.rs` `mod tests`. _Caveat: Summarize still elides to the static `[summary placeholder]`. The LLM recap the prose promises is untested at every level — `summarize_via_backend` has zero test references._
- [x] **Lua Context Operations** `P1` — `cru.context.{usage, messages, remove, estimate_tokens}` · `crucible-lua`, `crucible-core`, `crucible-daemon`
  - **Gets you:** `cru.context.remove(id, {type="last", n=2})` actually shortens the conversation path the next turn is built from; `usage` returns a populated table.
  - **Proof:** `crates/crucible-daemon/src/session_bridge.rs`::remove_messages_last_n_rewinds_tree (seeds 3 nodes, removes 2, asserts the tree path), `::remove_messages_indices_truncates_from_start`, `::context_usage_returns_expected_shape`. The tree is authoritative for the prompt (`agent_manager/messaging/stream.rs:258-260`). `compact` is the one member of this module that does nothing — see **Auto-Compaction**.
- [x] **`cru.context.attach`** `P1` — mid-turn context attachment from a Lua handler · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a handler that finds something useful partway through a turn (say from a `tool_result`) can put it where the agent's *next* LLM call **in that same turn** will see it — deduped by key so a repeated trigger attaches once, capped by a per-session character budget with a typed rejection reason.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/messaging.rs`::attached_context_reaches_the_agent_within_the_same_turn, `::repeated_triggers_attach_once_per_key`, `::context_attach_is_available_without_any_plugin_boot`

### Execution Limits

- [x] **Max Iterations** `P1` — `DEFAULT_MAX_TOOL_DEPTH = 10`; `max_iterations` on `SessionAgent`, `None` = unlimited · `crucible-daemon`
  - **Gets you:** after the configured number of tool rounds the runtime replays a depth-cap prompt ("You have reached the tool call limit…") to the model and the turn finishes with the model's final text — not an error.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/messaging.rs`::depth_cap_triggers_depth_prompt_and_completes_with_text (sets `max_iterations = Some(2)`, scripts four turns, asserts no `ended` error, the post-cap reply in `message_complete.full_response`, and the prompt text in a captured request)
- [-] **Execution Timeout** `P1` — `execution_timeout_secs` on `SessionAgent`; cancel and report on exceed · `crucible-daemon`
  - **Gets you:** unproven. The path is complete and unconditional — `messaging/send.rs:308-341` wraps the turn in `tokio::time::timeout` and emits `ended` with `"error: execution timeout reached"` — but no test has ever watched it fire.
  - **Proof:** _none beyond a set/get round-trip (`tests/rpc_config_agent_e2e.rs:578-580`), the only test touching this feature. One `#[tokio::test(start_paused = true)]` in `agent_manager/tests/messaging.rs` would settle it; the harness and the paused-clock pattern already exist there._

### Agent Undo

- [x] **Turn Undo** `P1` — `/undo [N]` reverts the last agent turn(s): file rollback via `WorkspaceSnapshot` plus message truncation · `crucible-daemon`, `crucible-cli`
  - **Gets you:** workspace files are restored to their pre-turn bytes on disk and the conversation the next turn is built from is rewound. Git mode uses `write-tree`+`commit-tree` for untracked-file safety; non-git mode uses an in-memory journal capped at 5 MiB. Two caveats: the **TUI viewport is not truncated** — you get a toast saying the turn was reverted while the reverted turn is still on screen — and the `SnapshotMap` is in-memory, so `/undo` after a daemon restart rewinds the chat and silently leaves files alone. `/redo` is deferred (no `redo_turns` on `ConversationTree`).
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/messaging.rs`::turn_undo_restores_snapshotted_file (writes v1, runs a turn, overwrites with v2, undoes, asserts the file reads v1); `crates/crucible-daemon/src/workspace_snapshot.rs`::snapshot_git_captures_uncommitted_change_and_restores, `::snapshot_non_git_uses_journal`, `::snapshot_journal_cap_skips_large_workspace`; TUI `tests/user_story_tests/undo_tests.rs::undo_flow_frame_sequence`
- [x] **Undo Lua API** `P1` — `cru.sessions.{undo, can_undo, undo_depth, undo_history}` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** `cru.sessions.undo(id, n)` performs a real undo (files + tree) and returns the turn count; `undo_history` returns per-turn `{turn_index, messages_removed}` tables.
  - **Proof:** `crates/crucible-daemon/src/session_bridge.rs`:646-687 lands on the E1-tested `AgentManager::undo` above; Lua surface shape in `crates/crucible-lua/src/sessions/tests/graph.rs`::sessions_undo_returns_count, `::sessions_undo_history_returns_list`. The Lua-side tests are mock-backed, so the full Lua → bridge → manager chain is unwatched.

### Output Validation

- [x] **Output Validation** `P1` — `validate_output` runs after each assistant turn in `execute_agent_stream`; `OutputValidation::None` (default) is a zero-cost early return · `crucible-daemon`, `crucible-core`
  - **Gets you:** a response failing validation with no retries left ends the turn with `ended.reason == "error: output validation exhausted retries"`, and the default setting lets anything through untouched.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/messaging.rs`::test_validate_retry_zero_retries_emits_exhausted_ended (asserts the event body) and `::test_validate_retry_none_validation_passes_freely`
- [-] **Validation Retry Re-entry** `P1` — a failure with retries remaining injects a regenerate-prompt and re-enters the stream · `crucible-daemon`
  - **Gets you:** unproven. Nothing asserts a second turn is ever issued with the regenerate prompt, so nothing shows the loop terminates.
  - **Proof:** _none — every validation test in the repo sets `validation_retries = 0`, so `ValidationOutcome::Retry` is never constructed under test and the recursive re-entry is never taken via the validation branch._
- [x] **Lua Validators** `P1` — `OutputValidation::Lua { name }` registered via `cru.context.register_validator(name, fn)`, enabled per session via `cru.sessions.set_output_validation` · `crucible-core`, `crucible-lua`, `crucible-daemon`
  - **Gets you:** a validator you register from Lua is actually invoked against the assistant's text and its verdict changes the turn's outcome. An unregistered name or an unbound runtime degrades to validation failure, not a panic.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/messaging.rs`::test_lua_validator_failure_triggers_retry_and_exhausts (registers a real closure in a real Lua VM), `::test_lua_validator_pass_no_retry`, `::test_lua_validator_unregistered_name_errors`; registry mechanics in `crates/crucible-lua/src/context.rs`::register_validator_stores_callback_and_runs

## AI Chat & Agents

### Conversation & Sessions

- [x] **Interactive Chat** `P0` — conversational AI with streaming text, thinking, tool calls, and subagent events · [[Help/CLI/chat]] · `crucible-cli`, `crucible-daemon`
  - **Gets you:** the TUI renders streaming assistant text, graduated thinking blocks, tool-call rows and subagent rows as a turn progresses.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/fixture_replay_tests.rs` (replays real JSONL and asserts rendered frames); `tests/user_story_tests/subagent_mcp_tests.rs::concurrent_subagents_render_as_separate_rows`
- [x] **Agent Cards** `P0` — configurable agent personas with system prompts, model, tool policy, mode and MCP servers · [[Help/Extending/Agent Cards]] · [[Help/Config/agents]] · `crucible-core` (config), `crucible-daemon`
  - **Gets you:** a discovered card layers its settings over the config defaults on the resulting session's agent.
  - **Proof:** `crates/crucible-daemon/tests/delegation_integration.rs`:859 (child session's `agent.agent_card_name == Some("researcher")` read back from the real session store); composition at `crucible-core/src/session/types/agent.rs:210-285`
- [x] **Agent Card Discovery & Model Resolution** `P0` — cards discovered from `~/.config/crucible/agents/`, kiln `agents/`, and project `.crucible/agents/` (later shadows earlier); model resolves card-explicit > `specialty:` through a `[llm.models]` config table > inherited · `crucible-daemon`
  - **Gets you:** you drop a card file in any of three places and the nearest one wins; a card can name a *specialty* instead of a model and the `[llm.models]` table maps it. Only `description` is required. `delegate_session` resolves targets card-first, then ACP profiles.
  - **Proof:** `crates/crucible-daemon/tests/delegation_integration.rs`::card_specialty_resolves_through_llm_models_table; discovery at `crates/crucible-daemon/src/agent_cards.rs`:44
- [-] **Agent Card Selection from the CLI** `P0` — start a chat session on a named agent card · `crucible-cli`
  - **Gets you:** nothing. No CLI or TUI surface starts a session on a card; `cru agents list|show|validate` only inspects them.
  - **Proof:** _none — `commands/session/acp.rs:107-111` hardcodes `agent_type = "acp"` whenever `--agent` is present, so `cru session create --agent <card>` always takes the ACP branch and errors `Unknown ACP agent profile: <card>`. `cru chat` never sends `agent_name` at all. The internal-card branch is reachable only from a raw RPC or from `delegate_session`._
- [x] **Session Persistence** `P0` — conversations saved as append-only JSONL in the kiln; markdown rendered on demand · [[Help/Core/Sessions]] · `crucible-daemon` (observe)
  - **Gets you:** every session leaves a `session.jsonl` on disk that reloads across daemon restarts, and `session.render_markdown` / `cru session show` render it as markdown when you ask. No markdown file is written eagerly.
  - **Proof:** `crates/crucible-daemon/tests/observe_e2e.rs`::test_jsonl_roundtrip, `::test_markdown_export_serde`, `::test_jsonl_file_is_valid_ndjson`; `crates/crucible-daemon/src/observe/session.rs`::test_reopen_session
- [x] **Session Resume** `P0` — load and continue previous sessions with full history · [[Help/Core/Sessions]] · `crucible-daemon` (rpc)
  - **Gets you:** a previously-ended session reloads with its prior events and accepts new turns appended to the same log.
  - **Proof:** `crates/crucible-daemon/tests/observe_e2e.rs`::test_session_resume_append; `crates/crucible-cli/tests/cli_e2e_internal.rs`::session_internal_lifecycle_with_real_daemon (asserts CLI stdout across create→pause→resume against a real daemon)
- [x] **Sessions Are Always Resumable** `P0` — lifecycle state never blocks continuing a conversation · `crucible-daemon`
  - **Gets you:** sending to an ended or evicted session transparently revives it — resident if it is, resumed from storage otherwise, with the kiln resolved via a `session_kilns` index. The session list is global rather than implicitly kiln-scoped, and the live/idle/ended axis is gone from the sessions surface.
  - **Proof:** `crates/crucible-daemon/src/server/tests/persisted_session.rs` (persisted-session revive path); `crates/crucible-web/web/e2e/session-lifecycle.spec.ts`
- [x] **Session Hygiene — Auto-Titles and Auto-Archive** `P0` — daemon-side title sweep and stale-session archiving · `crucible-daemon`, `crucible-web`
  - **Gets you:** an untitled session with content gets a topic-derived title without you doing anything, and stale stored sessions auto-archive (and unarchive) while keeping their files. Web session lists are recency-sorted with an archived section.
  - **Proof:** `crates/crucible-daemon/src/session_manager/tests.rs`::title_sweep_titles_untitled_sessions_with_content, `::test_archive_session_sets_archived_and_keeps_files`, `::test_unarchive_session_sets_archived_false`; `crates/crucible-web/web/e2e/title-generation.spec.ts`
- [x] **Segmented Turn Convergence** `P0` — a persisted `segment_complete` event at each text→tool boundary; backend-canonical message ids · `crucible-daemon`, `crucible-web`
  - **Gets you:** a live viewer, a second pane, and a reload all render byte-identical transcripts — including turns where the agent narrates between tool calls, which used to render the narration twice.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/messaging.rs`::text_then_tool_emits_segment_complete_before_tool_call, `::text_only_turn_emits_no_segment_complete`; `crates/crucible-web/src/events.rs`::real_segment_complete_event_maps_fields_and_name
- [x] **Session-Unique Scratch Workspaces** `P0` — a session created with no explicit workspace gets a private dir at `<session_workspace_dir>/<session_id>` (default `~/.crucible/workspaces`) · `crucible-daemon`
  - **Gets you:** a kiln-less session has a real filesystem containment boundary of its own instead of silently falling back to the kiln. These scratch dirs carry no `.crucible` config, which is what stops a confidential kiln being downgraded to Public at delegation time.
  - **Proof:** `crates/crucible-daemon/src/session_manager/tests.rs`::test_create_session_no_workspace_gets_scratch_dir, `::test_create_session_explicit_workspace_ignores_scratch_dir`; `crates/crucible-daemon/src/scm.rs`::resolves_session_workspace_dir_tilde_and_default
- [x] **Conversation History** `P0` — clear history (`:clear`), resume with prior messages; TUI viewport hydrated from daemon session events · `crucible-cli`, `crucible-daemon`
  - **Gets you:** `:clear` empties the viewport and clears daemon-side history; on resume the viewport is repopulated from replayed session events.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/vocab_tests.rs`::hydrate_from_recording_fills_the_viewport (asserts the rendered frame after replaying a recording)
- [-] **Message Queueing** `P0` — type and queue messages during streaming; Ctrl+Enter force-sends · `crucible-cli`
  - **Gets you:** no queue. Enter during a turn shows a toast ("Turn in progress — Esc cancels, then Enter to send") and keeps your draft; Ctrl+Enter **cancels the stream** and keeps the draft rather than sending it.
  - **Proof:** _none — `chat_app/input_handling.rs:147-149` says so outright ("There is no queue-while-streaming (the deferred message queue was removed)"), and `:129-139` returns `StreamCancelled` for Ctrl+Enter. Two tests already pin the current behaviour: `::enter_while_streaming_preserves_typed_input`, `::ctrl_enter_while_streaming_cancels_and_preserves_input`. The queue was deliberately removed 2026-06-10 and this entry was never updated._

### Agent Runtime

- [x] **Internal Agent** `P0` — built-in agent with session memory and tool access · [[Help/Extending/Internal Agent]] · `crucible-daemon`, `crucible-core`
  - **Gets you:** the built-in agent runs turns through the real scheduler, dispatches tools, and its output text reflects the tool result.
  - **Proof:** `crates/crucible-daemon/tests/security_enforcement.rs`::permissions_config_default_allow_lets_internal_bash_run (asserts the turn's final text contains the bash output, through real dispatch)
- [x] **Multiple LLM Providers** `P0` — unified interface across 8 chat backends (Ollama, OpenAI, Anthropic, Cohere, VertexAI, OpenRouter, GitHubCopilot, ZAI) plus FastEmbed for embeddings · [[Help/Config/llm]] · `crucible-daemon`, `crucible-core`
  - **Gets you:** a session's provider/model resolve to a genai client and the turn streams from that backend.
  - **Proof:** `crates/crucible-daemon/tests/llm_backend_comparison.rs` (live, `#[ignore]`d); client construction `provider/genai_handle.rs:420-520`; credential resolution `agent_factory.rs:439`
- [x] **Model Switching** `P0` — runtime `:model <name>` with autocomplete · `crucible-daemon`, `crucible-cli`
  - **Gets you:** `:model` opens a completion popup listing daemon-supplied models, and selecting one re-resolves the session agent — a cross-provider switch invalidates the handle cache.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/vt100_runtime_tests/spacing.rs`::model_popup_bg_matches_command_prompt_bg (drives `:model`, asserts the popup renders); `crates/crucible-daemon/src/agent_manager/tests/permissions.rs`::switch_model_cross_provider_invalidates_cache
- [x] **Extended Thinking** `P0` — budget presets (off/minimal/low/medium/high/max) via `:set thinkingbudget`; Ctrl+T toggles display · `crucible-daemon`, `crucible-cli`
  - **Gets you:** the preset changes the `ChatOptions` sent to the provider (`with_reasoning_effort(ReasoningEffort::Budget(n))`), and the display toggle changes whether thinking renders expanded or collapsed. This is the only per-turn LLM parameter that survives to the request.
  - **Proof:** display half — `tests/fixture_replay_tests.rs:272-300` (same fixture, `set_show_thinking(false)` collapses to `◇ Thought (N words)`). Budget half is traced end to end: `commands/set.rs:100-112` → `session.set_thinking_budget` RPC → `agent_manager/models.rs:372-397` (writes the field and invalidates the agent cache) → `agent_factory.rs:648` → `provider/genai_handle.rs:993-999` puts it on the outgoing request. No test asserts the outgoing `ChatOptions`; that is a worthwhile hardening test, not a gap in the path.
- [x] **System Prompt** `P0` — layered prompt composition at session creation · `crucible-daemon`, `crucible-core` (config)
  - **Gets you:** the prompt the provider receives is `Workspace:` / `Kiln:` / knowledge-base names / the card-or-config base prompt / the skills catalog, composed in that order, plus a deferral note when tools are deferred.
  - **Proof:** `crates/crucible-daemon/src/agent_factory.rs`:740-800 (`build_enriched_prompt_*` tests assert composition and ordering), traced to `genai_handle.rs:978` where the composed string is the request's `system_prompt`. The "loaded from agent card" half holds only on the card path, which the CLI cannot reach — see **Agent Card Selection from the CLI**.
- [-] **Environment Overrides** `P0` — `--env KEY=VALUE` for per-session env vars · `crucible-cli`
  - **Gets you:** env on a spawned **ACP subprocess** only. On the default internal agent the flag is parsed, logged, threaded through `AgentInitParams` — and then dropped. On a *resumed* session the agent is not reconfigured at all, so `--env` is dropped for ACP too.
  - **Proof:** _none for the internal agent — `factories/agent.rs:427-434` builds the env-carrying session agent only when `is_acp`; the internal branch calls `SessionAgent::internal_from_config`, which hardcodes `env_overrides: HashMap::new()`. Nothing daemon-side consults `SessionAgent::env_overrides` for internal agents; credentials come from the daemon's own process env. The "(e.g., API keys)" framing is actively misleading for the default agent._
- [x] **Agent Cancellation** `P0` — Ctrl+C/Esc cancels the local stream and propagates to the daemon via `session.cancel` · `crucible-daemon`, `crucible-cli`
  - **Gets you:** Esc or Ctrl+C during a turn stops the local stream and fires `session.cancel`; ACP agents additionally receive `session/cancel`.
  - **Proof:** `chat_app/input_handling.rs:116-123` → `chat_runner/actions.rs:227-233` (guarded on `is_streaming`); `crates/crucible-daemon/src/agent_manager/tests/dispatch.rs`::cleanup_session_cancels_pending_requests; ACP propagation `acp_handle.rs:394-418`
- [x] **Error Handling UX** `P0` — toast notifications, contextual messages, graceful degradation for DB lock / search / kiln fallback, retryability classification, and transparent retry for idempotent daemon RPCs · `crucible-cli`, `crucible-core`, `crucible-daemon` (rpc)
  - **Gets you:** toasts render and expire in the TUI, and a timed-out idempotent RPC retries transparently instead of surfacing an error.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/notification_tests.rs` (rendered-frame assertions); `crates/crucible-daemon/src/rpc_client/client/mod.rs`:391-428 (`typed_call_with_retry` retries twice with 200/400 ms backoff on transient patterns and returns immediately on application-level RPC errors); `is_retryable`/`retry_delay_secs` unit-tested on `EmbeddingError` and `StorageError`. Note the entry's old `BackendError` / `McpProxyTool` names do not exist in the tree.

### Tools & Permissions

- [x] **Tool Calls** `P0` — inline tool execution with streaming results; batched calls correlated by `call_id` · `crucible-daemon` (tools), `crucible-core`
  - **Gets you:** each tool call streams a `tool_call` then a `tool_result` keyed by `call_id`, and the TUI renders one row per `call_id` with the result landing in it. A batch of provider-emitted parallel calls is correctly correlated — but the stream loop dispatches them **sequentially**, one `.await` per call.
  - **Proof:** `crates/crucible-daemon/tests/progressive_disclosure_test.rs`::discovery_bridge_finds_inspects_and_invokes_a_tool (real dispatch over real files); `crates/crucible-cli/src/tui/oil/tests/user_story_tests/permission_tests.rs`::approve_lets_tool_result_render (rendered frame)
- [x] **Permission System** `P0` — an ordered layer stack decides allow / deny / prompt · [[Help/Concepts/Permission Precedence]] · `crucible-daemon`
  - **Gets you:** a tool call is allowed, denied with an agent-visible error, or prompted, and the decision changes the turn's output text. The real order is: `is_safe()` gate-entry check → `--permissions` CLI override → global `[permissions]` engine (deny absolute, allow short-circuits) → saved `PatternStore` patterns → Lua `on_request` hooks → mode rules then mode default stance (deliberately *after* hooks, so a user hook beats `cru.modes.auto`) → non-interactive immediate deny → user prompt with a 300 s deny timeout.
  - **Proof:** `crates/crucible-daemon/tests/security_enforcement.rs`::permissions_config_deny_blocks_internal_agent_bash, `::permissions_config_default_allow_lets_internal_bash_run`, `::non_interactive_unsafe_tool_without_policy_is_denied_not_hung`, `::card_allow_does_not_override_config_deny` — all run a real turn and assert the agent's final text; gate boundary `messaging/gate_decision.rs::an_mcp_read_only_hint_cannot_skip_the_permission_gate`. Known hole: `DaemonPermissionGate` (the ACP path) returns allow for `is_safe(tool)` *before* consulting the engine, so a config `deny = ["read:*"]` cannot block `read_file` for an ACP agent.
- [x] **Pattern Whitelisting** `P0` — "always allow" saves project-scoped patterns for future sessions · `crucible-daemon`
  - **Gets you:** choosing "always allow" writes a pattern into the project's `PatternStore` on disk, and a later call matching it skips the prompt entirely.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/permissions.rs`::store_pattern_outcomes (writes to a real temp project, reloads a fresh store, asserts `matches_bash`/`matches_file`/`matches_tool`; bare `*` rejected)
- [x] **Permission Hooks (Lua)** `P0` — custom Lua hooks can Allow/Deny/Prompt, with a 1 s budget · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a `cru.permissions.on_request` hook returning `{deny=true}` blocks the tool with an agent-visible error and **overrides the shipped auto-approve**. The 1 s figure is a budget, not a timeout: hooks run synchronously and elapsed time is checked *after* they return, so a hook that blocks for 60 s blocks the turn for 60 s and its answer is then discarded.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/init_lua_defaults.rs`::a_user_hook_overrides_the_shipped_auto_approve (writes a real `.crucible/lua/init.lua`, runs the real hook path, asserts `Deny` in `auto` mode) and `::the_shipped_permission_hook_registers_behind_user_hooks`
- [x] **Permission Prompt Serialization** `P0` — prompts open one at a time in arrival order, with a 300 s deny timeout · `crucible-daemon`, `crucible-cli`
  - **Gets you:** a parallel ACP tool batch produces permission prompts **one at a time** rather than all at once, and an unanswered prompt denies after 300 s instead of wedging the turn. The documented tradeoff: a walked-away-from prompt blocks the queue for the full 300 s.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/permission_tests.rs`::queued_permissions_open_in_arrival_order; `PermissionSerializer` at `agent_manager/messaging/permission.rs:27-46`
- [x] **Diff Synthesis in Permission Prompts** `P0` — a write/edit prompt shows the diff the tool would apply · `crucible-daemon`
  - **Gets you:** you approve a change by looking at the change, not at a tool name and an argument blob.
  - **Proof:** `crate::tools::diff_synth::synthesize_diffs` called at `agent_manager/messaging/permission.rs:885-891`, carried on the request via `PermRequest::with_diffs`, with late content patched in through the `tool_call_diff_update` event at `stream.rs:819`
- [x] **Interaction System** `P0` — `InteractionRequest` carries permission requests from the agent to whichever client is attached · `crucible-core`, `crucible-daemon`
  - **Gets you:** a gated tool call opens a permission modal showing the command and any synthesized diff; `y`/`n` resolve it and the turn continues accordingly.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/permission_tests.rs`::permission_modal_opens_and_shows_command, `::approve_lets_tool_result_render`, `::deny_emits_deny_and_turn_continues_with_error`
- [-] **Agent-Initiated Questions** `P0` — the agent asks the user a question (single-select, multi-select, free-text) mid-turn · `crucible-core`, `crucible-daemon`
  - **Gets you:** nothing. The modal renders one if handed one, but nothing in the agent path ever hands it one.
  - **Proof:** _none — there is no `ask` tool: the dispatcher's arm list has no question tool and neither does `PLAN_TOOL_NAMES`. Daemon-side, the only non-test `InteractionRequest::` constructions are `::Permission`; the one `ask` producer is a debug RPC emitting hardcoded "Option A/B/C". `cru.interaction.ask` in Lua only builds a table, and no caller emits it, awaits it, or routes a response back. `AskBatch`, `Edit`, `Show`, `Popup` and `Panel` have no producer either._
- [x] **Delegation** `P1` — `delegate_session` spawns a child agent reusing the ordinary session/task primitives · [[Help/Concepts/Delegation]] · [[Help/Delegation Patterns]] · `crucible-daemon`
  - **Gets you:** a real parent-linked child session; `subagent_spawned`/`completed`/`failed` events stream to the parent; the child's output comes back; and depth, allowlist, self-delegation, concurrency, timeout and data-classification trust are each enforced with agent-visible errors. Supervisor/router/broadcast are Lua recipes over `cru.sessions.*`, not built-ins.
  - **Proof:** `crates/crucible-daemon/tests/delegation_integration.rs`::blocking_delegation_completes_with_result_and_events, `::child_is_a_real_parent_linked_session_and_ends_on_completion`, `::depth_limit_blocks_nested_delegation`, `::target_not_in_allowlist_is_rejected_and_allowlist_requires_target`, `::self_delegation_is_rejected`, `::concurrency_limit_enforced_and_freed_after_cancel`, `::delegation_timeout_cancels_child_and_fails`, `::parent_cleanup_cancels_running_children`; trust in `tests/security_enforcement.rs::delegation_trust_derives_from_child_provider`
- [x] **Hidden Child Sessions** `P1` — delegated children are real sessions, excluded from `session.list` unless asked for · `crucible-daemon`
  - **Gets you:** your session list is not polluted by every subagent, but `cru session list --include-children` shows them; they link via `parent_session_id` and are ended, archived, deleted and cancelled together with their parent.
  - **Proof:** `crates/crucible-daemon/tests/delegation_integration.rs` (production-wiring e2e); `include_children` param on `session.list` in `rpc/dispatch.rs`
- [x] **Background Bash Jobs** `P0` — `list_jobs`, `get_job_result`, `cancel_job` over a `BackgroundJobManager` · `crucible-daemon` (tools)
  - **Gets you:** an agent (internal or external over MCP) starts long shell work in the background, lists what is running, fetches a result, and cancels one. This is what the old "Subagent Spawning" entry actually described — the manager spawns bash only; delegated children are scheduler-driven sessions (see **Delegation**).
  - **Proof:** `crates/crucible-daemon/src/tools/mcp_server.rs`::test_list_jobs_returns_jobs_for_session, `::test_list_jobs_without_context_returns_error`; exposure asserted in `crates/crucible-daemon/tests/mcp_server_tools_test.rs`
- [x] **Repeat-Failure Tool Blocking** `P0` — a tool that keeps failing within a stream is blocked for the rest of it · `crucible-daemon`
  - **Gets you:** the agent stops looping on a broken tool — further calls return "Tool 'X' is blocked for this stream after repeated failures." instead of executing.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/messaging/stream.rs`:573-604 (blocked-list check emits an error `tool_result` event) with `agent_manager/tool_tracking.rs`
- [x] **Security Enforcement** `P0` — permissions config, shell policy, filesystem containment, derived delegation trust · `crucible-daemon`, `crucible-core`
  - **Gets you:** `[permissions]` config is enforced for internal agents (not just ACP) and config `deny` beats an agent card's `allow`. `[security.shell]` policy applies to `bash`, checked per chained statement. Workspace file tools are contained to workspace + kilns + session dir with symlink, `..` and glob escapes blocked. Delegation trust derives from the target's actual provider, so a local-model card can serve a confidential kiln while cloud targets stay blocked. Non-interactive sessions deny would-prompt tools immediately instead of hanging.
  - **Proof:** `crates/crucible-daemon/tests/security_enforcement.rs`::permissions_config_deny_blocks_internal_agent_bash, `::card_allow_does_not_override_config_deny`, `::shell_policy_blacklist_blocks_bash_tool`, `::shell_policy_checks_each_chained_statement`, `::workspace_symlink_escape_is_contained`, `::glob_pattern_cannot_escape_containment`, `::delegation_trust_derives_from_child_provider`, `::non_interactive_unsafe_tool_without_policy_is_denied_not_hung`
- [x] **MCP Tool System** `P0` — `PermissionGate` trait, ACP integration, gateway tool definitions injected per session · `crucible-daemon` (tools, acp)
  - **Gets you:** unsafe tool calls prompt or are denied, and the decision the user makes is what executes.
  - **Proof:** `crates/crucible-daemon/tests/permission_gate_contract_tests.rs`::contract_prompt_callback_decision_is_respected, `::contract_unsafe_actions_are_denied_without_interactive_callback`, `::contract_permission_override_deny_blocks_even_safe_patterns`, `::agent_deny_rules_enforced_even_with_allow_default` (11 contract tests). Note this gate is on the *agent dispatch* path only — `cru.tools.call` routes around it.
- [x] **Per-session MCP Servers** `P0` — agent cards name MCP servers; `mcp_servers` propagates to `SessionAgent` and filters the gateway tool set · `crucible-daemon`
  - **Gets you:** an agent card naming server `gh` gets `gh`'s gateway tools and no others.
  - **Proof:** `crates/crucible-daemon/src/agent_factory.rs`::over_budget_agent_attaches_core_plus_bridge_and_plan_excludes_gateway (12 `upstream: "gh"` tools all resolved, which only holds if the `server_names` filter matched); card → session propagation in `crucible-core/src/session/types/tests/agent.rs:23-50`

### Tool Discovery & Disclosure

> Agents shouldn't carry every tool schema in context. Discovery tools let an agent find tools on demand; progressive disclosure makes that automatic when the tool set is large.

- [x] **Tool Discovery** `P1` — `discover_tools` and `get_tool_schema` let an agent enumerate and inspect tools at runtime · `crucible-daemon` (tools)
  - **Gets you:** `discover_tools("glob")` returns a result naming `glob`; `get_tool_schema("glob")` returns its `pattern` parameter; an unknown name errors.
  - **Proof:** `crates/crucible-daemon/tests/progressive_disclosure_test.rs`::discovery_bridge_finds_inspects_and_invokes_a_tool (real dispatcher over a real temp dir; the discovered tool is then executed and its output asserted) and `::discovery_bridge_reports_unknown_tool_schema_as_error`
- [x] **Progressive Tool Disclosure** `P2` — automatic deferral when mode-filtered tool schemas exceed 15% of the effective context budget · `crucible-daemon` (tools)
  - **Gets you:** deferrable (gateway/user MCP) tools drop out of the request and are replaced by the `discover_tools` → `get_tool_schema` → `invoke_tool` bridge, with a deferral note added to the system prompt. Kiln and workspace tools are never deferred; `invoke_tool` is unwrapped to the inner tool before hooks and permissions, and plan mode cannot be escaped through it.
  - **Proof:** `crates/crucible-daemon/src/provider/genai_handle.rs`::visible_tools_over_budget_defers_gateway_and_adds_bridge, `::visible_tools_under_budget_attaches_all_and_no_bridge`, `::visible_tools_plan_mode_filters_writes_and_defers`; bridge unwrapping in `agent_manager/messaging/tool_call.rs mod invoke_tool_tests`. The 15% share is computed against `context_budget`, which the factory now supplies from the session (it used to be permanently unset, so the decision always fell back to `DEFAULT_ASSUMED_CONTEXT` — see **Context Strategies**).

### Agent Skills

> Skills are markdown capability docs ([agentskills.io](https://agentskills.io)-compatible `SKILL.md` + optional `scripts/`, `references/`) that teach the agent procedures on demand. Discovery, parsing and daemon-side context injection all ship.

- [x] **Skill Discovery** `P1` — folder discovery across search paths, `SKILL.md` frontmatter parsing, scope precedence, `cru skills` CLI · [[Help/Concepts/Agent Skills]] · [[Help/CLI/skills]] · `crucible-daemon` (skills), `crucible-cli`
  - **Gets you:** skills under the personal / workspace / kiln search paths are found, parsed, shadowed by scope precedence, and listed by `cru skills list|show|search`. A symlinked `SKILL.md` is rejected and files are capped at 256 KB. Cross-harness discovery (`~/.claude/skills` and friends) is opt-in behind `CRUCIBLE_CROSS_HARNESS_SKILLS`.
  - **Proof:** `crates/crucible-daemon/tests/skills_discovery_tests.rs`::test_discover_skills_in_single_directory, `::test_priority_ordering_higher_scope_wins` (real dirs on disk); CLI at `crates/crucible-cli/src/commands/skills.rs`:32-160
- [-] **Bundled Help Skills** `P1` — help skills shipped at `runtime/crucible-help/skills` · `crucible-daemon` (skills)
  - **Gets you:** the skills load from a dev tree, from `$CRUCIBLE_RUNTIME`, and from an installed `<prefix>/share/crucible/runtime/` layout. Still nothing from a release tarball, which contains no `runtime/` at all — see **Bundled Runtime Plugins in Releases**.
  - **Proof:** `crates/crucible-daemon/src/skills/discovery.rs`::an_installed_binary_finds_the_bundled_runtime_skills, `::a_dev_tree_binary_still_finds_the_bundled_runtime_skills`. The resolver half is fixed: it looked only at `$CRUCIBLE_RUNTIME` and `<exe_dir>/../../runtime`, omitting the installed layout the other resolvers tried first, and all four now share `crucible_core::runtime_roots`. _Still `[-]` because the packaging half is untouched — with nothing at that path there is nothing to find, so an installed user's observable outcome is unchanged._
- [x] **Skill Context Injection** `P1` — the tier-1 skills catalog is rendered into the daemon's enriched system prompt; full `SKILL.md` loads on demand via `skill_view` · `crucible-daemon` (skills)
  - **Gets you:** the agent sees a name+description catalog in its system prompt and can pull a skill's full body when it decides to (list → view → use). It is injected **only when the session has a kiln**, because `skill_view` is kiln-scoped — so a session in a plain project dir advertises no skills at all.
  - **Proof:** `crates/crucible-daemon/src/agent_factory.rs`:758-770 (`build_enriched_prompt` asserts the catalog appears after the base prompt); `crates/crucible-daemon/src/tools/mcp_server.rs`::skill_view_finds_workspace_and_kiln_skills, `::skill_view_appends_allowed_tools_advisory`
- [ ] **Skill Self-Creation** `P2` — agent-authored skills distilled from successful sessions (ties into the Reflection Pass); provenance separating agent-created from user-authored · `crucible-daemon` (skills)

### Context & Knowledge

- [x] **File Attachment** `P0` — `@file` context attachment in chat, resolved daemon-side so every client gets it · `crucible-cli`, `crucible-daemon`
  - **Gets you:** `@`-picking (or typing) a workspace file puts its contents in front of the agent as a tagged system block for that turn, so the agent does not have to go read it. Workspace-relative only, deduped, truncated past 64KB per file, and `user@example.com` is not a file.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/messaging.rs`::at_mention_attaches_the_file_contents_to_the_turn (asserts on the messages a `PromptCapturingAgent` actually receives, not on any TUI field), plus the resolution rules in `crates/crucible-daemon/src/agent_manager/attachments.rs`::mentions_outside_the_workspace_are_refused, `::an_email_address_is_not_an_attachment`, `::an_oversized_file_is_truncated_rather_than_dropped`
- [x] **Rules Files** `P0` — project-level AI instructions (`AGENTS.md`, `.rules`, `.github/copilot-instructions.md` by default; `[context] rules_files` to change the set) loaded into the system prompt, hierarchically from the repo root down to the workspace · [[Help/Rules Files]] · `crucible-core` (config), `crucible-daemon`
  - **Gets you:** instructions in your project's `AGENTS.md` are in the agent's system prompt under `# Project rules`, after the agent card's own prompt, with a rules file nearer the workspace read later and so winning.
  - **Proof:** `crates/crucible-daemon/src/agent_factory.rs`::rules_file_contents_reach_the_system_prompt (asserts through `create_agent_from_session_config` — the call `send_message` makes — on the prompt the built handle reports), `::rules_files_load_from_repo_root_down_to_the_workspace`, `::no_rules_configured_means_no_rules_section`
- [x] **Multi-Kiln Sessions** `P0` — extra knowledge kilns attach at creation or mid-session · `crucible-daemon`, `crucible-web`
  - **Gets you:** `session.connect_kiln` / `disconnect_kiln` / `set_workspace` change a live session's knowledge scope, and the kiln is optional everywhere — a kiln-less session resolves the home-kiln default daemon-side. Attaching re-runs data-classification trust checks *before* opening the kiln, so a rejected attach leaves no trace.
  - **Proof:** `crates/crucible-web/tests/route_contract_tests/sessions.rs`::set_workspace_attaches_project_dir, `::connect_kiln_returns_scope_shape`; RPC dispatch for `session.connect_kiln` / `session.disconnect_kiln`. The scope-mutation path is proven; the *fan-out search across primary + connected kilns with source labels* is not separately asserted.

### Core Agent Features

> Core capabilities implemented in Rust rather than as plugins, with hook points where Lua can override.

- [x] **Auto-Linking** `P1` — `suggest_links` detects unlinked mentions of existing notes via word-boundary matching · `crucible-daemon`, `crucible-web`
  - **Gets you:** the web backlinks panel's "unlinked mentions" list, where clicking **Link** rewrites the open editor buffer. Case-insensitive, skips already-linked targets. This is web-only — there is no TUI or CLI entry point and the internal agent has no autolink tool.
  - **Proof:** `crates/crucible-web/tests/route_contract_tests/kilns.rs`::backlinks_returns_linked_and_filtered_unlinked (asserts the `unlinked` array's targets and offsets, self-mention filtered); matching semantics at `crates/crucible-daemon/src/tools/autolink.rs`:160-290

### Lua Session API

- [x] **Scripted Agent Control (session VM)** `P0` — Lua control of `thinking_budget` and `mode` from the TUI's session VM · `crucible-lua`, `crucible-cli`
  - **Gets you:** setting the thinking budget changes the provider's reasoning effort, and setting the mode changes which tools the agent can see — including on a cached live handle. `session.model` is a getter only (assignment raises "model is read-only"; use `:model`). Daemon getters read a local cache.
  - **Proof:** `crates/crucible-daemon/src/provider/genai_handle.rs`:993-999 (thinking budget becomes `ReasoningEffort::Budget` on the outgoing request); `crates/crucible-daemon/src/agent_manager/tests/models/mode.rs`::set_mode_applies_to_cached_live_handle, `::set_mode_emits_mode_changed_event`
- [-] **Scripted Agent Control (daemon plugin VM)** `P0` — the same `cru.get_session()` surface from a daemon-side plugin · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a visible error. Setters now raise `"<field>: not supported on this session"` instead of reporting success and changing nothing; getters still return defaults, so `s.mode` reads `"chat"` — a mode id not in the registry.
  - **Proof:** `crates/crucible-lua/src/session_api.rs`::an_unimplemented_setter_reports_that_it_is_unsupported. `SessionConfigRpc`'s setters defaulted to `Ok(())` so that stubs needed no boilerplate, which made the trait a machine for producing this codebase's repeat bug: `NoopSessionRpc` is `impl SessionConfigRpc for NoopSessionRpc {}` and is bound at every daemon site (plugin `session_start`, plugin `session_end`, `lua.init_session`). _Still `[-]`: there is still no daemon-side implementation, so the assignment reaches nothing — it just no longer claims otherwise. The separate and working surface is the hook parameter: `cru.on_session_start(function(session) … end)` binds `SessionDefaultsRpc` and does reach `SessionAgent` — two different Lua objects both spelled `session`._
- [x] **Lua `temperature` / `max_tokens`** `P0` — per-session sampling knobs from Lua · `crucible-lua`, `crucible-daemon`
  - **Gets you:** the value reaches the outgoing request. Same for every other writer of those two fields — `session.set_temperature`, agent-card frontmatter, `[llm] temperature` — since they all invalidate the agent cache and the handle is rebuilt from `SessionAgent`.
  - **Proof:** `crates/crucible-daemon/src/provider/genai_handle.rs`::generation_settings_reach_the_outgoing_chat_options (asserts `ChatOptions`, not a getter — a round-trip is exactly what hid this), `crates/crucible-daemon/src/agent_factory.rs`::session_generation_and_context_settings_reach_the_agent_handle. `GenaiAgentHandle` had no field for either and `ChatOptions` carried only capture flags and `reasoning_effort`, so the Lua surface, `SessionAgent`, agent-card frontmatter and RPC validation were five layers over nothing.
- [x] **Session Event Handlers** `P0` — Lua hooks on `turn:complete` can inject follow-up messages · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a handler returning `{ inject = { content = "..." } }` causes the agent to run another turn with that content as the message — the user sees a second streamed response.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/dispatch.rs`::plugin_vm_turn_complete_handler_fires_and_injects (fires on a real plugin VM + registry pair) and `::plugin_inject_overrides_session_inject`; the returned content is passed unconditionally as the message of a recursive `execute_agent_stream` at `messaging/stream.rs:964-1007`

### Lua Session & Tool Primitives

> These fill gaps so autonomous loops, fan-out, and context control are trivial plugins — not bespoke features.

- [x] **`cru.tools.call(name, args)`** `P1` — programmatic tool calling from Lua · `crucible-lua`, `crucible-daemon` (tools)
  - **Gets you:** a workspace tool executes and returns its output to Lua, subject to the operator's `[permissions]` rules. A `deny` is absolute; an `allow` runs; anything the rules leave at `ask` falls back to the read-only exemption, so a plugin can `read_file`/`grep` unconfigured but needs an explicit `allow` for `bash` or `write_file` — there is no prompt to fall back on from a Lua call.
  - **Proof:** `crates/crucible-daemon/src/tools_bridge.rs`::a_denied_tool_is_refused_through_the_lua_bridge, `::a_mutating_tool_needs_an_explicit_allow`, `::an_operator_deny_beats_the_read_only_exemption`, `::a_read_only_tool_still_executes_through_the_lua_bridge`; execution itself `tests/acp_integration_e2e.rs::test_tools_bridge_call_tool_routes_correctly`. The bridge used to construct `ExecutionContext::default()` and call `WorkspaceTools::execute_tool` directly — no session id, no gate — so any loaded plugin could run `bash` unprompted in any mode, including plan. Not an inert API but its inverse, which is why ranking by "does it reach the user" never surfaced it.
- [x] **`cru.tools.batch({...})`** `P1` — concurrent multi-tool calls · `crucible-lua`, `crucible-daemon` (tools)
  - **Gets you:** N tools execute concurrently from one Lua call and per-entry `{result=…}` / `{err=…}` come back, with error isolation between entries.
  - **Proof:** `crates/crucible-lua/src/tools_api.rs`::tools_batch_returns_all_results, `::tools_batch_handles_mixed_success_and_error` (assert the Lua-visible result table); underlying execution `tests/acp_integration_e2e.rs::test_tools_bridge_call_tool_routes_correctly`; concurrency is a real `join_all`. Gated per entry through the same `DaemonToolsBridge` gate as `cru.tools.call`, since batch fans out to `call_tool`.
- [x] **`cru.tools.list()`** `P1` — enumerate workspace tool definitions from Lua · `crucible-lua`, `crucible-daemon`
  - **Gets you:** name, description and parameters for every workspace tool, so a plugin can decide what to call.
  - **Proof:** `crates/crucible-daemon/tests/acp_integration_e2e.rs`::test_tools_bridge_list_tools (asserts a non-empty list, each entry carrying `name`)
- [x] **`cru.sessions.messages(id, opts)`** `P1` — read conversation history from Lua; opts `{role, limit}` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** the session's real `{role, content, timestamp}` history, role-filtered and limited — enabling context windowing, summarization, checkpoint detection.
  - **Proof:** `crates/crucible-daemon/src/session_bridge.rs`:265-335 reads the real session dir via `observe::load_events`, validates the role filter and applies the limit; Lua-side shape in `crates/crucible-lua/src/sessions/tests/messages.rs`. The bridge itself has no direct test against a real `SessionManager` — the filtering the Lua tests assert lives in the mock.
- [-] **`cru.sessions.inject(id, role, content)`** `P1` — insert messages mid-conversation · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a line appended to the session's JSONL. The in-flight conversation is unchanged, and the broadcast event reaches nobody.
  - **Proof:** _none — `inject_context_impl` writes the event log and never touches `StreamContext::conversation_tree`, which is what the agent's next LLM call is built from; the `context_injected` event has zero consumers in the whole repo. Use `cru.context.attach` for this-turn context; `inject` is the persisted log only._
- [-] **`session.fork()`** `P1` — `cru.sessions.fork(id, opts)` for parallel exploration and A/B testing · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a child session with copied message history and **no agent config** — no model, no provider, no system prompt. Parallel exploration does not work from Lua as advertised.
  - **Proof:** _none — the Lua path goes through `DaemonSessionBridge::fork_session`, whose own doc comment says it "does not copy agent configuration (no AgentManager access)"; only the RPC handler `handle_session_fork` has the `configure_agent` step. The two implementations are otherwise near-identical copy-paste, and neither has an integration test._
- [-] **`cru.sessions.collect_subagents(ids, timeout?)`** `P1` — await multiple subagents with an optional timeout · `crucible-lua`, `crucible-daemon`
  - **Gets you:** unproven. The chain is real and traceable to a 100 ms poll loop in `BackgroundManager::collect_jobs`, but nothing tests it at any layer.
  - **Proof:** _none — `grep collect_jobs` returns four hits, all call sites, zero assertions; the `subagent.collect` RPC's only test asserts the method name is registered. This is the primitive both **Delegation** and the retired Team Patterns note point users at as the fan-out story, and it is the least-covered thing in the Lua surface._
- [-] **`cru.sessions.subscribe` / `unsubscribe`** `P1` — stream a session's events live from a Lua plugin · `crucible-lua`, `crucible-daemon`
  - **Gets you:** unverified. The surface is registered and is the primitive `send_and_collect` builds on, but nothing observes a real event reaching a Lua handler.
  - **Proof:** _none beyond mock-backed async tests (`crates/crucible-lua/src/sessions/tests/subscription.rs`); it needs its own verification pass before it earns an `[x]`._

## Terminal Interface (TUI)

### Modes & Input

- [x] **Chat Modes** `P0` — Lua-declared modes; `normal` / `plan` / `auto` ship as defaults. Badge, cycling and per-mode slash command are all derived from the daemon's list · [[Help/TUI/Modes]] · `crucible-cli`, `crucible-daemon`
  - **Gets you:** the statusline renders a coloured badge for the session's mode — including a mode the TUI has never heard of, because `session.list_modes` **replaces** the built-in defaults wholesale. Shift+Tab advances through whatever the daemon offers, wrapping. Every declared mode gets its own slash command for free, so a user-declared `review` gets `/review`. A mode change made in another client (web, second TUI) updates this client's badge. Plan mode offers only read-shaped tools and denies a mutating tool that reaches the gate, enforced in three layers, one of which is unconditional Rust.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/message_routing_tests.rs`::a_lua_declared_mode_reaches_the_statusline (real Vt100 frame, asserts `REVIEW` after `ModesLoaded` + `ModeSynced`) and `::a_mode_change_from_another_client_updates_the_mode` (which also pins the no-echo invariant); badge styling in `tests/component_isolation_tests.rs::mode_badge_colors_include_bg_fg_and_bold`; plan enforcement in `crates/crucible-daemon/src/agent_manager/tests/init_lua_defaults.rs`::the_auto_mode_stance_is_allow_and_plan_is_deny plus `agent_manager/messaging/tool_call.rs:797-803`. `ChatMode` the enum no longer exists — mode is a `&str` id end to end. No test presses BackTab; both ends of that two-line chain are asserted separately.
- [-] **Auto Mode Approval** `P0` — in `auto` mode a tool runs without a permission modal · `crucible-daemon`
  - **Gets you:** unproven. The mechanism is implemented as data (the `auto` mode's stance is `Allow` in `runtime/defaults/init.lua`), which is a real improvement — auto mode was previously not implemented at all — but nothing watches the effect.
  - **Proof:** _none — the one test asserts `agent_manager.mode_stance("auto") == ModeStance::Allow`, a registry read whose accessor is documented as test-only, so it never traverses the path a user's request takes. `handle_permission_request` has no test at any level, and the `ModeStance::Allow` arm that actually skips the prompt is exercised by nothing._
- [x] **Input Modes** `P0` — Normal (`>`), Command (`:`), Shell (`!`) input · [[Help/TUI/Commands]] · `crucible-cli`
  - **Gets you:** the input prompt glyph and background change between the three modes, and the prefix is stripped from the displayed text.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/component_isolation_tests.rs`:456/`:465`/`:482` (`render_to_plain_text` asserting ` > ` / ` : ` / ` ! `), three committed input-area snapshots, distinct backgrounds asserted in ANSI at `:520`, and full-frame corroboration in `tests/vt100_runtime_tests/spacing.rs:450`
- [-] **Slash Commands** `P0` — `/mode`, `/default`, `/undo`, `/help`, one command per declared mode, plugin-registry commands, then forward-to-agent · `crucible-cli`
  - **Gets you:** local dispatch for the built-ins and for each declared mode, plugin commands routed through the registry, and anything else forwarded to the agent. Two corrections to the old text: **`/quit` does not exist** — there is no arm for it, so it is forwarded to the agent as a chat message (only `:quit`/`:q` quits) — and `/plan` `/auto` `/normal` are not fixed commands, they exist only because the daemon's mode list seeds them, so a Lua config omitting `plan` deletes `/plan`.
  - **Proof:** _none for the forwarding half — every test agent's `send_message_fire_and_forget` is a bare `Ok(())`, so no test records what was forwarded; one "test" greps `actions.rs` source with `include_str!`. Dispatch tests assert the emitted `Action` enum, never run through `process_action`, and no slash command has a rendered-output test. Shadowing order (built-ins beat modes beat plugins) is state-tested._
- [-] **REPL Commands** `P0` — `:quit`, `:help`, `:clear`, `:model`, `:set`, `:export`, `:messages`, `:mcp`, `:config`, `:palette`, plus `:lua`/`:=`, `:pick`, `:plugins`, `:reload`, `:undo` · [[Help/TUI/Commands]] · `crucible-cli`
  - **Gets you:** all of them dispatch — but only `:mcp` is proven to put anything on screen.
  - **Proof:** _none for nine of ten of the originally-listed commands — `:quit` `:clear` `:config` `:export` `:model` assert only the returned `Action`/`ChatAppMsg`; **`:help` has no test at all**; `:palette` sets popup state and nothing asserts the palette paints; `:messages` drawer contents are rendered-tested only via a direct setter that bypasses the command. `:mcp` is the exception (`tests/user_story_tests/subagent_mcp_tests.rs:126` types it into `StoryRuntime` and asserts the real screen) and is a nine-line template for the rest. Bare `:export` with no path has no arm and falls to the unknown-command warning. `:messages` also answers to `:msgs`/`:notifications`; `:palette` to `:commands`._
- [-] **Runtime Config (`:set`)** `P0` — vim-style `:set` with enable/disable/toggle/reset/query/history · [[Help/TUI/Commands]] · `crucible-cli`
  - **Gets you:** parsing and mutation of the config overlay, with more suffix forms than the entry ever documented: `??` query-history, `?` query, `&` reset, `^` pop-one-layer, `!` toggle, `inv` and `no` prefixes, bare `:set` for modified-only, `:set all`. **The `<` suffix this entry used to claim is not implemented** — a trailing `<` parses as `Enable { key: "foo<" }`. The `^` form is vim's `<`.
  - **Proof:** _none — the query answer is never proven on screen: `overlay.rs:602` asserts `format_query` returns the right string and the dispatch test asserts only `Action::Continue`, discarding the text. Nothing types `:set foo?` and looks at a frame. Parse-level coverage is thorough._
- [-] **Double Ctrl+C Quit** `P0` — first clears input or shows a warning; second within 300 ms quits · `crucible-cli`
  - **Gets you:** non-empty input is cleared first and two presses quit. The 300 ms window and the warning toast are both unproven.
  - **Proof:** _none for the timing or the toast — the two test presses are back-to-back so the >300 ms branch never runs (deleting the `if` keeps both tests green), and the "Ctrl+C again to quit" toast is only ever asserted in a hand-built status bar, never connected to `handle_ctrl_c`. 300 ms is a magic literal, not a named constant._
- [x] **Undo of Agent Edits** `P0` — `/undo [N]` and `:undo [N]` from the TUI · `crucible-cli`
  - **Gets you:** the last N agent file edits revert, with a toast reporting turns and messages removed.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/undo_tests.rs`::undo_toast_reports_turns_and_messages and `::undo_flow_frame_sequence` with a committed frame-sequence snapshot; dispatch at `chat_app/command_handling.rs:156`, `:306`. See **Turn Undo** for the viewport caveat.

### Streaming & Display

- [-] **Streaming Display** `P0` — real-time token streaming with cancel (Esc/Ctrl+C) · `crucible-cli`
  - **Gets you:** tokens appear in the terminal as they stream, and a cancelled stream graduates cleanly with its partial text. The cancel *keys* are the unproven part.
  - **Proof:** _none for the key binding — every cancel test injects `ChatMsg::StreamCancelled` directly, bypassing the Esc/Ctrl+C binding at `chat_app/input_handling.rs:116-123`. Streaming itself is strongly proven (`tests/vt100_runtime_tests/spacing.rs::graduated_thinking_scrolls_off_top_row_during_long_stream` renders a real frame per delta across 60 deltas), and so is the cancel effect — only the key→message hop is unwatched, and Esc-during-streaming is a first-session interaction._
- [x] **Streaming Graduation** `P0` — drain-based: completed containers render through Taffy and write to stdout (terminal scrollback); the viewport shows only live content · `crucible-cli`
  - **Gets you:** finished turns leave the viewport and land in real terminal scrollback, collapsed (thinking becomes `◇ Thought (N words)`), spinner-free, and byte-identical to what the viewport rendered.
  - **Proof:** `crates/crucible-oil/src/planning.rs`::plan_frame_graduation_produces_stdout_delta and `::graduation_and_viewport_emit_byte_identical_output_for_same_tree`; terminal side `crates/crucible-cli/src/tui/oil/tests/graduation_tests.rs`::graduated_thinking_is_collapsed, `::graduation_preserves_content`, `::graduation_preserves_tool_results`; "viewport shows only live content" pinned at `tests/vt100_runtime_tests/spacing.rs:519`
- [x] **Thinking Display** `P0` — streaming thinking blocks with a word count · `crucible-cli`
  - **Gets you:** thinking streams live as `Thinking… (N words)` and graduates to a collapsed `◇ Thought (N words)`. The count is a **word** count over the accumulated text — the old "counts delta messages, not actual tokens" caveat is obsolete and has been dropped.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/fixture_replay_tests.rs`::styled_snapshot_thinking_collapsed (Vt100 + styled screen capture; the committed snapshot contains `◇ Thought` and `(10 words)`); `components/thinking_component.rs::live_collapsed_with_words_shows_count`
- [-] **Thinking Toggle** `P0` — Ctrl+T and `:set thinking` show/hide thinking blocks · `crucible-cli`
  - **Gets you:** unproven on both routes.
  - **Proof:** _none — `toggle_thinking_with_toast` has zero test references and no test sends Ctrl+T at all; every test that changes the value calls the direct setter `app.set_show_thinking(...)`, skipping the `:set` path entirely, so nothing asserts that route reaches the renderer._
- [x] **Markdown Rendering** `P0` — full markdown-to-node rendering with styled output · `crucible-cli`, `crucible-oil`
  - **Gets you:** bold/italic ANSI, bullets, blockquote bars, box-drawn tables, and syntax-highlighted code in the terminal.
  - **Proof:** `crates/crucible-cli/src/tui/oil/markdown/tests.rs`::styled_text_wraps_correctly (asserts the `\x1b[1m` / `\x1b[3m` codes), `::test_bullet_list`, `::test_blockquote`, `::test_table`, plus panic-safety fuzzing in `tests/markdown_fuzz_tests.rs`. All rendered evidence is component-level; no test asserts styling survives into a full app frame.
- [-] **Context Usage Display** `P0` — token usage in the statusline, fed from the daemon's `message_complete` · `crucible-cli`
  - **Gets you:** the statusline shows `2k tok` / `3% ctx`. Whether the number is the *daemon's* number is not proven, and the source field is `total_tokens` — not prompt+completion as this entry used to claim.
  - **Proof:** _none end to end — no single test carries a daemon `message_complete` payload through `SessionEventStream::translate` → `app.on_message` → a rendered frame, and the `total` half is explicitly unasserted (`total: _`), so the percentage denominator is proven by nothing. The render itself is snapshot-tested. Two modules format the same string independently, so there are two places to drift._
- [-] **Lua UI Config Bridge** `P0` — `ui.config` RPC delivers colorscheme, highlight groups, geometry and statusline bars to every attached client, with diffed `ui_style_changed` pushes and `ui.set_theme` · [[Help/Lua/Configuration]] · [[Help/Extending/Scripted UI]] · `crucible-lua`, `crucible-daemon`, `crucible-cli`
  - **Gets you:** bar layout set from Lua reaches the frame. Colorscheme, highlight groups, geometry, hot reload and theme switching have no test proving any of them change anything on screen.
  - **Proof:** _none for four of five payload parts — the colorscheme test asserts a store read, the highlight-group test asserts a getter's return value, the geometry test asserts a function's return value, and the RPC test asserts wire shape. Nothing in the repo installs a Lua theme and then renders. `ui_style_changed` hot-reload has no test of any kind; `ui.set_theme` has no success-path test (only rejection and traversal). Much of the geometry payload is read by nothing at all: `UiGeometry.layout`, `.modal`, `.drawer`, `.toast`, `.statusline`, `popup.border` and `popup.padding` are parsed, wired, stored, round-trip-tested and never read. This is the canonical shape of this repo's repeat bug._
- [-] **Statusline Item Trees** `P0` — bars are lists of named items (`sl.mode`, `sl.model{}`, `sl.expr("git")`) with combinators (`sl.any`, `sl.when`) and multiple bars in ordered regions · `crucible-cli`
  - **Gets you:** named items, `sl.any`, `sl.when` and multiple bars all genuinely render. `cru.statusline.set` values are not proven to reach the frame, and **"closed anchors" no longer exist** — `Anchor` + `order` were replaced by ordered region lists.
  - **Proof:** _none for the push path — `cru.statusline.set` is round-trip-only (registry snapshot on the Lua side, dirty check on the client side); the full `cru.statusline.set` → `broadcast_exprs_changed` → `ui_style_changed` → `apply_ui_config` → frame chain has no test past the two stores. `sl.expr` renders, but only from a hand-built map — the production wiring through `theme::exprs::snapshot()` is exercised by nothing. Four of six sub-features do render (`components/status_items.rs:390`, `:414`, `:437`; `tests/region_placement_tests.rs:33`, `:49`)._
- [x] **Terminal Palette Colours** `P1` — `term4` / bare index / `bright_*` address the user's own terminal colours · `crucible-oil`, `crucible-cli`
  - **Gets you:** every spelling reaches the slot it names. Set `blue` in your colorscheme and you get the slot 4 your terminal is configured with; `bright_blue` gets slot 12.
  - **Proof:** `crates/crucible-oil/src/style.rs`::named_colors_render_to_the_palette_slot_they_name (asserts the SGR crossterm emits for every named colour against `palette_index`), `crates/crucible-lua/src/theme.rs`::term_slots_are_aliases_for_the_matching_indices (now compares painted output, not `palette_index`). `to_crossterm` mapped name-to-name, but crossterm's `Color::Red` is slot 9 and its `DarkRed` is slot 1 — so all six chromatic pairs rendered inverted and `White` rendered as 15. The statusline snapshots showed it plainly (`mode_normal: Green` painting as 10); 21 snapshots were corrected. The test that should have caught it read `contains("42") || contains("48;5;10")` — green *or* bright green — which cannot fail.

### Tool & Agent Display

- [-] **Tool Call Display** `P0` — per-tool rows with smart summarization and MCP prefix stripping · `crucible-cli`
  - **Gets you:** tool rows render with the `mcp_` prefix stripped and a collapsed result. **There is no spinner on the tool row** — the icon is a static `●`, and that is deliberately pinned by a test asserting the icon does not animate. The animation lives in the turn indicator.
  - **Proof:** _none for the spinner (the code says `let _ = spinner_frame; // unused — animation is in turn indicator` and `::pending_icon_is_static_across_frames` asserts it) and none for summarization reaching a frame — `::summarize_tool_result_glob` asserts `Some("3 files")` as a pure string, and nothing renders it. Prefix stripping and result collapse do render (`components/tool_render.rs::strips_mcp_prefix_from_name`, `::render_tool_call_collapses_short_result`)._
- [x] **Tool Source Badges** `P0` — rows show `[mcp:gmail]` / `[plugin:oci]` · `crucible-cli`
  - **Gets you:** you can see where a tool came from, so an unexpected tool is traceable to its server or plugin.
  - **Proof:** `crates/crucible-cli/src/tui/oil/components/tool_render.rs`::source_badge_visibility (`render_to_plain_text`, parameterized over both badge forms)
- [x] **Turn Indicator** `P0` — an animated spinner at the turn level · `crucible-cli`
  - **Gets you:** the one animation in the chat view, showing the agent is working. Giving it its own entry is what lets the tool row stop claiming an animation it does not have.
  - **Proof:** `crates/crucible-cli/src/tui/oil/components/turn_indicator.rs`:30, visible in the committed snapshot `…container_snapshot_tests__snapshot_tool_pending.snap` (` ◐` alongside the static ` ● Bash ls`)
- [-] **Tool Output Handling** `P0` — truncated tail display, buffer cap, parallel call tracking by `call_id` · `crucible-cli`
  - **Gets you:** a truncated tail with an `(N more lines)` footer. Correcting the numbers: the **display** shows 3 lines (`MAX_TAIL`), the **buffer** caps at 50 (`TOOL_OUTPUT_MAX_TAIL_LINES`, untested), and spill-to-file at >10 KB is a *daemon-side* behaviour — the TUI only suppresses the spill marker.
  - **Proof:** _none for the buffer cap or for `call_id` routing reaching a frame — routing is state-only, and the one Vt100 test driving two concurrent `call_id`s asserts only that there are no triple blank lines; nothing would catch two tools' outputs being swapped. The displayed tail itself is proven (`::format_output_tail_truncates_long_output`)._
- [x] **Subagent Display** `P0` — spawned / completed / failed tracking with a truncated prompt preview · `crucible-cli`
  - **Gets you:** each subagent renders as its own row with a status glyph and a truncated prompt, including concurrent ones as separate rows, and delegation shows the target agent.
  - **Proof:** `crates/crucible-cli/src/tui/oil/components/subagent_render.rs`::render_subagent_running, `::render_subagent_completed`, `::render_subagent_failed`, `::render_subagent_truncates_long_prompt`; real frames in `tests/user_story_tests/subagent_mcp_tests.rs::subagent_spawn_shows_prompt_preview`, `::concurrent_subagents_render_as_separate_rows`, `::delegation_shows_target_agent`. Elapsed time is the one sub-claim with no assertion.
- [-] **MCP Server Display** `P0` — `:mcp` lists servers with live connection status · `crucible-cli`
  - **Gets you:** `:mcp` renders servers with filled/hollow status dots on a real screen. The runtime-update half is untested *and goes through different code* than the tested one.
  - **Proof:** _none for `ChatAppMsg::McpStatusLoaded` — the story test bypasses the message and calls `set_mcp_servers` directly, and the two paths diverge: `McpStatusLoaded` assigns the field raw while `McpServersReady` calls `set_mcp_servers`. Whatever the setter does beyond assignment does not happen on the message path. The listing itself is proven (`::mcp_command_lists_servers_with_connection_status` asserts `●` and `○` on the frame)._

### Interaction Modals

- [-] **Permission Modal** `P0` — Allow (y), Deny (n), Allowlist (a); diff toggle (**`h`**, not `d`); queued permissions auto-open · `crucible-cli`
  - **Gets you:** the modal opens on a real screen showing the command and the y/n/a options, deny surfaces "Permission denied", and queued permissions open in arrival order. **The diff toggle key is `h`** — `d` does nothing. (The same `d` error also appeared in the Keybindings entry and is fixed there.)
  - **Proof:** _none for the toggle's effect — `tests/permission_invariant_tests.rs::test_h_key_does_not_close_modal` asserts only that the modal survives the keypress, and its message miscalls it a "help toggle". Two `render_to_string` calls would close it. Everything else here is the strongest-evidenced modal (`user_story_tests/permission_tests.rs`, real frames, plus a committed snapshot whose footer reads `y/n/a options … h diff … Esc cancel`)._
- [-] **Ask Modal** `P0` — single-select, multi-select (Space), free-text "other" · `crucible-cli`
  - **Gets you:** single-select selection state works. Nothing proves any of it renders, and two of the three named features have no test at all.
  - **Proof:** _none — multi-select and the free-text "other" path have no test of any kind, rendered or state. The only rendered evidence the modal paints is a smoke test whose assertion is disjunctive (`screen.contains("option") || screen.contains("Question")`) and passes on the user message typed earlier in the test._
- [-] **Diff Preview** `P0` — syntax-highlighted, collapsible, unified and side-by-side **line** diffs · `crucible-cli`
  - **Gets you:** all of that, well-evidenced. **Word-level diffing does not exist** — that adjective is the entire demotion.
  - **Proof:** _none for word-level — `diff_view.rs` uses `TextDiff::from_lines` throughout and has no intra-line highlighting anywhere. Everything else is strong: `components/diff_view.rs::syntax_highlighted_diff_for_rust_extension`, `::collapsed_renders_only_header`, `::unified_renders_added_and_removed_lines`, `::side_by_side_pairs_lines_in_two_columns`, ten committed snapshots, plus frame-tested gating in `tests/container_snapshot_tests.rs::show_diffs_on_includes_diff_body`._
- [-] **Permission Session Settings** `P0` — `:set perm.show_diff`, `:set perm.autoconfirm_session` · `crucible-cli`
  - **Gets you:** both wires are complete in code and neither has a single test, at any level.
  - **Proof:** _none — no test sets either knob and then opens a modal; both permission snapshots hardcode `show_diff = true` at the constructor, and for `autoconfirm_session` only CLI-arg parsing is tested. `perm.autoconfirm_session` silently approving every permission is the highest-consequence untested path in the TUI: a regression there fails **open**._
- [-] **Batch Ask / Edit / Show / Panel** `P0` — all 7 `InteractionRequest` variants have renderers and key handlers · `crucible-cli`
  - **Gets you:** 7/7 renderers and 7/7 key handlers. The old claim "fully implemented with key handlers, renderers, **and tests**" holds for 1 of 7.
  - **Proof:** _none for six variants' rendered output — `AskBatch` has zero tests of any kind; `Edit` has 8 state tests and nothing that renders the node; `Show`'s scrolling is never observed in a frame; `Popup` is state only; `Panel`'s filtering and multi-select are never asserted on a frame; `Ask` has the weak disjunctive smoke test above. Only `Permission` has rendered-output coverage. The modals composite into the footer slot rather than an overlay, so six near-identical `render_to_string` tests need no new harness — the cheapest large coverage win in the TUI._

### Autocomplete & Popups

- [x] **Autocomplete** `P0` — 9 trigger kinds: `@files`, `[[notes]]`, `/commands`, `:repl`, `:model`, `:set`, command args, F1 palette, **`:pick`** · `crucible-cli`
  - **Gets you:** typing a trigger opens a completion popup painted at the right column without covering the line you are typing, and accepting inserts the right thing (`@file`, a closed `[[wikilink]]`, a full `:model` command). The count of 9 was always right; the enumeration used to list only 8 and omit `Pick`.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/popup_tests.rs`::completion_style_behavior::file_popup_is_minimal_and_anchored_by_default (real composited frame, asserts the label lands at display column 10) and `::popup_clears_the_prompt_region::*`; per-trigger candidate matrix in `chat_app/autocomplete.rs::tests` (`slash_trigger_lists_registered_commands`, `double_bracket_trigger_lists_notes`, `model_trigger_lists_available_models`, `set_trigger_lists_option_shortcuts`, `pick_trigger_lists_from_source`, `accept_note_wraps_in_wikilink`, …). Only `@` and `/` have composited-frame assertions; the rest share the same kind-agnostic renderer.
- [-] **`:set` Value Completion** `P0` — completing the *value* half of `:set option=value` · `crucible-cli`
  - **Gets you:** option *names* complete. Option *values* never do — typing `:set thinking=` shows nothing, and `:set thinking=hi` goes empty rather than falling back.
  - **Proof:** _none — `AutocompleteKind::SetOption { option: Some(_) }` is never constructed; the only construction site hardcodes `option: None`, leaving ~55 lines of `CompletionSource::{Models, ThinkingPresets, Themes, Static}` unreachable. The classic dead-arm shape. Relatedly `PickSource::Sessions` returns `vec![]` unconditionally, so `:pick sessions` opens an empty popup._
- [-] **Command Palette** `P0` — F1 toggle · `crucible-cli`
  - **Gets you:** a popup with **four hardcoded entries** — `semantic_search`, `create_note`, `/mode`, `/help` — two of which do nothing when selected. "Full command discovery" is false: it ignores the slash-command registry, every REPL command, and every plugin command.
  - **Proof:** _none — `autocomplete.rs:181-189` returns a literal 4-item list, and selecting the two tool entries only writes `self.status = format!("Tool: {}", label)`. No headless test covers the palette at all; the only F1 coverage is an `#[ignore]`d PTY test that never selects an item. The `/mode` and `/help` entries do dispatch._
- [-] **Model Lazy-Fetch** `P0` — model list state machine (NotLoaded → Loading → Loaded) · `crucible-cli`
  - **Gets you:** the state machine and "Loading models…" / "Failed to load models" placeholders exist, but nothing asserts either reaches a frame — and the fetch **is no longer lazy**: models are prefetched at startup, with the lazy path surviving only as a re-fetch after a failure.
  - **Proof:** _none for the render — no test renders the `:model` popup, and the placeholder rows are constructed and never frame-asserted, which is exactly the shape that has shipped broken here before. State/task transitions are covered._
- [x] **`:pick` Fuzzy Picker** `P0` — `:pick [notes|files|commands]` opens a fuzzy picker · `crucible-cli`
  - **Gets you:** a picker over notes, workspace files, or the command registry; accepting inserts `@file` / `[[note]]` / the command. It is advertised in the `:repl` completion list and in `:help`, so a user can find it today. `:pick sessions` is a dead branch that returns nothing.
  - **Proof:** `crates/crucible-cli/src/tui/oil/chat_app/command_handling.rs`:285-291 → `open_picker`; sources at `autocomplete.rs:451-518`; accept behaviour at `:549-565`; `autocomplete.rs::tests::pick_trigger_lists_from_source`
- [x] **`:set completion_style`** `P0` — `auto` | `minimal` | `panel` · `crucible-cli`
  - **Gets you:** switches the completion popup between the nvim-pmenu-style anchored box (default for inline `@`/`[[`) and the classic full-width strip.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/popup_tests.rs`::completion_style_behavior::completion_style_panel_forces_strip_for_inline — sets the knob through the real `:set` path and asserts the label moves from column 10 to column 3 on the composited frame. One of the few TUI knobs with a genuine frame-level assertion.

### Shell

- [x] **Shell Modal** `P0` — `!command` full-screen execution; scrollable (j/k/u/d/g/G/PgUp/PgDn) · [[Help/TUI/Shell Execution]] · `crucible-cli`
  - **Gets you:** `!cmd` takes over the screen, streams stdout, shows the exit code, and scrolls. `e` (open in `$EDITOR`) and `t` (insert truncated) also exist.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/shell_tests.rs`::successful_command_shows_output_and_exit_zero and `::failing_command_shows_nonzero_exit_code` — spawn a real child, pump to completion, assert the rendered modal contains the stdout and `exit 0` / `exit 3`; scroll state in `components/shell_modal.rs::scroll_operations`, `::manual_scroll_disables_auto_follow`
- [x] **Shell Output Insert (`i`)** `P0` — `i` inserts the command's output into the composer · `crucible-cli`
  - **Gets you:** pressing `i` closes the modal and puts the command's output in the composer, fenced and labelled; `t` does the same with the last 20 lines. `q` closes without inserting.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/shell_tests.rs`::insert_key_inserts_output_in_one_step, `::quit_key_closes_completed_modal_without_inserting`; composer side `chat_app/tests.rs`::inserting_shell_output_fills_the_composer_and_the_transcript. `i` used to set `pending_insert` and return `Close`, expecting a later `Tick` to emit the output — but the app drops the modal on `Close`, so no `Tick` ever arrived. `Close` now carries the insert; the `#[ignore]`d test that documented the bug is live.
- [-] **Shell History** `P0` — last 100 commands recalled with `!` prefix · `crucible-cli`
  - **Gets you:** commands are stored and capped at 100, and used for a dedupe check. There is no `!`-scoped recall. Partial mitigation by accident: `!cmd` submits through the ordinary input buffer, so Up-arrow *will* recall it — from the generic, uncapped, intermixed input history, not the 100-entry shell store.
  - **Proof:** _none for recall — `shell_history` has exactly one reader (the dedupe check), and `shell_history_index`, documented as "current index into shell_history during recall", is only ever assigned `None`. No navigation path, no key handler, no `!`-prefixed completion source, and `AutocompleteKind` has no shell variant. A dead-field violation of the repo's "no type without a use site" rule._

### Notifications

- [x] **Toast Notifications** `P0` — auto-dismiss after 3 s; INFO/WARN badge in the status bar · `crucible-cli`
  - **Gets you:** the newest toast appears in the status bar with count badges beside it, and stops showing after 3 s. **ERROR is unreachable at runtime** — `NotificationKind` has only `Toast | Progress | Warning` and both mapping sites produce Info or Warning, so that level has been dropped from this entry.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/notification_tests.rs`::latest_toast_shows_in_status_bar (StoryRuntime → real vt100 frame); badge ANSI in the committed `…statusline_ctrlc_notification_*` snapshots. Auto-dismiss is real but its only test is `#[ignore]`d as a 3 s wall-clock test.
- [x] **Messages Drawer** `P0` — `:messages` toggles the full notification history panel · `crucible-cli`
  - **Gets you:** a bordered panel listing the whole notification history with timestamps; any key closes it.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/notification_tests.rs`::messages_drawer_lists_all_notifications_in_order (asserts all three messages on the rendered frame, in order) and `::keypress_dismisses_open_drawer`
- [x] **Warning Badges** `P0` — persistent count badge when warnings exist · `crucible-cli`
  - **Gets you:** after a toast fades, a persistent ` WARN 2 ` count badge stays in the status bar, surviving narrow widths.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/component_isolation_tests.rs`::snapshot_statusline_count_badges_narrow_width_40 (rendered ANSI bar; the committed snapshot contains ` WARN ` and the count); `components/notification_area.rs::warning_counts_returns_nonzero_only`

### Rendering Engine

- [x] **Oil Renderer** `P0` — custom terminal rendering engine (replaced ratatui) · [[Help/TUI/Component Architecture]] · `crucible-oil`
  - **Gets you:** the whole TUI is painted by Crucible's own renderer; ratatui is gone.
  - **Proof:** `ratatui` appears in no `Cargo.toml` in the workspace, and every rendered-frame test in this document goes through `crucible_oil::render::{render_to_string, render_to_plain_text}` or `Vt100TestRuntime`
- [x] **Taffy Layout** `P0` — flexbox-based terminal layout; one spacing system via `gap()` for both graduated and viewport content · `crucible-oil`
  - **Gets you:** consistent spacing between scrollback and viewport, from one mechanism rather than two.
  - **Proof:** `taffy = "0.12"` at `crates/crucible-oil/Cargo.toml`:19; `Node::gap` at `crucible-oil/src/node.rs:484`, exercised by `tests/spacing_tests.rs`, `layout_tests.rs`, `graduation_tests.rs` and the `vt100_runtime_tests/spacing.rs` frame tests
- [x] **Theme System** `P0` — token-based theming · [[Meta/TUI-Style-Guide]] · `crucible-oil`
  - **Gets you:** theme tokens (mode colour, toast severity colours, syntax theme) come out as real ANSI in the painted frame.
  - **Proof:** the ANSI status-bar snapshots resolve through `theme::active()` and carry the token-derived SGR sequences (`…status_bar_normal.snap`, `…status_bar_plan.snap`, `…statusline_count_badges_narrow_width_40.snap`); diff/syntax colours in `…diff_view__snapshot_tests__snap_python_syntax_highlighting.snap`
- [-] **Theme Overrides** `P0` — user-supplied colours replace the defaults on screen · `crucible-cli`, `crucible-oil`
  - **Gets you:** overrides are proven to reach *getters*, not the frame.
  - **Proof:** _none — no test in `crucible-cli` or `crucible-oil` installs a non-default theme and asserts the rendered ANSI changes; every `.snap` renders the compiled-in default. The one near-miss asserts a `Color` value, not a frame. This is the precise failure mode the project already paid for in v0.16.0, and it overlaps the **Lua UI Config Bridge** demotion — that entry covers delivery, this one covers effect._
- [-] **Viewport Caching** `P0` — cached messages, tool calls, shell executions, subagents · `crucible-cli`
  - **Gets you:** messages, tool calls, subagents **and shell executions** are cached and render — run `!cargo build`, close the modal, and the command, its exit code and its output tail are in the transcript. There is still no lazy line-wrapping.
  - **Proof:** `crates/crucible-cli/src/tui/oil/tests/user_story_tests/shell_tests.rs`::a_closed_shell_command_appears_in_the_frame (real render path) and `chat_app/tests.rs`::a_closed_shell_command_is_recorded_in_the_transcript. `ContainerList::add_shell_execution` had zero call sites because `update_shell_modal` took the history item and did `let _ = &history_item;` behind a TODO. _Still `[-]` for "lazy line-wrapping": no memoised wrap exists anywhere in `viewport_cache.rs`._
- [x] **Drawer Component** `P0` — bordered expandable panels with title/footer badges · `crucible-cli`
  - **Gets you:** a bordered panel with a title badge and an `ESC/q close` footer, capped at `max_items`.
  - **Proof:** `crates/crucible-cli/src/tui/oil/components/drawer.rs`::drawer_renders_items, `::drawer_has_borders`, `::drawer_has_footer_badge`, `::drawer_limits_items`; live use confirmed on a whole-app frame by `notification_tests.rs::messages_drawer_lists_all_notifications_in_order`

### Session & Export

- [x] **Session Export** `P0` — `:export <path>` saves the session as markdown · `crucible-cli`, `crucible-daemon` (observe)
  - **Gets you:** a markdown file with YAML frontmatter, collapsible thinking callouts, and tool call/result blocks, with tilde expansion on the path. A missing parent dir and no-active-session both warn rather than failing silently, and export is skipped in replay mode.
  - **Proof:** `crates/crucible-daemon/src/observe/markdown.rs`::tests asserts the exact rendered markdown for every claimed piece (frontmatter, `> [!thinking]- Thinking`, `test_render_tool_call`, `test_render_tool_result`, `test_render_tool_error`, `test_render_system_prompt`), and the write is `tokio::fs::write(&export_path, &md)` of that exact string. The one untested link is the `:export` → file-on-disk hop itself.
- [x] **Keybindings** `P0` — Enter, Esc, Ctrl+C, Ctrl+T, BackTab, F1, y/n/a and **`h`** (diff) in modals, plus a readline set · [[Help/TUI/Keybindings]] · `crucible-cli`
  - **Gets you:** every key named here dispatches, including Ctrl+A/E/W/U/B/F, Alt+B/F and Ctrl+J for a newline. Note the diff toggle is `h`, not `d`. Ctrl+Enter (cancel while preserving the draft) is bound and missing from both this list and `:help keys`.
  - **Proof:** live handlers for each key with `tests/event_loop_tests.rs::double_ctrl_c_triggers_quit_action`, `::ctrl_c_clears_input_first`, `tests/permission_invariant_tests.rs::invariant_escape_always_denies`, `::invariant_ctrl_c_closes_and_denies_permission_modal`, and the readline set in `tests/event_tests.rs`. **The linked doc `docs/Help/TUI/Keybindings.md` is materially wrong** and needs its own fix: it calls Ctrl+T "transpose characters", advertises Alt+T and Alt+M bindings that exist nowhere, claims Ctrl+K/Home/End/PageUp behaviours `InputAction::from` does not implement, and documents none of Enter, Esc, F1 or y/n/a. Two keys are untested: Ctrl+T and F1.
- [x] **Bottom-Anchored Chat Layout** `P1` — composer and status bar pinned to the bottom; conversation fills the space above · [[Meta/TUI User Stories]] · `crucible-cli`
  - **Gets you:** the input box and status bar are the last rows of every frame, popups render above the input bar, and content graduates upward into scrollback.
  - **Proof:** `crates/crucible-cli/src/tui/oil/chat_app/mod.rs`:177-224 (`flex(1)` on the content slot absorbs the remaining height); committed frame sequence `…undo_tests__undo_flow_frame_sequence.snap` shows the composer box and ` NORMAL ` bar as the last rows of every frame; `tests/popup_tests.rs::popup_positioned_above_input_bar`
- [x] **Input History** `P1` — Up/Down (and Ctrl+P/Ctrl+N) walk back through messages submitted this session · `crucible-cli`
  - **Gets you:** recall of anything you submitted this session, with your in-progress draft restored when you walk past the newest entry. **In-session only — no persistence across restarts**, and it is the generic input buffer, not the shell-history store.
  - **Proof:** `crates/crucible-cli/src/tui/oil/event.rs`:187-217 (`HistoryPrev`/`HistoryNext` with draft save/restore and index clamping), pushed on every submit at `:178-185`, routed in the live app at `chat_app/input_handling.rs:60`, asserted over the real buffer at `tests/event_tests.rs:107-132`, `:152-169`, `:206-224`
- [ ] **Splash Screen** `P1` — a startup splash for the TUI · [[Meta/TUI User Stories]] · `crucible-cli` — grep for `splash` across `crates/crucible-cli/src` returns zero hits; the 2026-07-22 splash work was the *web* splash. There is also no splash story in the TUI User Stories doc, which the project's own rule makes a prerequisite.
- [ ] **Session Stats** `P1` — per-session token/turn statistics surface · `crucible-cli` — no `:stats` REPL command and no session-stats surface anywhere in the TUI. `cru stats` / `cru storage stats` report storage and index counts, not per-session numbers; the statusline's context usage is the adjacent shipped thing.

## Extensibility & Plugins

- [x] **Lua Scripting** `P0` — Lua 5.4 runtime for plugins · [[Help/Lua/Language Basics]] · [[Help/Concepts/Scripting Languages]] · `crucible-lua`
  - **Gets you:** a plugin author's Lua evaluates in the daemon and returns values.
  - **Proof:** hands-on against the live daemon during the 2026-07-30 sweep — `cru lua '=type(cru.service)'` → `table`, `cru lua '=1+1'` → `2`; also `crates/crucible-daemon/src/daemon_plugins/tests.rs`::eval_table_as_json. Lua 5.4 vendored at `crates/crucible-lua/Cargo.toml`:17.
- [x] **Fennel Support** `P0` — Lisp-to-Lua compiler with macros; Fennel tools and `_test.fnl` suites compile and run · [[Help/Concepts/Scripting Languages]] · [[Meta/Analysis/Fennel for Plugins]] · `crucible-lua`
  - **Gets you:** a `.fnl` tool compiles and executes with the right result, and a Fennel test suite runs under `cru plugin test`.
  - **Proof:** `crates/crucible-lua/tests/integration/fennel.rs`::test_fennel_tool_execution; `crates/crucible-daemon/src/server/lua_plugin_suite.rs`::a_fennel_test_file_compiles_and_runs (a `.fnl` suite compiles and reports `1 passed` through the RPC body)
- [-] **Fennel Plugins in the Daemon** `P1` — an `init.fnl` plugin loads and runs like a Lua one · `crucible-lua`, `crucible-daemon`
  - **Gets you:** nothing. A Fennel plugin is discovered, its metadata is displayed, and then it fails to execute — it looks installed and does nothing.
  - **Proof:** _none — `load_plugin_spec` compiles Fennel (`lifecycle/spec.rs:91-104`), but `DaemonPluginLoader::execute_plugin` does `read_to_string(init_path)` → `lua.load(&source)` with no Fennel branch (`daemon_plugins/mod.rs:718-725`), and the loader downgrades the resulting parse error to `warn!` + `mark_error`. No shipped plugin is Fennel, so `every_shipped_plugin_executes` cannot catch it. Same silently-inert shape as the pre-2026-07-25 `crucible.on` bug._
- [x] **Plugin System** `P0` — discovery, lifecycle, manifests · [[Help/Extending/Creating Plugins]] · [[Help/Extending/Plugin Manifest]] · `crucible-lua`, `crucible-daemon`
  - **Gets you:** every shipped plugin is discovered from its `plugin.yaml`, executes in the daemon VM, and reports `state: Active` with no `last_error` in `plugin.list`. Manifest-less (`init.lua`-only) directories are discovered too.
  - **Proof:** `crates/crucible-daemon/src/daemon_plugins/tests.rs`::every_shipped_plugin_executes and `::every_shipped_plugin_is_discovered`; `crates/crucible-lua/tests/plugin_integration.rs`::test_manifestless_plugin_discover_and_load. One trap for authors: spec-table `handlers = {...}` declarations are parsed for display but **never dispatched** (the loader emits a `warn!` saying so) — only `crucible.on(...)` at load registers a working handler.
- [x] **Tool Annotations** `P0` — `@tool`, `@handler` (`@hook` is a deprecated alias), `@param` annotations for Lua functions · [[Help/Extending/Custom Tools]] · `crucible-lua`
  - **Gets you:** an annotated Lua function is discovered, gets a JSON schema, and executes with the right result. Note the annotation path reaches an agent **only through the MCP surface** (`cru mcp` / `McpServerManager`); the internal agent's dispatch uses the *spec-table* path and ignores annotations entirely. Two parallel tool systems.
  - **Proof:** `crates/crucible-lua/tests/integration/tools.rs`::test_lua_tool_discovery_and_execution (discovers 1 tool, executes it, asserts `{"result": 15}`), `::test_full_pipeline_lua`
- [-] **Event Hooks (note lifecycle)** `P0` — `note:created`, `note:modified` and friends firing into Lua · [[Help/Extending/Event Hooks]] · `crucible-lua`
  - **Gets you:** nothing. No note event is ever dispatched to Lua.
  - **Proof:** _none — `runtime_handlers_for`, the only dispatch entry point for `crucible.on` handlers, is called for `pre_tool_call`, `tool_result`, `pre_llm_call`, `transform_context`, `post_llm_call`, `turn:complete`, `precognition_format`/`precognition_select` and a caller-supplied name on the `lua.execute_hook` RPC. No note event is in that set, and no site in the note pipeline or file watcher dispatches to Lua. The linked doc is more accurate than this entry was — it correctly omits `note:created`. The generic hook system does work; see **Custom Handlers**._
- [x] **Lua File-Watch Hooks** `P0` — `crucible.on("FileChanged", …)` fires when the workspace changes · `crucible-lua`, `crucible-daemon`
  - **Gets you:** the trigger a daemon-computed statusline value like git status actually needs, since files change while you are not in a turn. This is the one event class that does bridge from the daemon broadcast bus into the Lua registry.
  - **Proof:** dispatch at `crates/crucible-daemon/src/server/file_event_hooks.rs` (`to_internal_event` returns `Some` for `file_changed` and `file_deleted`); consumer is the shipped statusline default at `runtime/statusline/default.lua`
- [x] **Custom Handlers** `P0` — event handler chains with interception and transformation · [[Help/Extending/Custom Handlers]] · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a handler a *plugin* registered runs on a real tool call and its return value becomes the tool result the model and the UI see; a handler can rewrite arguments before dispatch, patch a handled result, and an error in a gate handler blocks execution (fail-closed).
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/reactor.rs`::plugin_registered_pre_tool_call_handler_intercepts_tool (drives a real `DaemonPluginLoader`, asserts the emitted `tool_result` body), `::plugin_pre_tool_call_transform_rewrites_arguments` (asserts dispatch executed the rewritten path by only creating the rewritten file on disk), `::plugin_tool_result_handler_patches_a_handled_result`, `::runtime_dispatch_pre_tool_call_handler_error_blocks_execution`
- [-] **Handler Priority Ordering** `P0` — `crucible.on(..., { priority = N })` decides which handler wins · `crucible-lua`
  - **Gets you:** ordering asserted on a struct list, never on an outcome.
  - **Proof:** _none — `runtime_handlers_for_returns_sorted_by_priority` pushes three `RuntimeHandler` structs and asserts the returned `Vec` order with no handler bodies running, and `multiple_handlers_run_in_priority_order` is misnamed: both handlers use the default priority, so it asserts registration order. No test has two `crucible.on` handlers at different priorities compete. The *permission* hook chain — a different registry — does prove priority end to end._
- [x] **Execution Backends as Plugins** `P1` — workspace tools intercepted via `pre_tool_call` and routed to alternate backends; the agent core stays backend-agnostic · [[Help/Extending/Container Isolation]] · `runtime/plugins/oci/` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** `{ handled = true, result = … }` from a plugin bypasses the default executor and its value becomes the tool result. The reference `oci` plugin runs workspace tools inside OCI containers via `podman`/`docker`/`nerdctl exec`; the runtime is auto-detected, isolation that cannot be established **fails closed** rather than falling back to the host, and any tool no handler took over is denied by name. The guarantee holds for **internal agents only** — the dispatch layer refuses to pair an isolation claim with an ACP agent, which executes tools in its own process. Sandbox isolation is a plugin, not a core concern; the daemon never grows a per-backend abstraction.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/reactor.rs`::plugin_registered_pre_tool_call_handler_intercepts_tool, `::an_isolated_session_refuses_a_tool_no_handler_took_over`; `crates/crucible-daemon/tests/oci_plugin.rs`::oci_fails_closed_when_the_container_runtime_is_missing, `::oci_runs_bash_inside_the_container` (`#[ignore]`d, needs podman/docker); `daemon_plugins/tests.rs::only_required_start_hooks_can_refuse_a_session`; plus `runtime/plugins/oci/tests/*.lua` in CI. (`plugin.yaml` says version 0.2.0, not the 0.3.0 this entry used to claim.)
- [-] **Oil UI DSL** `P1` — Lua/Fennel API for interaction modals (ask, popup, panel) · [[Help/Extending/Scripted UI]] · [[Help/Plugins/Oil Lua API]] · `crucible-lua`, `crucible-oil`
  - **Gets you:** nothing a Lua script declares ever reaches a rendered frame.
  - **Proof:** _none — `cru.interaction.ask/popup/panel/permission` are pure table builders and nothing consumes their output: `lua_ask_to_core` / `lua_permission_to_core` have no non-test caller, `InteractionContext`/`InteractionRegistry` are constructed only in Lua's own tests, and `crucible-cli` imports only theme/statusline/hl/geometry types from `crucible_lua` — it never references `LuaNode`, `cru.oil`, `interaction`, or `DiscoveredView`. `docs/Help/Extending/Scripted UI.md` documents modals that never render. Three sources also disagree about whether these modules exist daemon-side; they do (registered unconditionally in `LuaExecutor::new`), which makes the "Not registered (UI-only)" comment in `daemon_plugins/mod.rs:114-116` false._
- [x] **Lua API Modules** `P0` — ~25 module tables plus 7 top-level helpers under the unified `cru.*` namespace (`crucible.*` retained as a long-form alias) · `crucible-lua`
  - **Gets you:** `cru.check`, `cru.config`, `cru.context`, `cru.emitter`, `cru.errors`, `cru.fs`, `cru.health`, `cru.http`, `cru.interaction`, `cru.json`, `cru.kiln`, `cru.oil`, `cru.oq`, `cru.paths`, `cru.ratelimit`, `cru.schedule`, `cru.service`, `cru.sessions`, `cru.shell`, `cru.statusline`, `cru.storage`, `cru.timer`, `cru.tools`, `cru.ws`, plus `fmt`, `get_session`, `inspect`, `log`, `retry`, `spawn`, `tbl_deep_extend`, `tbl_get`. **`cru.graph` does not exist** — `register_graph_module` installs a bare global `graph`, and the same is true of `oq` and `paths` (which happen to also be mirrored into `cru`).
  - **Proof:** hands-on `pairs(cru)` enumeration against the live daemon plugin VM during the sweep; `cru lua '=type(cru.graph)'` → nil while `=type(graph)' → table`
- [x] **Timer/Sleep Primitives** `P1` — `cru.timer.sleep(secs)`, `cru.timer.timeout(secs, fn)`; backed by `tokio::time` · `crucible-lua`
  - **Gets you:** sleep actually suspends and timeout actually expires, including from inside a plugin lifecycle hook (hooks are fired in an async context).
  - **Proof:** `crates/crucible-lua/src/timer.rs`::test_sleep_basic (asserts elapsed wall time), `::test_timeout_expires`, `::test_sleep_negative_errors`; `daemon_plugins/tests.rs::a_lifecycle_hook_can_call_async_apis`
- [x] **Rate Limiting** `P1` — `cru.ratelimit.new({ capacity, interval })` token bucket · `crucible-lua`
  - **Gets you:** the bucket actually blocks and refills; `:acquire()` waits, `:try_acquire()` does not, `:remaining()` reports.
  - **Proof:** `crates/crucible-lua/src/ratelimit.rs`::test_acquire_waits_for_refill, `::test_try_acquire_basic`, `::test_remaining`, `::test_invalid_params`
- [x] **Retry with Backoff** `P1` — `cru.retry(fn, opts)` exponential backoff with jitter · `crucible-lua`
  - **Gets you:** a failing function is retried the configured number of times and then raises; a non-retryable error stops immediately.
  - **Proof:** `crates/crucible-lua/src/lua_stdlib/tests/retry.rs`::test_retry_succeeds, `::test_retry_exhausted`, `::test_retry_non_retryable`, `::test_retry_with_real_timer`
- [x] **Event Emitter** `P1` — `cru.emitter.new()` minimal pub/sub · `crucible-lua`
  - **Gets you:** `:on`/`:emit`/`:off`/`:once` fire, stop firing, and fire exactly once, in registration order.
  - **Proof:** `crates/crucible-lua/src/lua_stdlib/tests/emitter.rs` — 13 tests including `::test_emitter_off`, `::test_emitter_once`, `::test_emitter_preserves_registration_order`, `::test_emitter_emit_async_fires_listeners`
- [x] **Argument Validation** `P1` — `cru.check.string()`, `.number()`, `.table()`, `.one_of()` with optional/range constraints · `crucible-lua`
  - **Gets you:** a bad argument raises a Lua error naming the parameter; a good one passes.
  - **Proof:** `crates/crucible-lua/src/lua_stdlib/tests/check.rs`::test_check_string, `::test_check_number_with_range`, `::test_check_one_of`, `::test_check_table` — each asserts both sides
- [-] **`cru.storage`** `P1` — per-plugin persistent key/value store · `crucible-lua`, `crucible-daemon`
  - **Gets you:** the module is on the plugin VM and is upgraded with a real `PropertyStore` at daemon boot, so a plugin can namespace state by plugin name.
  - **Proof:** _none — registration is proven (`crates/crucible-lua/src/storage_api.rs`, upgraded at `daemon_plugins/mod.rs:424-430`, visible in a live `pairs(cru)`), but nothing asserts a value written from a plugin survives a daemon restart, which is the whole point._
- [x] **Plugin Config** `P0` — per-plugin configuration schemas · [[Help/Lua/Configuration]] · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a plugin's `[plugins.<name>]` TOML section arrives in its `setup(cfg)`, and the user's `init.lua` `setup{…}` overrides it. Precedence: TOML is the base applied at load, user `init.lua` lands last and wins (the Neovim convention). A broken user `init.lua` fails open.
  - **Proof:** `crates/crucible-daemon/tests/plugin_config.rs`::setup_receives_the_plugins_toml_section, `::user_init_lua_setup_overrides_toml`, `::broken_user_init_lua_fails_open`, `::setup_values_beat_explicit_toml`
- [x] **Lua Config Beats TOML** `P0` — a user `init.lua` runs in the plugin runtime and its settings override `config.toml`; `cru.config` reads and writes app config from Lua · `crucible-lua`
  - **Gets you:** the Neovim precedence rather than a parallel store — the load-bearing fact for anyone writing an `init.lua`, and the prerequisite for `cru.defaults`.
  - **Proof:** `crates/crucible-lua/src/config.rs`::test_app_config_seed_then_lua_override, `::test_app_config_lua_get`, `::test_app_config_set_without_seed`, `::test_include`
- [x] **`cru.defaults` — Session Default Tier** `P0` — `cru.defaults.x` is the `:set`-global analogue; `session.x` is the buffer-local one · `crucible-lua`, `crucible-daemon`
  - **Gets you:** every new session inherits the global unless an agent card or an `on_session_start` hook overrides it, and the value is visible as session state rather than only applied at send time. Shipped defaults live in `runtime/defaults/init.lua`; a user `init.lua` can replace or append to them. A failing start hook does not break the session.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/init_lua_defaults.rs`::an_agent_with_no_prompt_of_its_own_gets_the_default, `::the_default_is_visible_as_session_state_not_just_at_send_time`, `::an_agent_card_prompt_wins_over_the_default`, `::a_user_init_lua_can_replace_a_shipped_default`, `::a_user_init_lua_can_append_to_a_shipped_default`, `::on_session_start_fires_and_can_set_this_sessions_values`, `::a_failing_start_hook_does_not_break_the_session`. Registered on the **session** VM, so `cru lua` (the plugin VM) cannot see it.
- [x] **`cru.modes` — Lua-Declared Modes** `P0` — `cru.modes.x = { tools = …, permissions = { default/allow/deny/ask } }` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a mode is data. The shipped `normal`/`plan`/`auto` are themselves Lua declarations and can be removed or redefined; a user-invented mode is selectable, filters the advertised tool set, and supplies a permission stance plus a rule list evaluated by the same engine — so `bash:rg *` inherits chained-command handling and a mode can permit specific *commands*, not just whole tools. An unknown or vanished mode **fails closed** rather than becoming the most permissive. Use a static stance for the simple case and a hook for the conditional one.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/init_lua_defaults.rs`::the_shipped_modes_are_declared_in_lua, `::a_user_defined_mode_can_be_selected`, `::a_shipped_mode_can_be_removed`, `::an_undeclared_mode_is_still_rejected`, `::a_mode_can_permit_bash_for_specific_commands_only`, `::session_modes_ignores_a_persisted_mode_that_no_longer_exists`; handle side `provider/genai_handle.rs::a_declared_modes_tool_selector_filters_the_advertised_set`, `::a_mode_whose_declaration_vanished_does_not_widen_the_tool_set`, `::switching_to_plan_mid_run_hides_plugin_tools`
- [x] **Plugin-Declared Commands** `P0` — a plugin's `spec.commands` become invocable commands, listed and reachable over RPC · `crucible-lua`, `crucible-daemon`, `crucible-cli`, `crucible-web`
  - **Gets you:** `/name args` in the TUI and the web palette invokes the plugin's command through the daemon registry instead of going to the agent as a chat message. Shadowing is deliberate: a plugin cannot shadow a built-in.
  - **Proof:** `crates/crucible-daemon/tests/plugin_tools_commands.rs`::plugin_declared_command_is_listed_and_invocable, `::plugin_commands_are_reachable_over_rpc`; `crates/crucible-daemon/src/server/plugins.rs`::commands_without_a_loader_is_an_empty_list_not_an_error, `::run_command_without_a_loader_reports_an_error`; TUI shadowing state-tested at `chat_app/command_handling_tests.rs:398`
- [x] **Plugin-Published Session Status** `P0` — `crucible.set_status{ session, key, plugin, text, level }` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a plugin can say something durable about a session that the TUI and web show — this is how `oci` reports "sandboxed: `<image>` (`<runtime>`)". It exists because a session's isolation state was otherwise unverifiable from the UI.
  - **Proof:** `crates/crucible-lua/src/plugin_status.rs` (`StatusRegistry`), registered at `crates/crucible-daemon/src/daemon_plugins/mod.rs`:172-183, consumed via `DaemonPluginLoader::status()`, used at `runtime/plugins/oci/init.lua:296,309,343`
- [-] **Plugin File Watcher** `P1` — `[plugins] watch = true` reloads a plugin when its file changes · `crucible-daemon`
  - **Gets you:** the config key now exists (it previously had none and the watcher was hardcoded off), and the loaded-plugin directory list is tracked.
  - **Proof:** _none — no test asserts that editing a plugin file on disk triggers a reload. `split_plugins_config` and `loaded_plugin_dirs` are the implementation; the effect is unwatched._
- [-] **Script Agent Queries** `P0` — `ask.agent(batch)` from Lua · [[Help/Extending/Script Agent Queries]] · `crucible-lua`
  - **Gets you:** nothing. The documented entry point does not exist.
  - **Proof:** _none — `ask.agent` is defined nowhere in the repo; `ask/register.rs:14-40` registers exactly `question`, `batch`, `notify`, `answer`, `answer_other` on a bare global `ask`. Live probe: `cru lua '=type(ask)'` → `table`, `=type(ask.agent)` → nil. The builder tables the module does produce feed the same dead `InteractionContext` seam as **Oil UI DSL**. The linked help page is a spec for something never built._
- [x] **HTTP Module** `P0` — HTTP client for plugins · [[Help/Extending/HTTP Module]] · `crucible-lua`
  - **Gets you:** a plugin issues a real HTTP request from the daemon VM and reads the status, with connection failures surfaced rather than swallowed.
  - **Proof:** hands-on during the sweep — `cru lua '=cru.http.get("http://127.0.0.1:3001/").status'` → `200` against the dev web server, and `…:3000/` → `0` for a refused connection. In-repo coverage is weaker (registration and request *building* only); the real consumer is `plugins/discord/lua/api.lua`. Worth a regression test with a local mock server — nothing in CI issues a request.
- [-] **Lua Integration (full)** `P1` — "complete scripting API for custom workflows and callout handlers" · `crucible-lua`
  - **Gets you:** undefined. The entry names no surface, no acceptance criterion and no consumer.
  - **Proof:** _undetermined — the claim is unfalsifiable as written: nothing can move it to `[x]` and nothing can prove it wrong. Two concrete sub-claims hide inside it, and either could become a real entry: **callout handlers** (callouts parse and the web renders them, but no Lua hook fires on one — there is no `callout` event in the dispatch set) and **custom workflows** (the daemon has a workflow system with no Lua entry point). Recommend replacing this entry with those two, or dropping it._
- [x] **Hook Documentation** `P1` — comprehensive guide on extending Crucible · [[Help/Extending/Event Hooks]]
  - **Gets you:** a plugin author gets the full hook contract — every dispatched event, its `ctx`/`event` fields, and its return-value table — plus companion guides for handlers, plugin creation, custom tools, container isolation, the HTTP module and the plugin manifest.
  - **Proof:** `docs/Help/Extending/Event Hooks.md` (344 lines: `crucible.on` signature, `opts.pattern`, `opts.priority`, per-event field tables, and the return-value semantics table), `Custom Handlers.md` (330), `Creating Plugins.md` (682), `Container Isolation.md`, `Custom Tools.md`, `HTTP Module.md`, `Plugin Manifest.md`. The doc is *more* accurate than this map was — it documents only events that fire. Two known doc defects to fix alongside: `Script Agent Queries.md` documents a nonexistent API and `Scripted UI.md` documents modals that never render.

### Plugin Developer Experience

> The Discord plugin proved the plugin system can express a real integration (a 1762-line multi-module plugin with WebSocket, REST, streaming and permissions) — though nothing in CI exercises it, so treat that as a design proof rather than a test. These items close the gap between "works" and "easy to write".
>
> **Guiding insight**: Neovim's plugin ecosystem exploded when LuaLS type stubs + lazy.nvim hot reload made Lua plugins as ergonomic as TypeScript. Crucible needs the same inflection point.

- [x] **LuaCATS Type Stubs** `P1` — `StubGenerator::generate` emits `cru.lua` (EmmyLua/LuaCATS) plus `cru-docs.json`, auto-running at daemon startup · `crucible-lua`, `crucible-daemon`
  - **Gets you:** `~/.config/crucible/luals/cru.lua` and `cru-docs.json` exist on disk without you asking.
  - **Proof:** hands-on — read `~/.config/crucible/luals/cru.lua` on this box during the sweep (416 annotation/function lines, 18 `---@class cru.*` blocks) and `cru-docs.json`; auto-run site `crates/crucible-daemon/src/server/plugin_boot.rs`:168-175; `crates/crucible-lua/tests/stubs_integration.rs`::generates_emmylua_stubs_with_core_and_ui_modules
- [x] **Type Stub Coverage** `P1` — the stubs describe the real `cru.*` surface · `crucible-lua`
  - **Gets you:** autocomplete for exactly the namespaces the plugin VM has — every one of them, and nothing else. A module registered tomorrow is stubbed without editing a list.
  - **Proof:** `crates/crucible-daemon/tests/plugin_stubs_contract.rs`::every_stubbed_namespace_exists_on_the_plugin_vm and `::every_plugin_vm_namespace_is_stubbed` — both directions, asserted against a real `DaemonPluginLoader` VM. The generator walked a hardcoded list inside a throwaway executor that *fabricated* six `cru.*` namespaces from bare globals; measured, 6 stubbed namespaces did not exist and 12 that did had no stubs. It now walks the plugin VM itself. `graph` and `mcp` registered a bare global only, unlike every sibling, so those two were fixed at the source and their stubs became true; `session`/`hooks`/`notify`/`ask` never existed under any name and stay unstubbed. The three stub directories now agree on `crucible_core::config::lua_stubs_dir`.
- [x] **Plugin Hot Reload** `P1` — `:reload <plugin>`; `plugin.reload` + `plugin.list` RPCs · `crucible-lua`, `crucible-daemon`, `crucible-cli`
  - **Gets you:** editing a plugin's Lua and reloading makes the new code take effect, its `on_unload` hook fires, its handlers are replaced rather than duplicated, and a failed reload leaves the old plugin intact.
  - **Proof:** `crates/crucible-lua/tests/integration/reload.rs`::test_reload_picks_up_changes (writes v1, loads, rewrites to v2, reloads, asserts v2), `::test_on_unload_hook_fires_on_reload`, `::test_reload_failure_leaves_old_plugin_intact`; daemon path `daemon_plugins/tests.rs::reloading_a_plugin_replaces_its_handlers`. No test drives `:reload` through the TUI.
- [x] **`:lua` REPL** `P1` — `lua.eval` RPC + `cru lua` CLI; `=expr` prints the result (the Neovim pattern) · `crucible-cli`, `crucible-daemon`
  - **Gets you:** the fastest way to falsify any Lua-API claim in this document — it is what caught the `cru.graph` and `ask.agent` gaps during the sweep.
  - **Proof:** hands-on, repeatedly — `cru lua '=type(cru.service)'` → `table`, `=type(cru.graph)` → nil, `='=cru.http.get(…).status'` → `200`; also `daemon_plugins/tests.rs::eval_expression_with_equals_prefix`, `::eval_table_as_json`, `::eval_syntax_error_returns_err`
- [x] **`cru plugin new`** `P1` — scaffold a plugin from a template · `crucible-cli`
  - **Gets you:** `plugin.yaml`, `init.lua`, `health.lua`, `.luarc.json` and `tests/init_test.lua`, and the path printed. **Its own printed next step is broken**: `cru plugin test .` run verbatim from the plugin dir prints `0 passed, 0 failed` and exits 0 (a silent false green — the relative path is resolved in the *daemon's* cwd), and with an absolute plugin-dir path the template's `require("init")` fails because the harness deliberately drops `<plugin_dir>/?.lua` from `package.path`. It only passes when pointed at the test *file*.
  - **Proof:** hands-on — ran `cru plugin new demoplug` in a scratch dir and all five files were created; templates validated by `crates/crucible-lua/tests/integration/plugin_template.rs`::test_plugin_template_yaml_is_valid, `::test_plugin_template_init_lua_is_syntactically_valid`, `::test_plugin_template_tool_annotation_format`
- [x] **Clean Error Messages** `P1` — `format_lua_error()` strips Rust FFI frames; errors carry plugin name, file path and line · `crucible-lua`
  - **Gets you:** a Lua error reaches you as plugin + file + line, not a Rust stack — including in RPC response bodies, where the line number is the assertion's line in your test file rather than a frame inside the runner.
  - **Proof:** `crates/crucible-lua/src/error.rs`::format_lua_error_strips_ffi_frames, `::format_lua_error_preserves_simple_errors`; reaching a response body in `server/lua_plugin_suite.rs::a_test_file_that_does_not_load_reports_which_file_and_why`, `::a_failing_test_returns_its_name_error_and_line`
- [x] **Plugin Test Harness** `P2` — `cru plugin test <path>` runs a busted-style suite against a mocked `cru.*` API · `crucible-lua`, `crucible-cli`
  - **Gets you:** `N passed, N failed` with per-failure suite / name / message / `file:line`, `describe`/`it`/`pending`/`assert.equal`/`assert.truthy`/`before_each`, mocked `cru.*` so tests need no daemon, and every shipped plugin's suite running in CI. Fennel suites too. Caveat: a **relative** `test_path` is resolved in the daemon's cwd, so `cru plugin test .` reports a false green.
  - **Proof:** hands-on — `cru plugin test /abs/path/tests/init_test.lua` → `3 passed, 0 failed`, and a deliberately broken path → the full diagnostic and exit 2. In-repo: CLI at `crates/crucible-cli/src/commands/plugin/test.rs`, runner at `crucible-lua/src/lua_stdlib/test_runner.rs`, mocks at `crucible-lua/lib/test_mocks.lua`, CI gate `server/lua_plugin_suite.rs::shipped_plugin_lua_suite_passes` guarded by `::every_shipped_plugin_has_a_test_case`
- [x] **`.luarc.json` Generation** `P2` — scaffolded `.luarc.json` points at the type stubs for zero-config IDE setup · `crucible-cli`
  - **Gets you:** `cru plugin new` emits a `.luarc.json` whose `workspace.library` contains the stub directory the daemon writes to, and `diagnostics.globals` covers both `cru` and the `crucible` alias.
  - **Proof:** `crates/crucible-cli/src/commands/plugin/new.rs`::the_scaffolded_luarc_json_points_at_the_generated_stub_directory (parses the emitted JSON) and `::the_scaffold_and_the_stubs_command_agree_on_the_directory`. The template was a static `include_str!` with no substitution, and `cru plugin stubs` printed instructions to paste the path in by hand — the command conceding the feature did not exist. _`cru plugin init`, named in the pre-reconciliation entry, is not a subcommand at all._
- [-] **`cru plugin` Lifecycle Subcommands** `P2` — `list`, `remove`, `update`, `health` · `crucible-cli`
  - **Gets you:** the subcommands exist and dispatch alongside the covered `new` / `test` / `stubs` / `add`.
  - **Proof:** _none — no test or hands-on run asserts any of these four produces output or changes state; the modules are present (`health.rs`, `list.rs`, `remove.rs`, `update.rs` under `crates/crucible-cli/src/commands/plugin/`) and that is the whole of the evidence._

### Plugin Abstractions

> Extracted from building the Discord plugin. These target the plugin types we expect to be most common: messaging bots, autonomous loops, content transformers, and long-running services.

- [x] **`cru.service`** `P1` — service lifecycle for long-running plugins: declarative descriptor with `start`/`stop`/`health`, config schema with validation, and `status`/`list`/`stop` · `crucible-lua`
  - **Gets you:** a plugin declares a service and gets back a spawnable descriptor; `status`/`list` report it and `stop` runs the stop hook. Each declared service is `tokio::spawn`ed at plugin boot. These are reachable **only from Lua** (or `cru lua`) — there is no RPC, CLI or TUI surface, so there is no operator visibility into running plugin services.
  - **Proof:** `crates/crucible-lua/src/lua_stdlib/tests/service.rs`::test_service_define_returns_descriptor, `::test_service_list_and_status`, `::test_service_stop`, `::test_service_define_validates_required_fields`; live `cru lua '=type(cru.service)'` → `table`; spawn site `server/plugin_boot.rs:154-166`
- [-] **Service Supervision & Secret Resolution** `P1` — supervised restart with backoff via `cru.retry`; `secret=true` resolves `CRUCIBLE_<PLUGIN>_<KEY>` from the env first · `crucible-lua`
  - **Gets you:** neither behaviour is proven. `Service.stop` also only flips a flag and calls the stop hook — it cannot cancel the spawned task, which keeps retrying.
  - **Proof:** _none — no test starts a service, makes it crash, and asserts it came back; the four service tests only `define` and inspect. The one config test mocks `crucible.config.get` to nil and asserts the schema default, never setting the env var, so the secret branch is untested. `take_service_fns` has no test caller either, so no plugin-declared service is proven to spawn end to end._
- [ ] **`cru.messaging`** `P2` — adapter trait for chat platform integrations; normalizes the receive → should_respond → session → send_and_collect → format → reply loop across Discord/Telegram/Slack/Matrix; builds on `cru.service`; **extract from two concrete implementations**, don't speculate the shape. Gate unmet — `plugins/discord/` is still the only messaging plugin, with no tests and no CI · `crucible-lua`
- [ ] **`cru.transform`** `P2` — content transform pipeline; `register(name, fn)` + `pipeline({…})` composing pure text→text functions for table formatting, mermaid rendering, citation insertion, platform-specific markdown cleanup; the unit messaging adapters plug into for `format_response` · `crucible-lua`

## Agent Protocols (ACP & MCP)

### ACP Host (Crucible → External Agents)

Crucible acts as an **ACP host**, spawning and controlling external AI agents (Claude Code, Codex, Cursor, Gemini CLI, OpenCode) with Crucible's memory, context, and permission system.

- [x] **ACP Host** `P0` — spawn and control ACP agents over JSON-RPC 2.0 on stdio, with capability negotiation · [[Help/Concepts/Agents & Protocols]] · `crucible-daemon` (acp)
  - **Gets you:** `cru chat -a <agent>` spawns a real external agent process, completes the ACP handshake, and streams its reply back into the session. Agent-reported MCP capabilities select the transport, with a stdio fallback when HTTP is unsupported.
  - **Proof:** `crates/crucible-daemon/tests/acp_smoke.rs`::mock_acp_handshake_succeeds, `::mock_acp_agent_returns_message_response` (spawns a real `mock-acp-agent` binary over stdio JSON-RPC); `crates/crucible-daemon/tests/acp_transport_negotiation.rs`::capabilities_stored_after_initialize, `::agent_reporting_http_support_gets_http_transport`. The plan/act-mode sub-clause is weaker: `session/set_mode` is serialized onto the wire but no test captures the emitted frame.
- [x] **Context Injection** `P0` — daemon-side Precognition results are prepended to the ACP prompt as a tagged System block · `crucible-daemon` (acp)
  - **Gets you:** an external agent sees knowledge-graph context it has no other way to reach, ordered before your own content. (The old entry's `PromptEnricher`, `<precognition>` XML block and `ContextConfig::inject_context` do not exist — the real marker is a `ContextMessage::system` tagged `PRECOGNITION_TAG`.)
  - **Proof:** `crates/crucible-daemon/tests/acp_smoke.rs`::injected_system_context_reaches_acp_prompt — the mock agent writes the **exact prompt text it received over the wire** to a file and the test asserts the injected content is in it; ordering in `acp_handle.rs::injected_system_context_is_prepended_to_user_content`
- [x] **In-Process MCP Host** `P0` — an MCP server running in-process; agents discover Crucible tools without an external server · `crucible-daemon` (acp)
  - **Gets you:** an external agent hits Crucible's HTTP/SSE endpoint and gets back a real tool list and real tool results, with no separate process to run.
  - **Proof:** `crates/crucible-daemon/tests/acp_integration/mcp_integration.rs`::test_in_process_mcp_sse_endpoint_is_reachable, `::test_tools_list_over_http_returns_delegate_session`; `crates/crucible-daemon/tests/acp_provider_test.rs`::test_acp_agent_with_real_providers_semantic_search_succeeds
- [x] **MCP Transport Negotiation** `P0` — HTTP for agents that support it, stdio `cru mcp --stdio --standalone` for everything else · `crucible-daemon` (acp)
  - **Gets you:** every agent gets Crucible's tools regardless of which MCP transport it speaks, without you configuring anything.
  - **Proof:** `crates/crucible-daemon/tests/acp_transport_negotiation.rs` — 11 tests including `::agent_without_http_support_falls_back_to_stdio`, `::agent_with_sse_only_gets_stdio_fallback`, `::agent_with_both_http_and_sse_gets_http_not_sse`, `::each_builtin_profile_gets_valid_mcp_transport`
- [x] **Agent Discovery** `P0` — parallel probing of the builtin profiles `opencode`, `claude`, `gemini`, `codex`, `cursor`, with per-agent env injection from `[acp.agents.*]` · `crucible-daemon` (acp)
  - **Gets you:** `cru chat -a <name>` finds the installed agent binary, applies its configured env to the spawned process, and a missing binary produces a clear error instead of a hang or panic. Custom profiles can `extends` a builtin. Known limitation: the agent cache is a process-wide static, so discovery results persist for the daemon's lifetime.
  - **Proof:** `crates/crucible-daemon/tests/acp_smoke.rs`::missing_binary_returns_connection_error; env application proven by `::injected_system_context_reaches_acp_prompt`, which depends on the child reading an injected variable; 10 profile-resolution unit tests at `acp/discovery.rs:602-833`. (The old entry's names `claude-code` and `cursor-acp` are not profile keys, and `codex` was missing from it entirely. "Parallel probing" is only wall-clock-asserted, not proven to overlap.)
- [-] **Sandboxed Filesystem** `P0` — path validation, traversal prevention, mode-based permissions for ACP agent file access · `crucible-daemon` (acp)
  - **Gets you:** nothing. Crucible does not mediate ACP agent filesystem access at all — agents use their own unmediated access, and a reader of this entry would wrongly conclude external agents run inside a Crucible sandbox.
  - **Proof:** _none — the live client handles exactly two inbound methods (`session/update`, `session/request_permission`), implements no `fs/read_text_file` or `fs/write_text_file` handler, and sends `InitializeRequest` with **no `ClientCapabilities`**, so agents are told Crucible offers no filesystem service. Both classes that do implement filesystem handling are dead: `acp/filesystem.rs` (831 lines, ~40 tests on traversal, symlink escape, null bytes, size limits) has **zero construction sites**, and `acp/acp_client.rs` is never constructed outside its own tests — and even it reads and writes whatever path the agent sends, with no root check or canonicalization. `plan = read-only` matches nothing in `acp/`. Roughly 1,100 lines of tested dead code to either wire or delete._
- [x] **Permission Gate** `P0` — ACP `session/request_permission` routes through the session's resolved permission policy (agent profile → global → CLI override); no handler ⇒ deny · `crucible-daemon` (acp)
  - **Gets you:** when an external agent asks to run an unsafe tool, your policy decides, and the agent receives the corresponding `Selected`/`Cancelled` response — a denied tool does not run.
  - **Proof:** `crates/crucible-daemon/tests/acp_integration/permission_flow.rs`::acp_permission_approved_sends_selected_response_to_agent, `::acp_permission_denied_sends_cancelled_response_to_agent` (assert the JSON-RPC frame the agent actually receives), `::acp_permission_handler_not_set_defaults_to_cancelled` (fails closed), `::acp_multiple_permission_requests_in_single_turn`; policy resolution in `tests/permission_gate_contract_tests.rs` (11 tests). Note the mechanism is `PermissionRequestHandler`, not the `PermissionGate` trait — that trait serves the internal-agent path, and the `PermissionGate`-consuming ACP client is dead code.
- [x] **Streaming Responses** `P0` — chunk processing, tool-call parsing from the stream, diff handling · `crucible-daemon` (acp)
  - **Gets you:** text, thinking and tool-call chunks appear incrementally in the chat as the external agent produces them, and file writes render as diffs; cancelling mid-stream aborts and closes the transport.
  - **Proof:** `crates/crucible-daemon/tests/acp_fixture_replay.rs`::claude_basic_chat_replays_cleanly — drives the real client against a **live recording captured from Claude Code 2.1.114**; diff handling in `acp/client/tests/diff.rs::test_generate_diff_for_write_operation`, `::test_generate_diff_for_edit_tool_string_replacement`, `::test_generate_diff_skips_read_operations`; `acp_integration/concurrent_sessions.rs::stream_edge_cancel_mid_stream_aborts_and_closes_transport`
- [-] **ACP Session Management** `P0` — sessions carrying config (cwd, mode, context size), history with ACP roles, persistence across reconnections · `crucible-daemon` (acp)
  - **Gets you:** per-session isolation, which does work. The config-carrying and persistence halves do not exist.
  - **Proof:** _none — `create_session` builds a `metadata` map with `cwd` and `mode_id` and then **drops it on the floor**: never stored, never sent, never read ("Full agent connection will be implemented in later cycles"). `load_session` is `self.active_session = Some(session); Ok(())` with the comment "actual restoration comes later", and its only caller in the workspace is a test — so persistence across reconnections is not implemented. "UUID sessions" is wrong on the live path (the id comes from the agent's `NewSessionResponse`), and "history with ACP roles" contradicts the deliberate design at `acp_handle.rs:932-937` ("ACP agents own their conversation history, so `turn()` sends only the new user content"). What does work and should replace this: per-session isolation (`concurrent_sessions.rs::concurrent_dual_sessions_isolated_no_cross_contamination`) and agent-advertised model state on `AcpSession`._
- [x] **ACP Model Switching** `P0` — `:model` on an ACP session switches the model on the **running** agent process via `session/set_model` · `crucible-daemon` (acp)
  - **Gets you:** you change model mid-conversation without restarting the agent or losing history; unknown model ids are rejected against the agent-advertised list.
  - **Proof:** `crates/crucible-daemon/tests/acp_smoke.rs`::acp_model_switching_round_trips — the mock writes the received `session/set_model` to a capture file and the test asserts the model id reached the wire and the process was not restarted
- [x] **ACP Session Recording & Replay** `P0` — `CRUCIBLE_ACP_RECORD_DIR` captures every ACP frame to a JSONL fixture replayable through the real client · `crucible-daemon` (acp)
  - **Gets you:** the whole ACP suite runs in CI with no agent binary installed — this is the mechanism that makes none of those tests `#[ignore]`d.
  - **Proof:** `crates/crucible-daemon/tests/acp_fixture_replay.rs`::claude_basic_chat_replays_cleanly (replays a live Claude Code capture); `crates/crucible-daemon/tests/acp_smoke.rs`::mock_acp_delegation_captured_in_recording; implementation `acp/client/recording.rs:70-80`, `client/replay.rs`. The env vars are documented nowhere in `docs/`.

### ACP Agent (Crucible as Embeddable Agent)

- [x] **ACP Agent Mode** `P1` — Crucible as an embeddable ACP agent (`cru acp`); any ACP host (Zed, JetBrains, Neovim) spawns Crucible to get the knowledge graph plus memory · `crucible-cli` (commands/acp)
  - **Gets you:** each ACP session is a real daemon session (Precognition, kiln tools, persistence); prompts, thinking and tool calls stream as `session/update`, and the session id shows up in `cru session list`.
  - **Proof:** hands-on during the sweep, piping framed JSON-RPC into `target/debug/cru acp --kiln docs/`: `initialize` returned the agent capabilities including `loadSession`; `session/new` returned `chat-2026-07-31T0305-2wafn0`, which then appeared in `cru session list`; `session/prompt` produced 14 `agent_thought_chunk` frames, an `agent_message_chunk` with the expected text, and `stopReason: "end_turn"`. In-repo: `crates/crucible-cli/src/commands/acp/mod.rs`::tests::initialize_round_trip_over_stdio_framing. One sub-clause is weaker — tool approvals round-tripping via `session/request_permission` is covered only by unit tests of the mapping function, not the round trip.
- [x] **ACP Schema Currency** `P1` — the pinned ACP schema and SDK are current · `crucible-daemon` (acp), `crucible-core`
  - **Gets you:** nothing left to bump. This entry used to ask for `0.10.6 → 0.10.7`; the repo is several releases past that in both directions.
  - **Proof:** `crates/crucible-core/Cargo.toml`:35 pins `agent-client-protocol-schema = "=1.6.0"`; `crates/crucible-daemon/Cargo.toml`:43 and `crates/crucible-cli/Cargo.toml`:54 use `agent-client-protocol = "0.10.4"` with `unstable_session_model`; `Cargo.lock` confirms both resolved.
- [ ] **ACP Registry Submission** `P1` — agent manifest for the [ACP Registry](https://github.com/agentclientprotocol/registry); one PR → available in all ACP clients · `crucible-daemon` (acp)

### MCP Server (External Agents → Crucible Tools)

- [x] **MCP Server** `P0` — expose the kiln as MCP tools for external AI agents · [[Help/Concepts/Agents & Protocols]] · `crucible-daemon` (tools)
  - **Gets you:** an external agent connected to `cru mcp` lists Crucible's kiln and delegation tools — 15 of them — and gets real results back. The surface is what Crucible uniquely has; it does not re-serve `bash` or file editing that the connecting harness already provides.
  - **Proof:** `crates/crucible-daemon/tests/mcp_server_tools_test.rs`::test_mcp_server_has_all_expected_tools (asserts the `list_tools` response) and `::test_all_tools_have_descriptions`; `tests/acp_integration/tool_roundtrip.rs::test_acp_tool_roundtrip_with_mcp_server`, `::test_acp_tool_roundtrip_read_file`
- [x] **Note Tools** `P0` — `create_note`, `read_note`, `read_metadata`, `update_note`, `delete_note`, `list_notes` · `crucible-daemon` (tools)
  - **Gets you:** an external agent creates, reads, updates and deletes real `.md` files in the kiln with frontmatter, and gets structured results — and these tools *are* path-sandboxed, unlike the ACP filesystem seam.
  - **Proof:** `crates/crucible-daemon/src/tools/notes/tests/crud.rs` (19 tests over a real temp kiln asserting both the file on disk and the response JSON); `notes/tests/path_safety.rs` (10 tests asserting parent-traversal, absolute-path and **symlink-escape** denial across create/read/update/delete/list/metadata)
- [x] **Search Tools** `P0` — `semantic_search`, `text_search`, `property_search` · `crucible-daemon` (tools)
  - **Gets you:** an external agent searches the kiln semantically, by text, and by frontmatter property, and gets matching notes back — with folder traversal denied.
  - **Proof:** `crates/crucible-daemon/tests/acp_provider_test.rs`::test_acp_agent_with_real_providers_semantic_search_succeeds (over HTTP, asserts the response body); text/property coverage at `crates/crucible-daemon/src/tools/search.rs`:652-1441 over real files, including `::test_text_search_folder_traversal`, `::test_text_search_absolute_folder`
- [x] **Kiln Tools** `P0` — `get_kiln_info` · `crucible-daemon` (tools)
  - **Gets you:** an external agent asks what kiln it is attached to and gets a real note count and path back.
  - **Proof:** `crates/crucible-daemon/src/tools/kiln.rs`::test_get_kiln_info_with_files, `::test_get_kiln_info_recursive`, `::test_get_kiln_info_uses_note_store`, `::test_get_kiln_info_empty`
- [x] **Workspace Tools** `P0` — `read_file`, `edit_file`, `write_file`, `bash`, `glob`, `grep` — **Crucible's own agent, not the MCP surface** · `crucible-daemon` (tools)
  - **Gets you:** Crucible's internal agent reads/edits/writes real files, runs shell commands with real exit codes and timeouts, and globs/greps the workspace. They are deliberately **not** served over MCP: a harness that speaks MCP already has its own, Crucible enforced no permission checks on the copies it served, and `agent_factory` added the same six separately so every kiln session advertised each of them to the model twice. Removed from `CrucibleMcpServer`; the surface there is kiln + delegation.
  - **Proof:** `crates/crucible-daemon/src/tools/workspace.rs`:634-920 — `test_write_file_creates_file`, `::_creates_parent_dirs`, `test_edit_file_replaces_text`, `test_read_file_returns_content_with_line_numbers`, `test_bash_executes_command`, `::_returns_exit_code_on_failure`, `::_timeout`, `test_glob_finds_files`, `test_grep_finds_matches`; plus `tests/acp_integration_e2e.rs::test_tool_dispatch_executes_read_file`
- [-] **TOON Formatting** `P0` — token-efficient response formatting · `crucible-daemon` (tools)
  - **Gets you:** TOON for **Lua plugin tool results only**. Every built-in Crucible MCP tool still returns JSON, so this entry — filed under the server that exposes those tools — promises a token saving nobody using them gets.
  - **Proof:** _none for the built-in tools — `json_ok` is the sole formatter for all 15 of them and emits `ContentBlock::json`; `rg 'toon_success'` across `mcp_server.rs`, `notes/`, `search.rs`, `workspace.rs` and `kiln.rs` returns nothing. The formatter itself is well-tested (30 tests in `tools/toon_response.rs`) and has exactly one production call site, on the Lua-plugin-tool branch of `extended_mcp_server.rs`. Two clean fixes: move the bullet under MCP Gateway and reword to "Lua plugin tool results", or route `json_ok` through `toon_success_smart`._

### MCP Gateway (Crucible → Upstream MCP Servers)

- [x] **MCP Gateway** `P0` — connect upstream MCP servers with prefixed tool names; `McpGatewayManager` is shared daemon-wide so every session sees the same upstreams · [[Help/Extending/MCP Gateway]] · [[Help/Config/mcp]] · `crucible-daemon` (tools)
  - **Gets you:** intended — tools from a configured upstream appear in the agent's tool list under a `prefix_` name and are callable. Prefix validation, allow/block filtering and precedence are unit-tested; the wiring from config → daemon bind → LLM tool defs → `GatewayToolExecutor` is complete and coherent.
  - **Proof:** _undetermined — **no test connects to any upstream MCP server**, real or faked, so nothing asserts a prefixed upstream tool actually returns a result. The one RPC-level test is vacuous (`test_mcp_status_returns_status` asserts `result.is_object() || result.is_null()`, which passes with zero upstreams configured — exactly the test environment). What would settle it: register the existing `InProcessMcpHost` as an upstream in-test and assert a `prefix_tool` call returns its result. This is untested, not dead — distinguish it from **MCP Auto-Reconnect** below. See also **MCP Config**, which is a harder blocker: both daemon bind sites hardcode `mcp_config: None`._
- [x] **Lua Plugin Tools** `P0` — dynamic tool discovery from Lua plugins · `crucible-daemon` (tools), `crucible-lua`
  - **Gets you:** a tool declared in a Lua plugin shows up in the agent's tool list and the agent can actually invoke it.
  - **Proof:** `crates/crucible-daemon/tests/plugin_tools_commands.rs`::plugin_declared_tool_is_dispatchable_by_the_agent, `::plugin_declared_command_is_listed_and_invocable`, `::plugin_commands_are_reachable_over_rpc`; discovery/registry `tools/extended_mcp_server.rs::test_list_all_tools`
- [-] **MCP Auto-Reconnect** `P0` — recover a dropped upstream without restarting the daemon · `crucible-daemon` (tools)
  - **Gets you:** nothing. If an upstream dies, its tools stay gone until the daemon restarts.
  - **Proof:** _none — `McpGatewayManager::start_reconnect_loop` (with a fully-implemented 30s→60s→120s→300s per-upstream backoff) has **zero call sites in the workspace**; the daemon builds the gateway once at bind and never spawns the loop, so `auto_reconnect: true` is an inert config default and `upstreams_needing_reconnect()` is called only by its own unit tests. A ~3-line fix at daemon bind. Two related corrections: the 30 s SSE keepalive this entry used to claim belongs to the **In-Process MCP Host** (the opposite direction of data flow), and the TUI's "live" status is one-shot — `chat_runner/runner.rs:263-293` builds a **second, throwaway** gateway from config, sends one status message and drops it ("Phase A is display-only"), inferring `connected` from a non-empty tool list rather than a connection state._
- [-] **MCP `readOnlyHint`** `P1` — an upstream tool declaring `readOnlyHint` stops being classified unsafe · `crucible-daemon` (tools)
  - **Gets you:** fewer permission interruptions when using a gateway with read-only tools — intended, and the classification change is in the tree.
  - **Proof:** _none — no test asserts the prompt is skipped for a hinted tool. A permission-decision test over a hinted versus unhinted MCP tool would settle it._

## Distribution & Growth

> How Crucible reaches users and spreads. Ordered by growth impact.
>
> **Insight from OpenClaw analysis (2026-02):** viral growth came from instant install, meeting users in apps they already use, and proactive behavior. Crucible's counter-position: "Your AI should live in your notes, not a chat app you don't control."

### Install & Onboarding (P0 — #1 adoption blocker)

- [x] **One-Line Install** `P0` — pre-built binaries via GitHub Releases; `curl -fsSL … | sh` · `crucible-cli`
  - **Gets you:** a working `cru` binary in well under a minute, with the web UI included — the release features are `["fastembed", "web"]` and CI builds `crucible-web/web/dist` with bun before the dist plan runs, so rust-embed has a real frontend to embed.
  - **Proof:** hands-on — `gh release view --json assets` on `v0.18.0` lists `crucible-cli-installer.sh`, both platform tarballs and their sha256s; config at `Cargo.toml:152` (`installers = ["shell"]`) and `:162`, plus `.github/build-setup.yml`
- [-] **Install Platform Coverage** `P0` — linux x86_64/aarch64, macOS Intel/Apple Silicon · `crucible-cli`
  - **Gets you:** two platforms, not four. `Cargo.toml:153-156` targets `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin` only, so a Linux-ARM or Intel-Mac user gets "no prebuilt binary for your platform" from the installer.
  - **Proof:** _none for the two missing targets — hands-on `gh release view` on `v0.18.0` shows exactly two platform tarballs. A leftover `aarch64-unknown-linux-gnu` entry in `github-custom-runners` makes the config *look* like it covers Linux ARM. README.md makes a third, also-wrong claim; Product.md, README and `Cargo.toml` should be made to agree, or the two targets added._
- [-] **Homebrew Tap & `cargo binstall`** `P0` — `brew install mootikins/crucible/crucible`, `cargo binstall crucible-cli` · `crucible-cli`
  - **Gets you:** unverifiable from this repo, and the in-repo evidence points the other way.
  - **Proof:** _none — the dist config declares `installers = ["shell"]` only, there is no formula or tap job anywhere in `.github/workflows/release.yml`, and no crates.io publish step, so `cargo binstall crucible-cli` has nothing to resolve. README documents `cargo install --git …` instead and mentions neither. Either drop these clauses or footnote them as externally maintained and unattestable here._
- [x] **Precognition Default-On** `P0` — changed from opt-in to on; knowledge-graph-aware context is the differentiator · `crucible-cli`, `crucible-daemon`
  - **Gets you:** a fresh session's first user message reaches the model with kiln notes injected, with no config from you.
  - **Proof:** `crates/crucible-daemon/src/agent_manager/tests/precognition.rs`::test_precognition_enriched_content_reaches_agent asserts the agent's received message list carries a System message with the kiln note title and that your own content is untouched; the default is set at `crucible-core/src/session/types/agent.rs:176` and `server/session/params.rs:145`. No test pins the *default itself* — flipping `params.rs:145` to `unwrap_or(false)` would fail nothing.
- [-] **`cru setup`** `P0` — bootstrap the runtime (plugins, themes, default `init.lua`) into `~/.config/crucible/runtime` and print the `runtimepath` line to add · `crucible-cli`
  - **Gets you:** the command exists and is the documented remedy for missing runtime files — and it is broken for exactly the users who need it.
  - **Proof:** _none for an installed binary — `setup.rs:56-70` resolves its *source* the same exe-relative way the daemon does (`../share/crucible/runtime`, then dev `../../runtime`), and release tarballs contain neither, so it bails with "Could not find Crucible runtime files. If you installed via cargo install, clone the repo and point to it." See **Bundled Runtime Plugins in Releases**._

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
  - **Proof:** `crates/crucible-web/tests/route_contract_tests/chat.rs`::chat_send_valid_message_returns_200 (drives the real router against a real JSON-RPC-over-socket mock daemon), `route_contract_tests/sessions.rs::create_session_returns_200_with_session_id`, `route_contract_tests/kilns.rs::list_kilns_returns_200_with_array`, `route_contract_tests/daemon_errors.rs::session_get_daemon_error_maps_to_502_with_error_body`; live end-to-end in `web/e2e/live/hero.live.spec.ts`
- [x] **SSE Event Bridge** `P1` — subscribe to daemon session events and stream them to HTTP clients; `EventBroker` fans out per-session · `crucible-web`
  - **Gets you:** chat tokens, thinking, tool cards and title renames stream into the browser live, and filesystem changes push tree updates without a refresh. Three SSE streams exist: `GET /api/chat/events/{session_id}`, `GET /api/fs/events` and `POST /exec` for shell output. This is **SSE only** — the sole WebSocket surface is the terminal PTY.
  - **Proof:** `crates/crucible-web/web/e2e/stories/chat-stream.story.spec.ts`::streams tokens, thinking, tool card, then completes (visual); `web/e2e/live/hero.live.spec.ts` against a real daemon; wire mapping pinned by `crates/crucible-web/src/events.rs`::real_tool_call_event_maps_id_title_and_arguments, `::real_thinking_event_maps_to_thinking`, `::real_text_delta_event_maps_to_token`, `::real_message_complete_event_maps_content_and_usage`. The old "backpressure handling" clause is dropped — both streams are plain `Sse::new(ReceiverStream)` over a bounded mpsc with no asserted drop or lag policy.
- [x] **Chat HTTP API** `P1` — session lifecycle and message send over HTTP · `crucible-web`
  - **Gets you:** create, list, pause, resume, end, archive, unarchive, cancel and export sessions; send chat messages; watch replies stream back; switch models and modes; connect and disconnect kilns; set workspace and title. The shipped surface is much wider than any enumeration will stay current with — read the routes.
  - **Proof:** `crates/crucible-web/tests/route_contract_tests/chat.rs`::chat_send_valid_message_returns_200, `::chat_send_empty_message_returns_400`, `::chat_send_missing_fields_returns_422`; session lifecycle in `route_contract_tests/sessions.rs`
- [x] **Search HTTP API** `P1` — vector, semantic and grep search plus notes and kilns over HTTP · `crucible-web`
  - **Gets you:** `POST /api/search/vectors`, `POST /api/search/semantic` (embed-then-search, the same two-step the CLI uses, with per-hit similarity scores), `POST /api/search/grep` (ripgrep with char offsets for highlighting), `GET /api/notes`, `GET /api/notes/:name`, `PUT /api/notes/:name`, `GET /api/notes/resolve`, `GET /api/backlinks`, `GET /api/kilns`.
  - **Proof:** `crates/crucible-web/tests/route_contract_tests/kilns.rs`::search_vectors_returns_200_with_results, `::search_semantic_returns_200_with_results`, `::search_semantic_blank_query_returns_empty`, `::list_notes_with_kiln_returns_200`, `::backlinks_returns_linked_and_filtered_unlinked`; `crates/crucible-web/src/routes/search.rs`::grep_search_maps_hits_to_wire_shape
- [x] **API Auth** `P1` — Bearer token middleware with an auto-generated key; localhost bypass; `~/.config/crucible/api_key` persistence · `crucible-web`, `crucible-core` (config)
  - **Gets you:** a non-loopback caller without the right Bearer token or session cookie gets 401, loopback callers pass, and a 0600 key file is generated on first start. `X-Forwarded-For` defeats the localhost bypass, and the shell and PTY routes carry an *additional* localhost-only gate plus a WebSocket Origin allow-list.
  - **Proof:** `crates/crucible-web/src/middleware/auth.rs`::bearer_auth_rejects_invalid_token, `::bearer_auth_rejects_missing_header`, `::bearer_auth_bypasses_for_localhost`, `::bearer_auth_bypasses_for_ipv6_localhost`, `::bearer_auth_no_longer_accepts_url_tokens`, `::resolve_api_key_at_generates_and_persists_when_missing_or_empty` (asserts the key length, file contents and `mode & 0o777 == 0o600`). Gap worth naming: no test asserts the composition in `start_server` itself, so moving a `.merge()` above the `.layer()` would silently unauthenticate a route group with every test still green.
- [x] **Cookie Session Auth** `P1` — the browser posts the API key once to `/api/auth/login` and gets an HttpOnly session cookie · `crucible-web`
  - **Gets you:** the key never appears in a URL — the security-relevant half of browser auth, which the Bearer-token entry above does not cover.
  - **Proof:** `crates/crucible-web/src/routes/auth.rs`::login_with_valid_key_sets_httponly_cookie; client prompt `web/src/components/__tests__/AuthTokenPrompt.test.tsx` and `web/src/lib/__tests__/api-token.test.ts`; public-route wiring at `src/server.rs:132-136`
- [-] **Webhook API** `P1` — `POST /api/webhook/:name` receives payloads and broadcasts `webhook:received` for Lua handlers · `crucible-web`, `crucible-daemon`
  - **Gets you:** a route that accepts a POST and returns `{"status":"ok"}`. Nothing else happens, and the GitHub/IFTTT/Zapier integrations this entry sells rest entirely on the dead half.
  - **Proof:** _none — the only bridge from the daemon broadcast bus into the Lua handler registry is `server/file_event_hooks.rs`, whose `to_internal_event` returns `Some` for exactly `"file_changed"` and `"file_deleted"`; everything else hits `_ => None` and is dropped, so no Lua handler can ever see a webhook. There are zero occurrences of "webhook" in `crucible-lua`, `runtime/plugins/` or `plugins/`. Second, independent reason: the route is mounted **inside** bearer auth, so an external service cannot reach it without the API key — which no external service will send._

### Messaging Integrations (P1 — meet users where they are)

> 1–2 good messaging integrations reduce the need for a web UI substantially. Integrations can be daemon-side Lua plugins or thin adapters over the HTTP gateway.

- [ ] **Telegram Bot** `P1` — Bot API adapter over the HTTP gateway; lowest friction, enables proactive digest delivery · `crucible-telegram` (new crate) · depends: [[#HTTP Gateway|HTTP-to-RPC Bridge]]
- [-] **Discord Plugin** `P1` — Discord integration (REST + Gateway) as a daemon-side Lua plugin · `plugins/discord/`
  - **Gets you:** a complete-looking plugin — four tools (`discord_send`, `discord_read`, `discord_channels`, `discord_register_commands`), a `:discord connect/disconnect/status` command and a gateway service, with every Lua API it calls genuinely existing — that **a normal install never loads**, and that nothing proves ever sends or receives a message.
  - **Proof:** _none, for two independent reasons. (1) Not on a load path: `daemon_plugin_paths` searches `CRUCIBLE_PLUGIN_PATH`, `~/.config/crucible/plugins/`, `<runtimepath>/plugins` and `$CRUCIBLE_RUNTIME/plugins` — the repo-root `plugins/` directory is none of those, and `runtimepath` defaults to empty, so a user must hand-copy it. (2) No test of any kind: `runtime/plugins/{kiln-expert,oci,reflection}` each have a suite; discord has none, and no Rust test references it. Its headline claim — proof-of-concept that a plugin can *be* a gateway — is precisely the unproven part. Either move it under `runtime/plugins/` (what the prose implies) or say plainly that it is a sample requiring manual installation._
- [ ] **Matrix Bridge** `P2` — Matrix protocol integration; strong overlap with the self-host/privacy audience · `crucible-matrix` (new crate) · depends: [[#HTTP Gateway|HTTP-to-RPC Bridge]]

### Remote Access (P2 — self-hosting for everyone)

> Agents can't be on every device. Self-hosting with easy remote access is more aligned with "local-first, your notes, your control" than paid cloud hosting.

- [x] **Remote Access Key Distribution** `P2` — `cru web key [--rotate]` prints or rotates the API key remote clients need · `crucible-cli`, `crucible-web`
  - **Gets you:** the connect URL is printed at startup, `cru web key` gives you the key to paste into another device (web UI → Settings → API Access), localhost never needs it, and `api_key = ""` disables auth explicitly. This is currently the **only shipped path** to using Crucible from another device.
  - **Proof:** `crates/crucible-cli/src/commands/web.rs`:85 (`handle_key` prints key and rotation state to stdout); gate behaviour in `crates/crucible-web/src/middleware/auth.rs`::remote_shell_gate_allows_non_localhost_when_active, `::remote_shell_active_is_fail_closed_without_an_api_key`
- [ ] **`cru tunnel`** `P2` — one-command remote access setup wrapping `cloudflared tunnel` or `tailscale funnel`; exposes the HTTP gateway with auth to your devices · `crucible-cli`
- [ ] **Cloudflare Tunnel Integration** `P2` — `cru tunnel --cloudflare`; auto-configures `cloudflared` with API auth; free tier for personal use · `crucible-cli`
- [ ] **Tailscale Funnel Integration** `P2` — `cru tunnel --tailscale`; WireGuard encrypted, ACL-gated; zero-config for Tailscale users · `crucible-cli`
- [ ] **Paid Hosting** `P?` — multi-tenant hosted option; needs daemon isolation, user management, billing; deferred until clear demand · future

### Proactive Behavior (P2 — viral feature)

> OpenClaw's most praised feature was the heartbeat — the agent reaching out unprompted. Crucible can do this better because it has a knowledge graph, not flat memory. Heartbeat is time-based; webhook triggers are event-driven — Crucible can do both, though the webhook half currently reaches no handler.

- [x] **Scheduled Lua Hooks** `P2` — `cru.schedule({every=N}, fn)` interval callbacks with `cru.schedule.cancel(handle)` · `crucible-lua`, `crucible-daemon`
  - **Gets you:** a Lua callback that actually fires on a timer and actually stops when cancelled, on the daemon's own plugin VM. A `MAX_ACTIVE_SCHEDULES = 256` cap is enforced and undocumented elsewhere.
  - **Proof:** `crates/crucible-lua/src/schedule.rs`::schedule_runs_and_can_be_cancelled (increments a Lua-side counter from the spawned task, asserts it moved after 180 ms, cancels, asserts it stopped), `::schedule_accepts_table_spec`, `::schedule_rejects_non_positive_interval`; registered on the daemon VM at `daemon_plugins/mod.rs:137`, with the `send` feature enabled so the daemon gets the live implementation rather than the erroring stub
- [-] **Declarative `[[schedules]]` Config** `P2` — `[[schedules]]` blocks (`name`, `every`, `action = "lua:<code>"`, `enabled`) register interval callbacks at daemon boot with no plugin required · `crucible-daemon`, `crucible-core`
  - **Gets you:** the no-code half of the proactive story — intended, parsed, and evaluated onto the plugin VM at boot.
  - **Proof:** _none — `server/plugin_boot.rs:178-217` parses each entry and evals `cru.schedule(…)`, but no test asserts a config-declared schedule ever fires. Only the Lua API beneath it is tested._
- [ ] **Kiln Digest** `P2` — periodic scan of recent kiln changes surfacing missed connections ("You wrote about X in two notes this week — want me to link them?"); delivered via messaging or TUI notification. Partial prior art in `plugins/discord/lua/digest.lua`, which is Discord-scoped and disabled by default · `crucible-daemon`, `crucible-lua`
- [ ] **Daily Briefing Plugin** `P2` — reference plugin summarizing recent kiln changes, pending tasks and orphaned notes; delivered via messaging or shown on TUI startup · `crucible-lua`

### Default Runtime Plugins (P1 — Neovim-style bundled plugins)

> Crucible ships a `runtime/` directory of Lua plugins alongside the binary, analogous to Neovim's `$VIMRUNTIME/plugin/` — in the repo. Release tarballs do not carry it; see **Bundled Runtime Plugins in Releases**. These load automatically, are overridable, and their source code *is* the documentation for how to build plugins. The bundled set is `kiln-expert`, `oci` and `reflection`.
>
> **Plugin search path (priority order):**
> 1. `CRUCIBLE_PLUGIN_PATH` — env override (highest priority)
> 2. `~/.config/crucible/plugins/` — user global
> 3. `KILN/.crucible/plugins/` — kiln personal (gitignored)
> 4. `KILN/plugins/` — kiln shared (version-controlled)
> 5. `$CRUCIBLE_RUNTIME/plugins/` — bundled default (lowest priority)

- [x] **Runtime Plugin Path & Provenance** `P1` — `$CRUCIBLE_RUNTIME/plugins/` as a real lowest-priority search path with `PluginSource` tracking · `crucible-lua`, `crucible-daemon`
  - **Gets you:** setting `CRUCIBLE_RUNTIME` (or installing to `<prefix>/share/crucible/runtime`) makes that directory's `plugins/` a real search path tagged `PluginSource::Runtime`, and `source` and `version` reach the `plugin.list` RPC body.
  - **Proof:** `crates/crucible-daemon/src/daemon_plugins/tests.rs`::test_default_paths_includes_runtime_when_set (asserts the returned path list contains a tempdir path with `*src == PluginSource::Runtime`), `::test_runtime_path_resolved_from_exe`
- [x] **Plugin Shadow-by-Name** `P1` — a same-named user plugin wins over the runtime copy · `crucible-lua`
  - **Gets you:** intended — the headline promise of the whole priority table above.
  - **Proof:** _undetermined — the implementation is a `if self.plugins.contains_key(&name) { continue }` skip at `lifecycle/discovery.rs:70-72` and `:88-91`, and **no test anywhere asserts plugin shadow-by-name**; the workspace's other `shadow` tests are for skills, agent cards, runtime *defaults* and slash commands. A two-path discovery test asserting which copy loaded would settle it: `a_user_plugin_shadows_a_same_named_runtime_plugin`._
- [-] **Provenance in `:plugins`** `P1` — `:plugins` shows `[core]`, `[runtime]`, `[user]`, `[kiln]` tags · `crucible-cli`
  - **Gets you:** nothing. No surface in the product displays a plugin's provenance.
  - **Proof:** _none — `source` goes on the wire and is read by nobody: `PluginStatusEntry` has only `{name, version, state, error}`, with **no `source` field at all**, so provenance is dropped before it reaches the TUI. `:plugins` renders `"  {icon} {name} v{version} ({state})"` and `cru plugin list` prints `NAME VERSION STATE TOOLS/CMDS/HOOKS`; neither reads `source`. Asserted as fact in two other places, both now corrected._
- [-] **Bundled Runtime Plugins in Releases** `P1` — the bundled plugins reach an installed user · `crucible-cli`
  - **Gets you:** nothing. `kiln-expert`, `oci` and `reflection` never reach anyone who did not clone the repo.
  - **Proof:** _none — hands-on: `dist-manifest.json` for `v0.18.0` shows both platform tarballs contain exactly `CHANGELOG.md`, `LICENSE-APACHE`, `LICENSE-MIT`, `README.md` and the `cru` executable. **No `runtime/` directory, no `share/crucible/`**, and `[workspace.metadata.dist]` has no `include` key. `daemon_plugin_paths` looks for `<exe>/../share/crucible/runtime/plugins`, a path the tarball never creates, and the documented escape hatch `cru setup` resolves its source the same way and bails. Cross-section consequence: the **Reflection Pass** cannot run for any installed user either. The fix is one `include` key plus an installer that unpacks it._
- [x] **`kiln-expert` Runtime Plugin** `P1` — on-demand search across *unmounted* kilns via subagent delegation · `runtime/plugins/kiln-expert/`
  - **Gets you:** you configure a `kilns` map of label → path and the agent can search a kiln that is not attached to the session, without you mounting it first.
  - **Proof:** `runtime/plugins/kiln-expert/plugin.yaml` plus its Lua suite in `runtime/plugins/kiln-expert/tests/`, run in CI by `server/lua_plugin_suite.rs::shipped_plugin_lua_suite_passes`. Subject to the packaging gap above.

### Ecosystem & Shareability (P1-P2)

- [-] **Plugin Install** `P1` — `cru plugin add <git-url>` / `cru install` clone a plugin from git (branch/pin supported); git-native distribution, lazy.nvim model · `crucible-cli`, `crucible-daemon`
  - **Gets you:** plausibly a cloned plugin in search path #2. The path is complete and straight-line — a real `git clone` (`--depth 1` unless pinned, `--branch` honoured, `--` terminator against argv injection), `git checkout <pin>` with rollback on failure, then a flock-guarded read-modify-write of `plugins.toml` — and URL/name hardening is thoroughly tested. This is an *untested* path, not an inert one.
  - **Proof:** _none — no test in the workspace references `plugin_ops::install`, `bootstrap_plugin_entry`, or `cru plugin add`, and the reason is structural: `plugins_dir()` and `plugins_toml_path()` read `dirs::config_dir()` with no injectable path, so a test would clone into the developer's real `~/.config/crucible`. It could not be run hands-on either — `normalize_git_url` rejects `file://` by design, so there is no offline URL to install from. The fix is the same injectable-path treatment the daemon's `data_home` got. Separately: `cru plugin add` clones only — discovery of `~/.config/crucible/plugins/` happens at daemon boot, so a newly added plugin needs a daemon restart._
- [x] **Documentation Site** `P1` — a published Starlight site built from `docs/` in place, deployed to GitHub Pages · `docs-site/`
  - **Gets you:** the docs kiln as a browsable public site with a landing hero, a per-page neighbourhood graph, hover previews that promote to floating windows, and canvases published as read-only boards.
  - **Proof:** `docs-site/astro.config.mjs:12` (`site: 'https://mootikins.github.io'`) plus the deploy job in `.github/workflows/pages.yml`; board rendering imports the web app's own canvas geometry
- [ ] **Agent Memory Branding** `P1` — rename "Precognition" to "Agent Memory" in user-facing docs; communicates the value proposition directly · docs
- [ ] **`cru share`** `P2` — export sessions as self-contained HTML or shareable artifacts; `:export` exists for local markdown, this adds shareable formats · `crucible-cli`
- [ ] **Graph Visualization** `P2` — shareable knowledge graph renders (SVG/HTML) you can send someone; creates viral demo moments. Deliberately still `[ ]`: the shipped **Knowledge Graph Visualization** (web) is an in-browser interactive view with no exportable artifact, and the docs site's neighbourhood graph is a published page rather than a shareable file. Worth deciding whether that second one supersedes this entry · `crucible-cli` or `crucible-web`

## Workflow Automation

- [x] **Workflow Markup** `P2` — DAG workflows in markdown: `@agent`, `->` data flow, `> [!gate]` · [[Help/Workflows/Workflow Syntax]] · `crucible-core` (parser + engine), `crucible-daemon`
  - **Gets you:** a note with `type: workflow` frontmatter parses `## Step @agent -> out [k:: v]` headings and `> [!gate]` callouts into an executable DAG, and `cru workflow start <note>` runs it. `cru workflow list` / `show` / `start` / `approve` / `status` / `cancel` all exist.
  - **Proof:** `crates/crucible-core/src/parser/types/workflow.rs`:61-148 defines `WorkflowStep.agent`, `.output`, `Gate` and `ValidationEntry`; exercised end to end by `crates/crucible-core/src/workflow/engine.rs`::parallel_member_outputs_bind_scope_after_join (asserts `-> out_a` binds into `exec.scope()`); reachable from the CLI via `commands/workflow.rs:412` → `workflow.start` RPC → `rpc/workflow_handlers.rs:81`
- [x] **Parallel Execution** `P2` — `(parallel)` heading suffix and `&` step prefix for concurrent steps; consecutive parallel siblings join before the next step · [[Help/Workflows/Workflow Syntax]] · `crucible-core` (parser + engine), `crucible-daemon`
  - **Gets you:** two parallel sibling steps genuinely execute concurrently and the next step does not start until both have joined; a failing member fails the workflow *after* the join, reporting all failures; a run of one degrades to plain sequential.
  - **Proof:** `crates/crucible-core/src/workflow/engine.rs`::parallel_section_children_run_concurrently — registers a handler backed by `tokio::sync::Barrier::new(2)`, so the workflow can only complete if both branches are in flight simultaneously (sequential execution deadlocks). Plus `::parallel_member_outputs_bind_scope_after_join`, `::failing_parallel_member_fails_workflow_after_join_reporting_all_failures`, `::single_parallel_step_runs_sequentially`. A genuine concurrency proof rather than a mock; the only gap is that nothing exercises it through the `workflow.start` RPC.
- [x] **Workflow Resume** `P2` — a workflow interrupted by a daemon restart picks up where it stopped · [[Help/Workflows/Index]] · `crucible-daemon`
  - **Gets you:** the non-terminal snapshot on disk is rehydrated on the next `workflow.status` / `approve_gate` / `cancel` call for that session, with parallel-group position preserved.
  - **Proof:** `crates/crucible-daemon/src/rpc/workflow_handlers.rs`:296-321 (`resolve_or_rehydrate` reads `WORKFLOW_STATE_FILE` and calls `WorkflowExecution::rehydrate`), persisted at `:318-338`; snapshot fidelity asserted by `crates/crucible-core/src/workflow/engine.rs`::snapshot_roundtrip_preserves_parallel_group_position. No RPC-level test covers the rehydrate path — `crates/crucible-daemon/tests/` has no workflow test file at all, so the snapshot→resume chain is proven only at the engine layer.
- [x] **Workflow Authoring** `P2` — guide for creating workflows · [[Help/Extending/Workflow Authoring]]
  - **Gets you:** the authoring guide the entry points at, plus [[Help/Workflows/Index]] and [[Help/Workflows/Workflow Syntax]].
  - **Proof:** `docs/Help/Extending/Workflow Authoring.md` present in the docs kiln. Content currency was not assessed.
- [ ] **Workflow Markdown Log** `P2` — render a workflow run as markdown. Persistence today is `serde_json::to_vec_pretty` into `WORKFLOW_STATE_FILE`; nothing renders a run · `crucible-daemon`
- [ ] **Session Learning** `P2` — codify successful sessions into reusable workflows. (The Reflection Pass is a different mechanism — it proposes notes, not workflows.)

## Storage & Processing

- [x] **SQLite Backend** `P0` — the default and only storage backend · [[Help/Config/storage]] · `crucible-daemon` (storage)
  - **Gets you:** notes written into a kiln are parsed into SQLite and returned by `list_notes` / `get_note_by_name` over the daemon socket, across the notes, note_links, entities, properties, relations, blocks, tags and entity_tags tables.
  - **Proof:** `crates/crucible-daemon/tests/rpc_kiln_e2e.rs`::test_list_notes_returns_seeded_notes (spawns a real daemon, opens a seeded kiln, asserts all 3 notes by name) and `::test_kiln_lifecycle_open_query_close`
- [x] **Vector Embeddings** `P0` — FastEmbed (ONNX) local embedding generation · [[Help/Config/embedding]] · `crucible-daemon` (llm)
  - **Gets you:** a real 384-dim finite embedding vector per text, with `batch_size` reaching the actual inference call. **Not on by default**, despite `EmbeddingProviderConfig::default()` being FastEmbed: `CliAppConfig.enrichment` is an `Option` with no `Some(default)` fallback, so a config with no `[enrichment]` section gives the daemon no embedding provider at all — and neither `cru init` nor the setup wizard writes one. The error you then hit tells you to set `[embedding]`, a section the loader **hard-rejects as legacy**, so following the message makes the config unloadable. A first-run trap worth fixing at both ends.
  - **Proof:** `crates/crucible-daemon/src/llm/embeddings/fastembed.rs`::test_fastembed_single_embedding (asserts `embedding.len() == 384` and every value finite from a real `embed` call, not `#[ignore]`d); `batch_size` reaches inference at `fastembed.rs:378`
- [-] **Embedding Reranking** `P0` — search result reranking for relevance · `crucible-daemon` (storage)
  - **Gets you:** nothing — result order is identical with and without it. The reranker never runs.
  - **Proof:** _none — `crates/crucible-cli/src/core_facade.rs`:137-146 is a passthrough whose entire body is a `tracing::debug!("Reranking not available in this storage mode…")` followed by `self.semantic_search(...)`. `FastEmbedReranker` has zero call sites outside its own module, its would-be caller `enrich_with_reranking` is itself uncalled, and there is no `rerank` RPC method. Note the two existing "reranking" tests are **vacuous** — they build the expected string and arithmetic inline in the test body and never call product code._
- [x] **File Processing** `P0` — parse, enrich and index notes via a pipeline · [[Help/CLI/process]] · `crucible-daemon`
  - **Gets you:** `cru process` / `process_file` / `process_batch` parse a markdown file and land it in the index, with `process_batch` emitting progress events. Parse and index are unconditional; "enrich" is conditional on an embedding provider being configured (see **Vector Embeddings**), which by default it is not.
  - **Proof:** `crates/crucible-daemon/tests/rpc_kiln_e2e.rs`::test_list_notes_returns_seeded_notes (processed notes come back over RPC); batch progress events at `server/kiln.rs:667-677`
- [x] **Hash-based Change Detection** `P0` — content-addressable block hashing · `crucible-core`
  - **Gets you:** re-processing an unchanged file is skipped, and `force_reprocess` overrides the skip.
  - **Proof:** `crates/crucible-daemon/src/pipeline/note_pipeline.rs`:717-729 (identical content: first result not skipped, second skipped) and the companion at `:818-827` asserting `force_reprocess: true` defeats it
- [-] **Transaction Queue** `P0` — batched database operations with consistency · `crucible-daemon` (storage)
  - **Gets you:** nothing by that name. Writes are **per-note transactional** — `SqlitePool::with_transaction` is a plain `BEGIN TRANSACTION` closure wrapper with three call sites, each one note per transaction. `process_batch` batches *file processing* with progress events, not database operations.
  - **Proof:** _none — there is no queue component to point at. `DatabaseError::Transaction` is an unused variant and `ProcessingResult::transaction_count` is set only inside its own module's tests. Better reworded than left as a bare `[-]`: "per-note transactional writes"._
- [-] **Task Storage** `P0` — task records, history, dependencies, file associations · `crucible-daemon` (storage)
  - **Gets you:** nothing in storage. Tasks are a markdown harness — see **Task Harness (`TASKS.md`)** under Note-Taking & Authoring, which is the real, working capability.
  - **Proof:** _none — the SQLite schema has no tasks table, `METHODS` has no `task.*` entry, and `cru tasks` operates on `crucible_core::parser::{TaskFile, TaskGraph}` over a markdown file. Dependencies come from `TaskGraph` over frontmatter; "history" and "file associations" have no backing at all. The crate attribution was wrong too — this is `crucible-core` (parser) + `crucible-cli`._
- [x] **Kiln Statistics** `P0` — `cru stats` file and size metrics · [[Help/CLI/stats]] · `crucible-cli`
  - **Gets you:** total files, markdown files, total size in KB and the kiln path, as text or with `-f json`.
  - **Proof:** `crates/crucible-cli/src/commands/stats.rs`:126-134 (the actual output block) with `::test_execute_with_mock_service`, `::test_stats_output_json_serialization`, `::test_filesystem_service_with_markdown_files`, `::test_filesystem_service_recursive`
- [-] **Indexed Note & Link Metrics** `P0` — note counts from the index and link analysis in `cru stats` · `crucible-cli`
  - **Gets you:** neither. `KilnStats` has exactly three fields (`total_files`, `markdown_files`, `total_size_bytes`) and its collector is a plain recursive directory walk — so a kiln with 500 unindexed files reports 500, and no link analysis happens anywhere in `cru stats`.
  - **Proof:** _none — `crates/crucible-cli/src/commands/stats.rs`:11-16 is the whole struct. Link analysis does exist, but as the `kiln.graph` RPC and the web graph view._
- [x] **Daemon Server** `P0` — Unix socket JSON-RPC server; `daemon.capabilities` → `methods` is the live method list · `crucible-daemon`
  - **Gets you:** every `cru` subcommand and every web route talking to one process over a socket. The method count is served at runtime rather than carried here — the two counts this document used to hold (35 and 55) were both wrong and disagreed with each other.
  - **Proof:** the whole `rpc_*_e2e.rs` family under `crates/crucible-daemon/tests/` drives a real socket; `rpc/dispatch.rs:2126-2136` asserts `METHODS` contains the named methods with no duplicates
- [x] **Daemon Client** `P0` — auto-spawn, version check, RPC client library · `crucible-daemon` (rpc)
  - **Gets you:** any `cru` subcommand transparently spawns the daemon if it isn't running, and shuts down and respawns one whose build SHA does not match. Note "reconnect" is **web-only** — `crucible-web` does a generation-guarded reconnect that also rewires the SSE router; the CLI/TUI client has none, so a daemon restart mid-session leaves the TUI on a dead socket.
  - **Proof:** `crates/crucible-daemon/src/rpc_client/client/mod.rs`:109-176 (`connect_or_start` → `verify_or_restart` → `start_and_retry` with exponential backoff over 10 attempts); every daemon integration test connects through this client against a real socket
- [x] **Event Subscriptions** `P0` — per-session and wildcard event streaming · `crucible-daemon`
  - **Gets you:** a client subscribing to a session id (or `"*"`) receives that session's events over the socket as they fire; an event addressed to `"*"` reaches everyone.
  - **Proof:** `crates/crucible-daemon/tests/session_create_emits_setup_events.rs`::session_create_emits_setup_events_for_internal_agent (subscribes with `["*"]` against a real daemon and asserts the specific setup events arrive); fan-out at `server/core.rs:81-82`
- [x] **Notification RPC** `P0` — add, list and dismiss notifications via the daemon · `crucible-daemon`
  - **Gets you:** `session.add_notification` / `list_notifications` / `dismiss_notification` add, return and remove notifications across toast, progress and warning kinds, and the TUI renders them.
  - **Proof:** `crates/crucible-daemon/tests/notification_rpc.rs`::test_list_notifications_after_adding (asserts the response body's `id`, `kind`, `message`) and `::test_dismiss_notification_removes_from_list`, both against a real spawned daemon
- [x] **File Watching** `P0` — file change detection (notify/polling, debouncing, daemon bridge) with auto-reprocessing: `file_changed` events trigger `pipeline.process()` via the daemon reprocess task; enrichment disabled for now (parsing + storage only) · `crucible-daemon` (watch)
  - **Gets you:** a note you create, edit, or delete while the daemon is running is indexed on its own, without `cru process`.
  - **Proof:** `crates/crucible-daemon/tests/watch_indexing.rs`::note_created_while_daemon_runs_becomes_searchable, `::note_deleted_while_daemon_runs_leaves_the_index` — both open a kiln through the real server, touch a file, and assert on `list_notes`; both failed before `create_default_handlers` registered anything
- [-] **Storage Maintenance Commands** `P0` — `storage.verify`, `storage.cleanup`, `storage.backup`, `storage.restore` and the `cru storage` command module · `crucible-daemon` (storage), `crucible-cli`
  - **Gets you:** the RPCs are dispatched and reachable from the CLI, which is a real user-visible capability set with no prior entry at all.
  - **Proof:** _none — the methods are present in `rpc/dispatch.rs:437-440` and the command module exists, but the sweep did not verify that any handler reaches a real effect. Enter as in-progress until one does._
- [x] **Git / SCM Project Integration** `P0` — `scm.clone`, `scm.branches`, `scm.worktree_add` · `crucible-daemon`, `crucible-web`
  - **Gets you:** create a project from a remote repo URL, and back a repo/branch picker on the web composer — pick a branch to jump to its worktree or create one from a `worktree_dir` template. N sessions across N worktrees without leaving the composer. Config knobs are `[scm] projects_dir` / `worktree_dir` / `session_workspace_dir`.
  - **Proof:** `crates/crucible-daemon/src/scm.rs`::collect_branches_and_add_worktree_against_real_git (runs real `git`, asserts the worktree on disk), `::clone_repo_against_real_git_fixture`, `::clone_dest_contained_to_projects_dir`, `::clone_dest_rejects_symlink_hop_out_of_base`
- [ ] **Burn Embeddings** `P?` — Burn ML framework for local embeddings. **Removed, not stubbed**: `llm/embeddings/mod.rs:77-78` returns `ConfigError("Burn provider is no longer included in crucible-daemon::llm")`, while `EmbeddingProviderConfig::Burn` still exists as a config variant — so a `type = "burn"` config parses and then fails at runtime · `crucible-daemon` (llm)
- [ ] **LlamaCpp Embeddings** `P?` — GGUF model inference for embeddings · `crucible-daemon` (llm)
  - **Gets you:** unclear. `llm/embeddings/gguf_model.rs` and `inference.rs` exist (the latter with a `batch_size: 512` default), but there is no `LlamaCpp` variant in `EmbeddingProviderConfig` and no `BackendType` arm reaching them.
  - **Proof:** _undetermined — the sweep did not establish whether these are a live-but-unwired GGUF path or dead code left by the Burn removal. Settle it by checking whether anything constructs the type in `inference.rs` outside its own tests._
- [ ] **Session Compaction** `P?` — compact sessions with cache purge for memory efficiency. **Worse than unimplemented — an active hazard that fires automatically**: auto-compaction trips at 0.95 of `context_budget` and calls `request_compaction`, which sets `SessionState::Compacting`; because nothing consumes that state the session is stuck in it — `session.list` reports `compacting`, `session.pause` then fails its `state != Active` guard, and a later `session.compact` returns `InvalidState`. See **Auto-Compaction** · `crucible-daemon`

## Configuration & Setup

- [-] **Config System** `P0` — TOML config with profiles, includes and environment overrides · [[Help/Configuration]] · `crucible-core` (config)
  - **Gets you:** none of the three named features. What the live loader (`CliAppConfig`) actually does is `{file:}` / `{dir:}` / `{env:}` substitution **inside values**, plus three CLI-flag overrides — priority is CLI flags → config file → defaults, with env absent as a layer. `ValueSourceMap` (`cru config show --trace`) tracks File/Cli/Default and is live.
  - **Proof:** _none for any of the three — `profiles` lives on `Config`, reachable only through `ConfigLoader`, which has **no production caller**; `merge_includes` (the `[include]` section reader) is called from exactly one place, inside that same dead loader; and there is no environment-override pass anywhere in `CliAppConfig::load_inner`, whose own doc comment lists the priority without env. All ~40 include tests call the functions directly, none through `CliAppConfig::load`. The dead `Config`/`ConfigLoader` module is ~670 lines plus a 246-line `profile.rs`. Note also `crucible_core::config::AppConfig`, which CLAUDE.md names as the canonical config type, does not exist as a struct or alias — that doc needs the same correction._
- [x] **Provider Config** `P0` — `[llm.providers.<name>]` type / endpoint / api_key / default_model across nine backends · [[Help/Config/llm]] · `crucible-core` (config)
  - **Gets you:** which backend a new session talks to, and the model list users pick from. Nine backends exist (Ollama, OpenAI, Anthropic, Cohere, VertexAI, OpenRouter, GitHubCopilot, ZAI, FastEmbed), not the three this entry used to name. `LlmProviderConfig.temperature` and `.max_tokens` are inert — see **Agent Config**. Legacy `[providers]`, `[embedding]` and `chat.provider` are hard-rejected at load with actionable errors.
  - **Proof:** `crates/crucible-daemon/src/server/session/create.rs`:263 resolves `default_provider()` at session creation; `agent_manager/providers.rs:20-63` builds `ProviderInfo` straight from config and returns it via `providers.list` → `cru models` and the TUI model picker
- [x] **Embedding Config** `P0` — `[enrichment.provider]` type, model and batch size · [[Help/Config/embedding]] · `crucible-core` (config)
  - **Gets you:** which backend embeds and how many texts go per request — `batch_size` sizes the real HTTP request for Ollama (and switches to the legacy single-request endpoint at `<= 1`) and threads into `model.embed` for FastEmbed.
  - **Proof:** `crates/crucible-daemon/src/llm/embeddings/ollama.rs`:371 (`for chunk in non_empty.chunks(self.batch_size)`) and `::test_provider_batch_size_from_config`; `fastembed.rs:363-378`
- [-] **Pipeline Tuning Knobs** `P0` — `[enrichment.pipeline]` concurrency and resilience settings · `crucible-core` (config)
  - **Gets you:** nothing from eight of nine fields. Only `max_precognition_chars` is read in production; `worker_count`, `batch_size`, `max_queue_size`, `timeout_ms`, `retry_attempts`, `retry_delay_ms`, `circuit_breaker_threshold` and `circuit_breaker_timeout_ms` are all inert.
  - **Proof:** _none — `PipelineConfig.worker_count` defaults to `num_cpus::get()` and has zero consumers; setting `worker_count = 1` changes nothing observable. Several of the dead knobs (`circuit_breaker_*`, `retry_*`) imply resilience machinery that does not exist. `docs/Help/Config/embedding.md` documents the section and is worth checking against this list._
- [-] **Storage Config** `P0` — backend selection, embedded vs daemon mode · [[Help/Config/storage]] · `crucible-core` (config)
  - **Gets you:** neither capability, and the whole section now does nothing.
  - **Proof:** _none — `StorageConfig` has exactly one field, `idle_timeout_secs`. There is no `backend` field at all, so `backend = "sqlite"` is silently dropped as an unknown key; the custom `Deserialize` accepts a legacy `mode` key solely to warn that it "has no effect. The daemon is the only storage backend." And `idle_timeout_secs` is itself inert — its only read in the workspace is a `println!` in `cru status`, while its doc comment claims a daemon idle-shutdown that does not exist. Compounding it, `cru init` writes `[storage] backend = "sqlite"` into every new kiln config, teaching a knob with no effect._
- [-] **MCP Config** `P0` — upstream MCP server connections · [[Help/Config/mcp]] · `crucible-core` (config)
  - **Gets you:** upstream servers in the TUI's `:mcp` list with a tool count. Their tools reach no agent, and web sessions get not even the list.
  - **Proof:** _none for tool availability — **both daemon bind sites hardcode `mcp_config: None`** (`crucible-cli/src/main.rs:103` and `commands/daemon.rs:72`), so `ctx.mcp_gateway` is always `None`. The consumer is fully built and waiting (`agent_factory.rs:228`, `:604` feed gateway tools into the agent's tool definitions); it never receives a gateway. Separately, the TUI connects its own throwaway gateway purely to count tools for display and then drops it. All the filtering logic is unit-tested against configs with no live upstream. See also **MCP Gateway**._
- [-] **Project Config** `P0` — attached-kiln declarations in `.crucible/project.toml` · [[Help/Config/workspaces]] · `crucible-core` (config)
  - **Gets you:** nothing from the `kilns = [...]` table. It is parsed, test-asserted, and ignored.
  - **Proof:** _none — `ProjectConfig.kilns` has no production consumer (`rg '\.kilns\b'` outside tests returns only assertions inside the config crate's own test modules), and the two production callers of `read_project_config` are both in `crucible-web` reading only `security.project_files`. The type this entry used to name, `WorkspaceConfig`, carries `#[deprecated]` and has zero production references. Multi-kiln association is real but via a **different** mechanism: the global registry in `~/.crucible` plus `KilnManager::open_named_kilns` and `session.connect_kiln`. The linked doc target is also wrong under the current Project/Kiln/Workspace vocabulary._
- [-] **Agent Config** `P0` — default agent, temperature, max_tokens, thinking budget · [[Help/Config/agents]] · `crucible-core` (config)
  - **Gets you:** thinking budget (see **Extended Thinking**), temperature and max_tokens all reach the model for internal agents. "Default agent" still does not, and for ACP agents temperature remains a cache nothing forwards to the process.
  - **Proof:** `crates/crucible-daemon/src/provider/genai_handle.rs`::generation_settings_reach_the_outgoing_chat_options; see **Lua `temperature` / `max_tokens`**. _Still `[-]` for `acp.default_agent`, which is read in exactly two places, both of which only print it. Worth remembering why the other three survived as inert for so long: `test_temperature_round_trip`, `test_max_tokens_round_trip` and `all_config_knobs_round_trip_over_the_wire` set-then-get through RPC by name, and passed the whole time the knob reached nothing._
- [x] **Project Registry** `P0` — directories register as projects (`project.register` / `list` / `get` / `unregister`); `.crucible/project.toml` carries attached kilns and `[security]` policy · `crucible-daemon`
  - **Gets you:** sessions, the web root dropdown, and search containment are all project-scoped — a grep root outside every registered project is rejected, and a subdirectory of one is allowed. This is the third of the three load-bearing terms (Project / Kiln / Workspace) and had no entry at all before.
  - **Proof:** `crates/crucible-daemon/src/server/grep.rs`::subdirectory_of_registered_project_is_allowed and `::root_outside_every_registered_root_is_rejected` (the RPC response body differs on registration); `crates/crucible-web/src/routes/session.rs`::set_workspace_attaches_project_dir
- [x] **CLI Commands** `P0` — 28 top-level subcommands, notably `chat`, `session`, `search`, `process`, `stats`, `config`, `daemon`, `web`, `mcp`, `acp`, `lua`, `plugin`, `skills`, `proposals`, `tasks`, `workflow`, `agents`, `models`, `storage`, `init`, `setup` · [[Help/CLI/Index]] · `crucible-cli`
  - **Gets you:** all of them dispatch and produce output. The count is no longer carried in the description because it drifts every release — the previous "16 command modules" was stale by 1.75×.
  - **Proof:** `crates/crucible-cli/tests/snapshots/cli_help_snapshot_tests__top_level_help.snap` locks the rendered command list
- [x] **Init Command** `P0` — `cru init` project initialization with path validation · `crucible-cli`
  - **Gets you:** `.crucible/` created with `config.toml`, `sessions/` and `plugins/`, refusing hard-blocked paths. Caveat: the config it writes is stale and partly invalid — it emits `[storage] backend = "sqlite"` (a key `StorageConfig` does not have, silently dropped) and a `[chat]` table with `provider =`, which the loader **hard-rejects**. This is survivable today only because the kiln-local `.crucible/config.toml` appears never to be loaded, which is itself worth confirming.
  - **Proof:** `crates/crucible-cli/src/commands/init.rs`:621 asserts `.crucible/config.toml` exists on disk after a run; validation gate at `:41`
- [-] **Setup Wizard** `P0` — first-run wizard on `cru chat` when no kiln exists · `crucible-cli`
  - **Gets you:** a first-run prompt, but not the one described. Three of the four clauses are wrong: it is a `dialoguer` stdio wizard, **not** an Oil TUI one; it triggers on bare `cru`, **not** on `cru chat`; and it keys off a missing *config file*, not a missing kiln. `cru chat` does prompt for a kiln separately, and that path has no provider detection or model selection — it hardcodes `("ollama", "llama3.2")`.
  - **Proof:** _none for the described flow — `run_setup_wizard` has exactly one call site, inside the bare-`cru` arm of `main.rs`; the Chat arm never calls it. `wizard.rs` uses `println!` + `dialoguer` throughout. `is_first_run` and `generate_initial_config` have unit tests; nothing asserts the trigger or the flow. Blast is high because this is literally the first-session path — this may be better rewritten than rebuilt, but as written it promises a TUI wizard that does not exist._
- [x] **Kiln Discovery** `P0` — git-like upward `.crucible/` search · `crucible-cli`
  - **Gets you:** running `cru` inside a directory under a kiln finds that kiln by walking up, with `$CRUCIBLE_KILN` as a fallback. The **effective** order is config file → ancestor walk → `$CRUCIBLE_KILN`, not the "CLI flag → ancestor walk → env var → global config" this entry used to claim: both production callers pass no CLI flag or global path, there is no top-level `--kiln-path`, and `ensure_valid_kiln` returns early on a configured kiln *before* calling `discover_kiln`.
  - **Proof:** `crates/crucible-cli/src/kiln_discover.rs`:22-67 with 10 unit tests at `:122-249` covering the flag, ancestor walk, env var, global config, temp-dir exclusion and not-found; reaching the user via `chat_preflight.rs:32-42`
- [x] **Kiln Path Validation** `P0` — hard blocks (root, nested kiln), strong warnings (git repo, source project, home dir, tmp), mild warnings (cloud sync) · `crucible-cli`
  - **Gets you:** `cru init` refuses `/` and nested kilns outright and prompts for confirmation on the warning cases. Real hole: the `cru chat` auto-create path creates a kiln at a user-supplied path **without calling the validator**, so the first-run route bypasses every hard block and warning — a user can be walked into creating a kiln at `~` or inside a git repo by the flow that is supposed to guard it. "Shared validation layer" also overstates it: there is exactly one caller.
  - **Proof:** `crates/crucible-cli/src/kiln_validate.rs`:109-260 implements all four severities with 14 unit tests; consumed at `commands/init.rs:41` where severity drives the prompt
- [x] **CLI Help & Discoverability** `P0` — `long_about` with examples on every top-level subcommand; `infer_subcommands` so unique prefixes resolve; clap's "did you mean" on typos · `crucible-cli`
  - **Gets you:** `cru --help` lists every subcommand with example-bearing help, `cru con show` resolves to `cru config show`, and typos get suggestions. All 28 top-level variants carry `long_about`; nested subcommands were not counted, so read the claim as "every top-level subcommand".
  - **Proof:** three committed insta snapshots (`cli_help_snapshot_tests__top_level_help.snap`, `__chat_subcommand_help.snap`, `__session_subcommand_help.snap`); `#[command(infer_subcommands = true)]` at `crates/crucible-cli/src/cli/mod.rs`:61
- [x] **Getting Started** `P0` — installation and first steps · [[Guides/Getting Started]] · [[Guides/Your First Kiln]]
  - **Gets you:** both guides present in the docs kiln.
  - **Proof:** `docs/Guides/Getting Started.md` and `docs/Guides/Your First Kiln.md`. Content currency was not assessed — and given the Storage Config, Setup Wizard and `[embedding]` findings above, these two pages are the most likely place removed knobs are still being taught.
- [x] **Platform & Provider Guides** `P0` — Windows setup, GitHub Copilot, OpenRouter, Z.AI, Basic Commands, Session Search · [[Guides/Windows Setup]] · [[Guides/GitHub Copilot Setup]]
  - **Gets you:** six guides, not the two this entry used to name — `docs/Guides/` also ships `OpenRouter Setup.md`, `Z.AI Setup.md`, `Basic Commands.md` and `Session Search.md`.
  - **Proof:** all six files present in the docs kiln.
- [x] **Plugin Loading Errors** `P0` — `:plugins` shows load status; failures surface as toast notifications with error details · `crucible-lua`, `crucible-cli`
  - **Gets you:** each plugin printed with a state glyph and, on failure, `✗ name v0.1.0 (error: <message>)`, plus a notification you actually see. (It does not show provenance — see **Provenance in `:plugins`**.)
  - **Proof:** render at `chat_app/command_handling.rs:860-891` including `entry.error`; the notification reaches the frame via `tests/user_story_tests/notification_tests.rs::latest_toast_shows_in_status_bar`, and a failed plugin raises one per `chat_app/tests.rs::plugins_discovered_raises_notification_for_failed_plugin`

## Web & Desktop

> Builds on the HTTP gateway. The web UI is a **mostly-thin client to the daemon** — but no longer purely one: `crucible-web` holds SSRF validation on session endpoints, path-traversal containment on kiln reads, note-upsert gating, layout persistence to its own file, PTY lifecycle, and the wire-shape normalization without which no permission prompt renders at all. Serve over Tailscale/Cloudflare Tunnel for self-hosted remote access; PWA for mobile without app-store friction.
>
> **Design principles**, as they stand after the 2026-07-30 reconciliation:
> 1. **Gateway-centric** — daemon owns state; the web is a thin view layer for everything it can be.
> 2. **Multi-session supervision** — the Inbox, attention markers and notification routing exist so work in a non-focused tab is not silently lost. (The original "agent inbox is the landing page" is not what shipped: a fresh load lands on an empty center pane by design, and Inbox is one panel among thirteen.)
> 3. **Knowledge graph is the differentiator** — visual graph exploration no competitor has in-browser.
> 4. **Web extensibility is TypeScript, not Lua** — reversed from the original principle by the 2026-07-26 asymmetric-extensibility decision: Lua covers behavior and the TUI, TypeScript covers the web, and the shared contract is data rather than widgets. Panels register in `web/src/lib/register-panels.tsx`.
> 5. **Good API docs** — an interactive playground is unstarted; the honest version is tracked as `OpenAPI Spec` below. The de-facto contract lives in `crates/crucible-web/tests/route_contract_tests/` (15 files across ~22 route modules).

### Foundation UI

- [x] **Static File Serving** `P1` — Axum serves the SolidJS bundle (PWA manifest + service worker) from `dist/` via rust-embed; static routes are public · `crucible-web`
  - **Gets you:** browsing to the server returns the app, and unknown non-asset paths fall back to `index.html` so deep links work. Two nuances: rust-embed only applies in **release** builds — debug builds and any `--static-dir` override serve from the filesystem — and `--static-dir` resolves **relative to cwd**, a foot-gun that has previously caused a dev server to silently serve a stale bundle.
  - **Proof:** the live tier boots a real `cru web` and loads the app in a browser (`web/e2e/live/hero.live.spec.ts`); wiring at `crates/crucible-web/src/server.rs`:137, embed and SPA fallback at `src/assets.rs:19,72-77`
- [x] **Web Chat UI** `P1` — SolidJS chat: streaming, markdown, tool cards, permission modals · `crucible-web`
  - **Gets you:** you type in the browser and see streamed tokens, a collapsible thinking block, tool-call cards with args and results, token counts, and permission modals you can answer — plus subagent and delegation cards, a precognition badge, per-segment message bubbles, a context-usage meter, a mode control and export.
  - **Proof:** `web/e2e/stories/chat-stream.story.spec.ts::streams tokens, thinking, tool card, then completes (visual)`; `web/e2e/chat-happy-path.spec.ts::sends a message and displays streamed response`, `::cancel button stops streaming`; markdown in `web/src/lib/__tests__/markdown.test.ts::renders crucible wikilink anchors`, `::sanitizes unsafe script tags`; tool cards in `web/src/components/__tests__/ToolCard.test.tsx`. Tool cards are TypeScript — the old "Lua can extend tool card definitions" clause is dropped.
- [x] **Flexible Panel System** `P1` — dockable, splittable, poppable panel layout with server-side persistence · `crucible-web`
  - **Gets you:** you drag tabs between left/right/bottom/center zones, split and nest center panes, pop a tab out to a floating window and dock it back, and the layout survives a reload — persisted **server-side** through `GET/POST/DELETE /api/layout` to a file on disk, so it follows you across browsers. The model is a binary split tree (layout v5), not a fixed 4-edge dock.
  - **Proof:** `web/e2e/cross-zone-dnd.spec.ts::drag center tab to left edge panel`, `::dragging last tab out of edge panel auto-collapses it`; `web/e2e/windowing-comprehensive.spec.ts::supports nested splits by splitting a child pane after initial split`, `::split ratio persists after dragging splitter away from default`; `web/e2e/windowing-regression.spec.ts::pop-out MOVES the tabs to a floating window (no mirrored group)`, `::dock button moves a floating window back into the layout`; plus `web/src/lib/__tests__/layout-serializer.property.test.ts`
- [x] **Navigator / Scope Switching** `P1` — ribbon-hosted panel with a projects/kilns/sessions swapper and an inline search takeover · `crucible-web`
  - **Gets you:** you switch between projects, kilns and sessions and start a new session from one place. **There is no breadcrumb and no header bar** — the shell controls live in the ribbons, and a test asserts the header bar's absence, so re-adding one would now break it.
  - **Proof:** `web/e2e/session-management.spec.ts::displays sessions in the session panel`, `::selects a session when clicked`; `web/e2e/new-session-chat-tab.spec.ts::clicking New Session opens a draft; first message creates the chat tab in the right pane`; `web/e2e/windowing-regression.spec.ts::Ribbons carry the shell controls — no header bar`; filtering in `web/e2e/session-filter.spec.ts`
- [x] **File Tree** `P1` — accessible tree over project files and kiln notes with drag-to-move and context actions · `crucible-web`
  - **Gets you:** you browse a single tree under a root dropdown that selects among registered projects and kilns, open files into center tabs, drag files to move them (backed by the link-preserving `fs.move`), and get rename/delete/new-note context actions with extension-based icons. Move is deliberately drag-only, not a menu entry.
  - **Proof:** `web/src/components/files/__tests__/FileTreeView.test.tsx::renders a role=tree with treeitems carrying aria-level`, `::renders top-level nodes folders-first`, `::marks the open note with aria-current="page"`, `::has no menu entry for move (drag-and-drop owns it)`; `web/e2e/file-tab.spec.ts::opening a file creates a file tab in the center pane`, `::file tab deduplication`; routes proven by `crates/crucible-web/tests/route_contract_tests/fs.rs`::fs_list_returns_200_with_array, `::fs_move_returns_200_with_moved_true`, `::fs_trash_returns_200_with_trash_path`. (The tree is hand-rolled, not Ark UI, and there are no longer separate Workspace/Kiln sections.)
- [x] **CodeMirror 6 Editor** `P1` — multi-file tabs with dirty indicator, language detection, save via API · `crucible-web`
  - **Gets you:** you open a file in a tab, edit it, see a dirty indicator, save with Ctrl-S or `:w`, and the bytes land on disk. Also shipping in this surface: **vim keybindings**, wikilink autocompletion and hover previews, frontmatter card rendering, and table auto-formatting.
  - **Proof:** `web/e2e/live/hero.live.spec.ts:101-123` — real daemon, real file: asserts the editor content, edits it, watches the save indicator appear and clear, then `readFileSync(notePath)` confirms the write; `web/src/components/editor/__tests__/CodeMirrorEditor.test.tsx:::w and :wq route to onSave (same path as Ctrl-S, clears the dirty chip)`; `web/e2e/stories/editor-tabs.story.spec.ts`, `editor-roundtrip.story.spec.ts`; `web/src/lib/__tests__/language-detection.test.ts`
- [x] **Live Preview & Reading View** `P1` — Obsidian-style live preview as the markdown editing default, with a matched reading view · `crucible-web`
  - **Gets you:** callouts, fenced-code highlighting, task-list checkboxes, tables, images, sanitized embedded HTML, a Properties card for YAML *and* TOML frontmatter, and wikilink following with hover previews that tear off into floating editor windows.
  - **Proof:** `web/e2e/live-preview-blocks.spec.ts`; `web/src/lib/__tests__/markdown.test.ts`, `callouts.test.ts`, `frontmatter.test.ts`, `backlink-context.test.ts`; `web/e2e/stories/wikilink-hover.story.spec.ts`, `wikilink-navigation.story.spec.ts`
- [x] **Inline Diff Review of Agent Edits** `P1` — an agent's proposed edit stages as an inline merge diff in the editor with per-hunk accept/reject · `crucible-web`
  - **Gets you:** an Edit/Write/MultiEdit tool card gains "Open in editor"; the file opens with the change overlaid green/red and per-hunk Accept/Reject in the gutter. Accept the hunks you want, save, and the accepted text is what reaches disk. A proposal is never staged over a buffer with unsaved edits. This is distinct from the permission modal's *preview* — it is hunk-level review after the fact, in the buffer.
  - **Proof:** `web/e2e/inline-diff-editor.spec.ts::accepting a hunk and saving writes the accepted content`, `::rejecting a hunk keeps the original text for that hunk`, `::the ToolCard "Open in editor" button opens the real file with the diff`, `::will not stage a proposal over unsaved edits`, `::clears the pending review once it is saved`; store `web/src/stores/__tests__/pendingDiffStore.test.ts`
- [x] **Model Picker** `P1` — Cursor-style dropdown below the textarea; switch model during a conversation · `crucible-web`
  - **Gets you:** the picker opens, shows available models, and switching one calls the API mid-conversation.
  - **Proof:** `web/e2e/model-switching.spec.ts::model picker opens and shows available models`, `::switching model calls the API`; routes `route_contract_tests/sessions.rs::list_models_returns_200_with_models_array`, `::switch_model_returns_200`
- [x] **Session Auto-Naming** `P1` — an untitled session renames itself once the daemon titles it · `crucible-daemon`, `crucible-web`
  - **Gets you:** a fallback label until the title arrives, then a rename that propagates everywhere in the UI. Titling is **daemon-side** (topic auto-title on the first completed turn); the browser only renders the `title_changed` SSE event — so this is not SolidJS work as the entry used to claim.
  - **Proof:** `web/e2e/title-generation.spec.ts::title_changed SSE event renames the session across the UI`, `::untitled sessions keep the fallback label until the daemon titles them`; wire event at `crates/crucible-web/src/events.rs`:399-401
- [x] **Agent Inbox** `P1` — one panel listing every session waiting on you, answerable in place · `crucible-web`
  - **Gets you:** pending permissions answered without switching tabs, recent sessions by activity, an archived section, and an all-clear state when nothing is pending. It is **one panel among thirteen**, not the landing page.
  - **Proof:** `web/src/components/__tests__/InboxPanel.test.tsx::renders a pending permission answerable in place`, `::responds via the API and broadcasts resolution on Allow`, `::shows all-clear when nothing is pending`; registered at `web/src/lib/register-panels.tsx:26`
- [x] **Permission Approval UI** `P1` — approve or deny from the browser with a diff preview and scope choice · `crucible-web`
  - **Gets you:** the agent asks, the browser shows a modal with the old-vs-new diff for a write, and Allow/Deny with a scope choice posts back and clears it. Queued requests open one at a time, and the same requests are answerable from the Inbox.
  - **Proof:** `web/e2e/stories/permission.story.spec.ts::allow-once posts the correct respond payload and clears the modal`, `::choosing a scope (session) is sent in the payload`, `::deny posts allowed:false and clears the modal`, `::queued permissions open sequentially`; wire normalization — which is what makes the modal render at all — pinned by `crates/crucible-web/src/events.rs`::real_bash_permission_flattens_to_frontend_shape, `::real_write_permission_joins_segments_into_one_path_token`, `::real_tool_permission_carries_name_and_args`. Diff formatting is TypeScript (`DiffViewer.tsx`, `MultiEditDiff.tsx`), not Lua.
- [x] **Session Management** `P1` — list, create, open, resume, archive, unarchive, delete and export sessions from the browser · `crucible-web`
  - **Gets you:** all of the above plus session scope changes (attach/detach kiln, set workspace) and draft-surface lazy creation including ACP agents and kiln-less sessions. Note the lifecycle redesign **removed** the End and "Continue as new session" buttons — tests now assert their absence.
  - **Proof:** `web/e2e/session-lifecycle.spec.ts::resumes a session and loads history`, `::archives a session via hover action button`, `::deletes a session via hover action button with confirmation`, `::sessions persist across page refresh`, `::no End button visible for active session`; `route_contract_tests/sessions.rs::archive_session_returns_200_with_archived_true`, `::delete_session_returns_200_with_deleted_field`, `::export_session_returns_markdown_content_type`, `::connect_kiln_returns_scope_shape`
- [x] **Web Terminal** `P1` — xterm.js over a WebSocket-attached PTY, in a bottom panel · `crucible-web`
  - **Gets you:** a real interactive shell in the browser — WebGL renderer, vector-drawn powerline/box glyphs, `COLORTERM=truecolor`, configurable font that applies live, starting in the server's launch directory, reconnecting after a socket drop, with a concurrency cap on upgrades. Remote access is opt-in and **fail-closed**: `cru web --remote-shell` / `[server] remote_shell = true` serves it to authenticated non-localhost clients, and without an API key the opt-in is ignored. This is the only WebSocket surface in the web app and the most security-load-bearing one.
  - **Proof:** `web/e2e/stories/terminal.story.spec.ts::renders a PTY prompt, echoes input, and reconnects after a drop`; gating asserted by `crates/crucible-web/src/middleware/auth.rs`::remote_shell_gate_allows_non_localhost_when_active, `::remote_shell_active_is_fail_closed_without_an_api_key`; server at `src/routes/terminal.rs`
- [x] **Composer Autocompletion** `P1` — `/` command and `[[` wikilink completion in the chat composer · `crucible-web`
  - **Gets you:** typing `/` opens a popup of the daemon's real command set, narrowing as you type and inserting the command (leaving a space for commands taking an argument); typing `[[` completes to a closed wikilink. A path separator is not treated as a trigger, and a failed command fetch keeps the popup closed and retries on the next keystroke.
  - **Proof:** `web/src/hooks/__tests__/useAutocomplete.test.ts::opens the popup when the user types "/"`, `::serves commands from the server, not a hardcoded list`, `::does not treat a path separator as a command trigger`, `::opens on "[[" and completes to a closed wikilink`; the command set is real and dispatchable per `route_contract_tests/commands.rs::commands_endpoint_lists_the_command_set`, `::every_advertised_command_is_dispatchable`
- [x] **Command Palette & Note Switcher** `P1` — Ctrl+P for panels/files/actions, Ctrl+O for a note quick switcher · `crucible-web`
  - **Gets you:** the primary navigation surface of the web app. Every registered panel has an "Open …" command, so a closed graph/terminal/backlinks window can always be brought back; the note switcher is recency-sorted with path subtitles and scored subsequence fuzzy matching, and `[[` and `>` cross between the two mid-typing.
  - **Proof:** `web/src/components/__tests__/CommandPalette.test.tsx`; `web/src/lib/__tests__/keyboard-shortcuts.test.ts`, `fuzzy.test.ts`, `panel-registry.test.ts`
- [x] **Notifications & Attention Routing** `P1` — background sessions that need you raise a toast and a per-session attention marker · `crucible-web`
  - **Gets you:** a pending permission, error or completion in a non-focused tab is not silently lost — this is the multi-session glue the Inbox alone does not provide.
  - **Proof:** `web/src/components/__tests__/NotificationCenter.test.tsx`; `web/src/stores/__tests__/notificationStore.test.ts`, `attentionStore.test.ts`; consumed by the Inbox's WAITING/STREAMING session states
- [x] **Precognition, Subagent & Delegation Surfaces** `P1` — the browser shows which notes precognition injected, and renders subagents and delegations as their own cards · `crucible-web`
  - **Gets you:** the stated differentiator is visible rather than invisible — you can see what context the agent was given, and watch spawned subagents and cross-agent delegations with completion/failure state.
  - **Proof:** `web/src/components/__tests__/PrecognitionBadge.test.tsx`; wire mapping `crates/crucible-web/src/events.rs`::precognition_complete_translates_to_precognition_result (which also guards the daemon→web event rename); components `SubagentCard.tsx`, `DelegationCard.tsx`
- [x] **Voice Input / Transcription** `P1` — record audio from the composer and get it transcribed into the message box · `crucible-web`
  - **Gets you:** dictation into the chat composer, configurable from Settings.
  - **Proof:** `web/src/components/MicButton.test.tsx`; `web/src/hooks/useMediaRecorder.test.ts`; `web/src/lib/transcription.test.ts`; `web/src/contexts/WhisperContext.test.tsx`
- [x] **Design Tokens & Style Gate** `P1` — a test fails the build if a component reaches for a raw palette class instead of a semantic token · `crucible-web`
  - **Gets you:** a UI that reads as one system in light and dark, with structural surfaces animating through shared motion primitives. It is also why the visual playwright baselines can be as tight as 0.3–4% diff ratios.
  - **Proof:** `web/src/components/__tests__/style-consistency.test.ts::no component uses an off-token Tailwind palette class`, `::surfaces use tokens, not white-alpha (bg/text/border)`, `::edge panels slide via one rAF-driven progress (frame + translate locked)`, `::command palette pops in over a fading overlay`
- [x] **PWA Support** `P1` — manifest + service worker; installable from the browser, mobile access without an app store · `crucible-web`
  - **Gets you:** an installable app whose update surfaces as a prompt rather than reloading mid-turn (`registerType: 'prompt'`, `skipWaiting: false`), with the service worker forbidden from intercepting `/api/*` including the SSE stream. Debug builds ship a self-destructing SW.
  - **Proof:** built artifacts on disk (`web/dist/manifest.webmanifest`, `dist/sw.js`, `dist/workbox-*.js`, three icon sizes), served with content types pinned by `crates/crucible-web/src/assets.rs`::pwa_assets_resolve_to_correct_mime_types; manifest fields at `web/vite.config.ts:26-46`; SW registered at `web/src/index.tsx:40-41`. No test asserts an actual HTTP 200 for `/manifest.webmanifest` — the one gap if this entry is ever challenged.

### Knowledge & Search (web)

- [x] **Knowledge Graph Visualization** `P2` — interactive force-directed wikilink graph · `crucible-web`
  - **Gets you:** the Graph panel renders an Obsidian-style map of the kiln — drag nodes, hover to highlight, filter by query, toggle orphans and tags — and clicking a node opens the note. Phantom nodes are synthesized for unresolved links.
  - **Proof:** `crates/crucible-web/tests/route_contract_tests/kilns.rs`::kiln_graph_returns_200_with_notes_and_links (the data, over HTTP); graph construction `web/src/lib/graph/__tests__/build.test.ts::builds note nodes and resolved edges with degrees`, `::synthesizes phantom nodes for unresolved links`, `::query filters notes and prunes edges to hidden notes`, `::drops orphan notes when showOrphans is off` (12 tests). Honest caveat: the drawing is a `<canvas>` redrawn in a rAF loop, so nothing asserts rendered pixels — evidence is strong on the data and the route, weaker on the paint.
- [x] **Note Reading & Backlinks** `P2` — read a note with frontmatter, working wikilinks, hover previews and a backlinks panel · `crucible-web`
  - **Gets you:** the note's frontmatter as a card, its wikilinks as working anchors with hover previews, and linked *and* unlinked backlinks in a side panel where one click wraps a mention as a wikilink in the open buffer. The old entry's "custom columns/sort/filters" framing does **not** ship — that idea now lives entirely in `Structured Data Views`.
  - **Proof:** `web/src/components/__tests__/BacklinksPanel.test.tsx::renders linked and unlinked mentions for the focused note`, `::clicking a linked mention dispatches the global open-file event`, `::one-click Link wraps the mention as a wikilink in the open buffer`; over HTTP `route_contract_tests/kilns.rs::backlinks_returns_linked_and_filtered_unlinked`; `web/e2e/stories/wikilink-hover.story.spec.ts`
- [x] **Search UI** `P2` — one query fanned out over notes, files and sessions, with a Text|Semantic toggle · `crucible-web`
  - **Gets you:** results from all three sources in one panel, a toggle that swaps note results between literal grep and vector similarity, matched spans highlighted, and scoping that drops the other sections. Reachable from the Navigator's search takeover and the command palette. **Property search does not ship** — there is no frontmatter-field query path in the UI.
  - **Proof:** `web/src/components/__tests__/SearchPanel.test.tsx::fans a query out to notes (glob *.md), files, and sessions`, `::highlights the matched span and opens a hit`, `::scoping to Sessions drops the notes/files sections`; routes `route_contract_tests/kilns.rs::search_semantic_returns_200_with_results`, `::search_vectors_returns_200_with_results`, `::search_semantic_blank_query_returns_empty`, and `src/routes/search.rs::grep_search_maps_hits_to_wire_shape`
- [ ] **Structured Data Views** `P3` — Obsidian Bases-style tables and kanban from frontmatter. If built it is a TypeScript panel over the storage query layer, not a Lua extension · `crucible-web`, `crucible-daemon` (storage)

### Artifacts & Rich Content

- [x] **Rich Content Renderers** `P2` — mermaid, KaTeX, syntax highlighting, callouts and copy buttons · `crucible-web`
  - **Gets you:** a ```` ```mermaid ```` fence renders as a diagram (falling back to source on failure and never invoked for a plain code block), `$…$`/`$$…$$` render as KaTeX surviving DOMPurify without treating currency as math, code blocks get shiki highlighting and a copy button, and Obsidian callouts render — in chat, in the reading view, and live in the editor. Mermaid and shiki are lazily imported to keep the bundle down; math and diagram rendering have user-facing toggles in Settings.
  - **Proof:** `web/src/lib/__tests__/mermaid-pipeline.test.ts::replaces a ```mermaid fence with a rendered diagram`, `::falls back to the source when a diagram fails to render`, `::never invokes mermaid for a plain code block`; `math.test.ts::renders inline $…$ as KaTeX HTML`, `::survives DOMPurify (spans/classes/styles kept)`, `::does NOT treat currency as math`; `web/e2e/live-preview-blocks.spec.ts`; `shiki.test.ts`; `markdown.test.ts::wraps code blocks with a copy button`; `callouts.test.ts`. These are direct TypeScript modules, not plugins registering content-type handlers.
- [x] **Skills Panel** `P2` — browse, search and read agent skills in the browser · `crucible-web`
  - **Gets you:** skills grouped by scope, a debounced search that switches to the search endpoint, a drawer with the skill body, a shadow badge when a skill is shadowed, and copy-to-clipboard for its `/name` invocation. **Enable/disable of an individual skill is not exposed.**
  - **Proof:** `web/src/components/__tests__/SkillsPanel.test.tsx::groups skills by scope and renders rows`, `::opens the drawer and loads detail on row click`, `::copy-invocation writes /<name> to clipboard`, `::debounces typed query and switches to search endpoint`, `::shows the shadow badge when shadowed_count > 0`; routes `route_contract_tests/skills.rs::list_skills_returns_200_with_skills_array`, `::get_skill_returns_200_with_body`, `::search_skills_returns_200_with_matches`
- [x] **Plugin Manager Panel** `P2` — browse plugins with load state and last error; install, reload and remove them · `crucible-web`
  - **Gets you:** rows from the rich plugin info response, a `last_error` row for a broken plugin and none for a healthy one, an install modal taking a git URL, and an uninstall confirmation that passes the purge flag through.
  - **Proof:** `web/src/components/__tests__/PluginPanel.test.tsx::renders rows from the rich plugin_info response`, `::shows last_error for a broken plugin, and no error row for a healthy one`, `::install modal calls installPlugin with the entered URL`, `::uninstall confirmation passes purge flag through to removePlugin`; routes `route_contract_tests/plugins.rs::list_plugins_returns_rich_plugin_info`, `::install_plugin_returns_200_with_outcome`, `::remove_plugin_with_purge_query_returns_200`
- [ ] **Agent Artifacts** `P2` — promote a response fragment into its own persistent side panel. Much of the *motivation* is already covered — tool output renders as cards, agent file edits render as reviewable inline diffs, and mermaid/LaTeX/code render inline — so what remains genuinely missing is **extraction** · `crucible-web`

### Configuration & System (web)

- [x] **Settings Panel** `P2` — session model config plus editor, font, API access and transcription settings · `crucible-web`
  - **Gets you:** thinking budget, temperature, max tokens, precognition and results-per-query for the active session (gated on there being one); editor behaviour (vim keys, autosave, line width, math/diagram rendering, floating save button); fonts for UI, code and terminal; API access for a remote device; and transcription. Two distinct stores: **session** settings go to the daemon over RPC, **UI** settings are client-side. Neither touches `config.toml`.
  - **Proof:** `crates/crucible-web/tests/route_contract_tests/session_config.rs`::set_thinking_budget_returns_200, `::get_thinking_budget_returns_200_with_budget`, `::set_temperature_returns_200`, `::get_temperature_returns_200_with_value`; client persistence `web/src/lib/settings.test.ts`, `web/src/contexts/SettingsContext.test.tsx`. Note the temperature and max-tokens knobs here write daemon fields that no LLM request carries — see **Agent Config**.
- [x] **System Info** `P2` — daemon health, kilns, MCP status and plugin status · `crucible-web`
  - **Gets you:** `/health` and `/ready` over HTTP, the kiln list, MCP server status and plugin health (state, source, last error) in the Settings panel. Narrowed from the original entry: **embedding stats do not ship**, and there is no single "System Info" panel — the pieces are scattered across Settings sections and bare HTTP endpoints.
  - **Proof:** `crates/crucible-web/tests/route_contract_tests/health.rs`::health_check_returns_200_with_json, `::ready_check_returns_200_with_ready_status`; plugin health rendered per `web/src/components/__tests__/PluginPanel.test.tsx::shows last_error for a broken plugin, and no error row for a healthy one`; MCP status route `src/routes/mcp.rs:7` surfaced at `SettingsPanel.tsx:498`
- [-] **SCM Operations (web)** `P2` — list git branches, add a worktree and clone a repo from the browser · `crucible-web`
  - **Gets you:** the routes exist (`GET /api/scm/branches`, `POST /api/scm/worktree`, `POST /api/scm/clone`) over the tested daemon SCM layer.
  - **Proof:** _none at the web layer — there is no route-contract test file for `scm.rs` and no vitest or playwright spec asserting the rendered result; only `web/src/lib/__tests__/git-url.test.ts` covers URL parsing. That missing contract test is the cheapest RED in this section. The daemon side is proven — see **Git / SCM Project Integration**._
- [ ] **Embedding & Index Stats** `P2` — surface embedding counts and index health in the browser. Split out of the old System Info entry, which claimed it; no route or panel exposes it today · `crucible-web`
- [ ] **Config Editor** `P2` — schema-driven form for `config.toml`. `GET /api/config` is read-only (no PUT/POST) and there is no schema generation from config types. The Settings Panel above is what exists where this was pencilled in; editing `config.toml` from the browser remains the genuine gap · `crucible-web`, `crucible-core` (config)
- [ ] **OpenAPI Spec** `P2` — machine-readable API spec generated from routes; ship the spec and let users bring Swagger UI / curl / httpie. No `utoipa`, `aide`, `okapi` or `schemars` anywhere in `crates/`. A generator would have a well-tested surface to describe · `crucible-web`
- [ ] **Log Viewer** `P2` — real-time daemon log streaming. No log route exists; the three SSE streams are chat events, filesystem events and per-command shell output. The web terminal lets a user tail logs manually, which lowers the urgency · `crucible-web`

### Canvas & Desktop

- [x] **Canvas** `P3` — infinite spatial workspace over a JSON Canvas file · `crucible-web`, `crucible-core` (canvas)
  - **Gets you:** the Canvas panel opens a `.canvas` file as an infinite surface — text, file, link and group nodes, labelled edges, marquee select, drag and resize with corner and edge handles, connector drawing with snap-to-target, and live sandboxed web embeds on link cards — saved back to disk. Out-of-root references are quarantined without revealing the path, and embeds are sandboxed into an opaque origin. Two things the old entry implied that do **not** ship: agent *sessions* on the canvas, and labelled canvas edges surfacing as graph relations.
  - **Proof:** `web/src/components/__tests__/CanvasPanel.test.tsx::renders every node type as DOM and edges as SVG`, `::draws edge labels`, `::offers four corner handles and four edge connectors on a selected card`, `::highlights the node a connection would land on, and snaps the line to it`, `::grows the marquee rectangle as the pointer moves`, `::flushes a pending save when the panel unmounts`, `::quarantines a rejected node without revealing its path`, `::sandboxes the embed into an opaque origin so it cannot reach the session` (22 tests); doc round-trip `web/src/lib/__tests__/canvas-doc.test.ts`, `canvas-viewport.test.ts`; route `crates/crucible-web/src/routes/canvas.rs`:32
- [ ] **Workflow Visual Editor** `P3` — DAG editor for workflow markup. The shipped canvas surface is a plausible substrate if this is ever built · `crucible-web` · depends: [[#Workflow Automation]]
- [ ] **Tauri Desktop** `P3` — native desktop app wrapping the web UI; menu-bar agent status, system notifications. Its stated blocker (a working web chat UI) is now satisfied, so this is genuine open work rather than blocked work; PWA install covers part of the motivation · `crucible-web`

### Superseded Web Plans

> Kept as entries rather than deleted so the next reader does not re-propose them. Both were foundations for a rendering approach the web UI did not take.

- [-] **Oil Node Serialization** `P1` — `impl Serialize for Node`, Oil nodes to JSON for browser rendering · `crucible-oil` (behind the `serde` feature)
  - **Gets you:** nothing — a serializer with no reader.
  - **Proof:** _none — `crucible-oil`'s `default = []` and both dependents opt out (`crucible-cli` and `crucible-lua` are `default-features = false` and neither lists `serde`), so the `Serialize` impls are never compiled in the workspace. `crucible-web` does not depend on `crucible-oil` at all, and there is no consumer of the JSON on the other side. It existed solely to feed the SolidJS Oil Renderer below._
- [ ] **SolidJS Oil Renderer** `P1` — an `<OilNode>` component tree for the browser. **Out of scope, not pending**: the web UI went a different route entirely — native SolidJS components composed through a panel registry, with markdown/mermaid/KaTeX/shiki rendering in TypeScript. Nothing depends on it, contradicting this entry's original "everything else depends on it" · `crucible-web`, `crucible-oil`
- [ ] **Plugin Panel Hosting** `P1` — iframe sandbox + message-passing protocol for Lua-registered web panels. **Superseded, not pending**: per the 2026-07-26 asymmetric-extensibility decision, web panels are TypeScript, registered in `web/src/lib/register-panels.tsx` (13 panels today). `PluginPanel.tsx` is a plugin *manager*, not a host · `crucible-web`, `crucible-lua`

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
| `crucible-desktop` (GPUI) | 2024-12-13 | Cut — using Tauri + web instead |
| `add-desktop-ui` OpenSpec | 2024-12-13 | Archived — GPUI approach abandoned |
| `add-meta-systems` | — | Too ambitious (365 tasks), overlaps with the focused Lua approach |
| `add-advanced-tool-architecture` | — | Overlaps with the working MCP bridge |
| `add-quick-prompt-features` | — | Nice UX, not core — revisit later |
| `refactor-clustering-plugins` | — | Nice feature, not core |
| Ratatui TUI | 2025-01-17 | Removed — migrated to the oil-only TUI |
| SurrealDB Backend | 2026-02-23 | Removed — SQLite is the default and only backend; the crate was deleted (17K LOC). Document Clustering and the K-Means stub went with it |
| Team Patterns (supervisor / router / broadcast) | 2026-05-12 | Removed (~1984 LOC) — the hardcoded types each picked one delegation shape and shut out variants. Delegation *primitives* are infrastructure, delegation *patterns* are user code: they are now 5–20 line Lua recipes over `cru.sessions.*`, documented in [[Help/Delegation Patterns]] |
| Grammar + Lua Integration (`cru.grammar` GBNF bindings) | 2026-05-12 | Removed — shipped briefly in Wave 2 with no working backend. Revisit if llama-cpp is integrated |
| `session-digest` Runtime Plugin | 2026-05-12 | Removed — LLM-judged dedupe risked wrong merges and kiln pollution, and users preferred prompted refinement over automatic digests. Replaced by the propose-only Reflection Pass |
| hermit plugin | 2026-03-29 | Removed — capabilities belong in chat/messaging integration plugins |
| Deferred message queue (TUI) | 2026-06-10 | Removed — typing during a turn preserves the draft instead; Ctrl+Enter cancels |
| User-facing `temperature` / `max_tokens` knobs | 2026-06-28 | Removed from `:set` and `cru set` — the genai turn path never applied them. The programmatic fields remain but are still inert; see **Agent Config** |

## Links

- [[Meta/Analysis/Systems]] — System architecture and boundaries
- [[Meta/Product Decision Log]] — Dated product decisions, with reversals annotated
- [[Meta/Analysis/Fennel for Plugins]] — Whether to promote Fennel for plugin authoring
- [[Meta/TUI User Stories]] — TUI requirements
- [[Meta/Web User Stories]] — Web requirements
- [[Meta/Plugin User Stories]] — Plugin requirements
