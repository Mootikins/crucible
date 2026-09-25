---
title: TUI User Stories
description: Complete user stories for every implemented TUI feature, with acceptance criteria and test-tier mapping
tags: [meta, ux, tui, user-stories, testing]
updated: 2026-09-24
---

# TUI User Stories

Complete user-facing stories for the Crucible chat TUI, covering every implemented feature in [[Meta/Product]]. Each story carries a **test tier** telling you where its automated verification lives (or belongs):

| Tier | Mechanism | Determinism |
|------|-----------|-------------|
| **T1 unit** | `OilChatApp` state tests / handler tests | full |
| **T2 frame** | `Vt100TestRuntime` headless frame capture + insta snapshots (plain or ANSI-styled), scrollback asserts | full |
| **T3 replay** | JSONL `SessionEvent` fixtures (`assets/fixtures/`) pumped through the app — event-stream verification | full |
| **T4 pty** | `TuiTestSession` (expectrl + vt100) against the real binary | real terminal; reserve for what T1–T3 can't see |
| **T5 video** | VHS tapes (`assets/*.tape`, `just demo`) | demo artifact, not CI |

Multi-frame stories are verified as **frame sequences**: capture a `Vt100TestRuntime` frame after each scripted step and snapshot the sequence, not just the end state.

## Coverage governance

The tiers only help if scenarios move between them deliberately. These rules decide when.

**Promote a GAP / manual scenario to an automated tier when all three hold:**
- **Deterministic** — the outcome is fixed given the inputs (no real wall-clock, network, or model sampling). Drive spinner/streaming convergence with `StoryRuntime::settle`/`expect_frame`, never a sleep.
- **Acceptance-criteria-shaped** — the story has a concrete "then" you can assert against a rendered frame or emitted `Action`, not a vibe.
- **Broke once** — a real regression slipped through here. A scenario that has never failed is a lower priority than one that has; promotion buys the most where it already cost us.

Until a GAP meets all three, leave it marked GAP with a one-line note on what blocks automation.

**Graduate a mock tier (T1–T3) to the live tier (T4 pty) when the assertion depends on state that crosses the daemon boundary** — real session persistence, `--resume` hydration, cross-console visibility, or anything the in-process `OilChatApp` fakes via injected events. T1–T3 verify the view and the event-stream contract; only T4 proves the daemon actually holds the state. If a story's "then" is "the same state is visible after restart / from another console", it belongs in T4.

**Every new feature adds a story and a tier before it merges.** A behavior with no US entry and no tier is untested by definition. Add the story (with acceptance criteria), pick the lowest tier that can prove it, and — if it crosses the daemon boundary — add the T4 leg too.

---

## 1. Modes & Input

### US-101: Cycle chat modes
**As a user**, I press BackTab to cycle Normal → Plan → Auto, so I control how much autonomy the agent has.
**Acceptance:** mode indicator updates in status bar; Plan mode blocks write tools (daemon-synced); mode survives across turns; `/mode`, `/plan`, `/auto`, `/normal` set modes directly.
**Tests:** T1 (mode state + daemon sync msg), T2 (status bar per mode), T4 (BackTab keycode).

### US-102: Input modes
**As a user**, I start a line with `:` for REPL commands or `!` for shell so the same input box drives everything; plain text is chat.
**Acceptance:** prompt glyph changes (`>` / `:` / `!`); Esc returns to normal input; mode-specific autocomplete engages.
**Tests:** T1, T2 (input_area snapshots exist — extend for `!`).

### US-103: Slash commands
**As a user**, I type `/` commands and they execute locally or route onward.
**Acceptance:** the built-ins are `/mode` (cycle), `/default`, `/undo [N]`, and `/help`; `/clear` is the user's clear through the `session.clear` RPC (the web `/clear` and the palette "Clear Chat" send the same RPC), and its divider names no plugin; every declared mode is its own command (`/plan`, `/auto`, `/normal`, a Lua-declared `/review`); plugin-registered commands run via `plugin.run_command`; other unknown `/` input is forwarded to the agent as a plain chat message, with no local effect and no suggestion (levenshtein typo suggestions exist for `:` commands only); `/help` lists the REPL commands plus registered slash commands.
**Tests:** T1 dispatch matrix in `chat_app/command_handling.rs` (quit/clear/messages/model/config/export/undo, unknown-command suggestion), T2 (help render).

### US-104: REPL `:set` runtime config
**As a user**, I use vim-style `:set key=value` (and `?`, `??`, `&`, `^`) to change runtime config (context strategy/budget/window, autocompact threshold, precognition, perm.*).
**Acceptance:** each documented key round-trips (set → query shows new value); invalid keys error with a message; session-scoped keys sync to the daemon; `:set key?` shows value, `&` resets; app-config keys (the ones the classifier does not own) are written to the daemon store and read back from it, so `:set key?`, `:set key??`, `:lua cru.config.get(key)` and plugins give one answer; every spelling of one key reaches one store; `&` and `^` on an app-config key call the daemon's `config.reset` and `config.pop`, which drop layers from the store and merge again — `&` drops the layer `:set` writes, `^` drops the highest layer holding the leaf and shows the one under it, and the answer names the layers that went; neither edits a file, and both refuse the keys that name where the daemon acts; a refused write warns and records nothing. Every advertised key must be REAL: `:set syntax_theme=<name>` switches diff/code-block highlighting live (validated against the loaded theme set plus `derived`, which follows the colorscheme; seeded from `cli.highlighting` at startup). Renamed from `:set theme=` when the UI gained its own colorscheme; the inert `verbose` knob was removed 2026-07-10.
**Tests:** T1 per-key dispatch matrix in `chat_app/command_handling.rs` (test-case over every session-scoped key: daemon-sync emission, invalid-value warnings, query round-trip, reset), plus two completeness walks over `SHORTCUTS` × `SetCommand::iter()` — every declared target answers locally under every spelling, and no spelling of an app-config key touches the local store. T2 (`:set` result notification render; the daemon's query answer reaches the transcript; a `&`/`^` answer reaches it naming the dropped layer and the revealed origin).

### US-108: `:lua` escape hatch
**As a power user**, `:lua <expr>` (or `:= <expr>`) evaluates a Lua expression in the daemon's plugin runtime and shows the result as a system message, so I can poke config/state beyond the `:set` knobs without leaving the chat. The default command line never evals implicitly — unknown `:` input still gets command suggestions, not execution.
**Acceptance:** `:lua 1+1` renders `2` in the viewport; runtime errors surface as a `lua:`-prefixed warning notification; `:lua` with no body shows usage; works identically via `:=`.
**Tests:** T1 dispatch + result/error rendering in `chat_app/command_handling.rs` (US-108 block); daemon `lua.eval` RPC has its own coverage.

### US-105: Double Ctrl+C quit
**As a user**, one Ctrl+C clears input or warns; a second within 300ms quits, so I can't quit by accident.
**Acceptance:** first press with text clears it; with empty input shows the quit warning in the status bar; second within window exits cleanly.
**Tests:** T1 (timing state machine), T2 (ctrl-C notification snapshots exist), T4 (real SIGINT path).

### US-106: Bracketed paste
**As a user**, pasting multi-line text inserts it as one input block without executing lines.
**Acceptance:** paste of N lines yields one buffer with N lines; no premature send; paste inside `:`/`!` modes stays literal.
**Tests:** T1 multi-line substrate (`InputBuffer::insert_str`, Ctrl+J) in `user_story_tests/paste_tests.rs`; **GAP: bracketed-paste event plumbing not wired** (`CtEvent::Paste` → `Event::Tick` in `convert_event`), documented there. T4 (real bracketed-paste sequences).

### US-107: Typing while the agent streams
**As a user**, I can keep typing while the agent streams; my draft is never lost, and Ctrl+Enter cancels the turn so I can send it.
**Acceptance:** input stays responsive during streaming; there is **no queue-while-streaming** (the deferred message queue was removed) — Enter on a chat draft keeps the draft and shows a "Turn in progress — Esc cancels, then Enter to send" toast; `:` and `/` commands still execute on Enter mid-stream; Ctrl+Enter cancels the stream and preserves the draft so it can be sent once the turn stops.
**Tests:** T1 `enter_while_streaming_preserves_typed_input` + `ctrl_enter_while_streaming_cancels_and_preserves_input` in `chat_app/input_handling.rs`.

## 2. Streaming & Display

### US-201: Token streaming with graduation
**As a user**, responses stream token-by-token; completed content graduates to terminal scrollback so the live viewport stays small.
**Acceptance:** deltas render incrementally; graduated content appears exactly once in scrollback (no duplication, no spinner remnants); spacing consistent across graduation boundary.
**Tests:** T2/T3 (fixture_replay invariants exist), T4 (real scrollback).

### US-202: Cancel generation
**As a user**, Esc or Ctrl+C during streaming cancels the turn locally and server-side.
**Acceptance:** stream stops; partial content preserved and graduated; status returns to idle; daemon receives cancel.
**Tests:** T1 (cancel msg emission), T3 (cancel mid-fixture).

### US-203: Thinking display
**As a user**, I see thinking blocks stream with a token count, toggle them with Ctrl+T, and set `:set thinking`.
**Acceptance:** collapsed/expanded states render correctly; toggle applies retroactively to visible blocks; thinking never leaks into graduated content when hidden. An agent that reasons, speaks, then reasons again within one turn — any ACP-delegated agent, and the internal agent between tool batches — has every one of those thoughts rendered, in wire order. A provider's end-of-stream replay of a reasoning block it already streamed is painted once, not twice; suppression is content-based and turn-scoped, and never fires on a run short enough to be a single ordinary thought (a thought that merely repeats its predecessor is new content, not a replay).
**Tests:** T2 (collapsed/expanded snapshots exist), T1 (toggle state); T1 `session_event_stream_tests` (interleaving, replay drop, minimum-run floor, per-turn reset, per-fixture rendered-thought counts); T2 `user_story_tests/acp_parity_tests::{a_delegated_agent_second_thought_reaches_the_screen, an_end_of_stream_reasoning_replay_is_not_painted_twice}`.

### US-204: Markdown rendering
**As a user**, responses render styled markdown: bold/italic, inline code, highlighted code blocks, lists, tables.
**Acceptance:** code blocks are single nodes (no inter-line gaps); syntax colors match theme; wide tables truncate gracefully at narrow widths.
**Tests:** T2 styled snapshots at widths 50/80/120 (partially exists), markdown_fuzz_tests.

### US-205: Context usage + statusline
**As a user**, the status bar shows mode, model, token usage (used/total), and cache hit rate; Lua `cru.statusline.setup()` reorders it.
**Acceptance:** usage updates after each `message_complete`; Lua config drives layout with builtin fallback; overflow degrades gracefully at narrow widths (badges stay intact, model/toast spans elide with `…`, sections never overlap).
**Tests:** T1 (statusline config), T2 (status_bar width snapshots at 40/50/80/120; narrow-width graceful degradation shipped 2026-07-10 via oil row flex-shrink + `no_shrink` badges).

### US-206: A reply the model did not finish
**As a user**, a reply the provider cut off, or one the model declined to give, says so under the text instead of reading as a finished answer.
**Acceptance:** `message_complete` carrying `stop_reason = max_tokens` draws a system line naming the output limit; `refusal` draws one naming the refusal; `end_turn` and a payload with no `stop_reason` draw nothing extra; the model's own partial text stays on screen beside the note.
**The daemon words the note.** `StopReason::user_notice` is the only wording. The TUI calls it. The browser cannot, so the web layer puts the answer on the frame as `stop_notice` and the page draws the string it received — the page holds no table of its own, because the two wordings drifted when it did.
**Tests:** T1 `chat_runner/tests/translate.rs::{a_truncated_reply_draws_a_note_after_the_bubble, a_finished_reply_mints_no_notice}`; T2 `user_story_tests/stop_reason_tests`. Web: `crucible-web`'s `the_projection_carries_the_daemon_wording_for_every_reason` and `the_frontend_words_no_stop_reason_notice`, plus `ChatContext.test.tsx` "a reply the provider cut off".

### US-207: A turn ends once, and a plugin can ask for the next one
**As a user**, the console stops showing a turn as running the moment the daemon says the turn is over, whatever ended it; and a turn a plugin asked for reads as its own turn under the reply.
**Acceptance:** `turn_finished` ends the turn for every status (`completed`, `cancelled`, `handler_cancelled`, `timed_out`, `failed`), so a cancel from ANOTHER client stops this console's spinner; a `failed` turn and a `handler_cancelled` turn (for example the loop guard of an ACP turn) show the `error` they carry as a warning; the opening event of a turn a `turn:complete` handler asked for names its plugin and renders the full text as `↻ <plugin>` in a system row, with its own reply under it. The label stays after a resume, also for a log with the old flat origin. A permission prompt in a plugin turn names the plugin. A person's message that a plugin relayed (`send_and_collect`, such as Discord) stays a user message and shows "via <plugin>".
**Tests:** T2 `user_story_tests/turn_end_tests.rs::{every_turn_finished_status_ends_the_turn, a_failed_turn_shows_its_error, a_handler_cancelled_turn_shows_its_reason, the_turn_a_handler_asks_for_renders_as_its_own_turn}` and `user_story_tests/clear_tests.rs::{a_plugin_turn_and_its_prompt_name_the_plugin, a_resumed_plugin_turn_keeps_its_label, a_relayed_message_names_its_relay}`. Web: `InteractionHandlers.test.tsx` "names the plugin whose turn asks".

### US-208: I choose how a plugin's turns ask for permission
**As a user**, I set a plugin's turns to `inherit`, `ask` or `stop` in this session from the TUI, as the web settings do, and I read the value that the daemon holds.
**Acceptance:** `:set plugin_approval.<plugin>=inherit|ask|stop` calls the session's `set_plugin_approval` on the daemon handle; `:set plugin_approval.<plugin>?` shows the value that the handle reads, which a resume loads from the daemon; the TUI keeps no copy of the value; another value warns and calls nothing. The web settings select reads the value again on `plugin_approval_changed`.
**Tests:** T1 `chat_runner/tests/knob_rpc.rs::{interactive_set_knob_reaches_matching_rpc::plugin_approval, plugin_approval_is_set_and_read_through_the_handle, an_unknown_plugin_approval_is_refused}`. T2 `user_story_tests/clear_tests.rs::an_unknown_plugin_approval_warns`. Web: `routes/__tests__/session.test.ts` "invalidates the plugin approvals when the daemon changes one". Daemon: `rpc_integration/models.rs::plugin_approval_round_trips_over_socket_and_on_attach`. Web: `chatEventReducer.test.ts` "turn_finished: a failed turn shows its error" and "a turn that a handler cancelled shows its reason". Daemon: `agent_manager/tests/turn_finished.rs`.

## 3. Tools, Subagents & MCP

### US-301: Tool call lifecycle display
**As a user**, running tools show a spinner and smart summary (file/line/match counts); completion shows check or X with collapsible output.
**Acceptance:** parallel calls tracked independently by call_id; tail display capped (50 lines); >10KB output spills to file with a pointer; MCP prefix stripped from names.
**Tests:** T2 (pending/complete snapshots exist), T3 (parallel tool fixture — extend).

### US-302: Subagent display
**As a user**, spawned subagents show status (spawned/completed/failed), elapsed time, and truncated prompt.
**Acceptance:** concurrent subagents render as separate rows; failure state distinct; completion collapses to a summary.
**Tests:** T2 spawn/complete/fail + concurrent-row + delegation-target render in `user_story_tests/subagent_mcp_tests.rs`; T3 (delegation-demo fixture).

### US-303: MCP server status
**As a user**, `:mcp` lists configured MCP servers with live connection status that updates at runtime.
**Acceptance:** connected/disconnected/connecting states visible; status refresh on `McpStatusLoaded`.
**Tests:** T1 (status msg handling), T2 `:mcp` list render with connection status in `user_story_tests/subagent_mcp_tests.rs`.

### US-306: The card and the prompt draw the render of the daemon
**As a user**, a tool card shows the canonical tool name, the one line that the daemon rendered for the call, and each field of the render, for my own tools, for ACP agents and for a kind that a plugin adds. When the call finishes, the card shows the summary of the result render; a call that went to the background shows it on its finish row. The permission prompt shows the render line and the render fields (or the arguments when the render has no fields), and also the agent, the tool name on the wire and the layer that asked.
**Acceptance:** the TUI never rebuilds a call from its arguments and never takes a summary or a label from the tool name; a `tool_call_update` and a result render replace the render; a recording from before the render shows its old primary argument as the line, and its late diffs on the card (the migration in `session_events::migrate`, which the daemon's history loader also runs for the web); a recording with no primary shows no line.
**Tests:** T1 `chat_runner/tests/translate.rs` (the update carries the new render; the result carries the result render) and `components/tool_render_tests.rs::{the_card_shows_the_render_line_on_one_row, the_card_shows_the_summary_of_the_result_render, the_card_draws_the_fields_of_the_render}`; T2 `user_story_tests/tool_render_tests.rs`, including `a_tool_card_draws_the_fields_and_the_result_summary`, `the_permission_modal_draws_the_render_line_and_fields`, `a_background_call_keeps_its_result_summary` and `an_old_transcript_shows_its_lines_and_diffs`; T1 `containers.rs::a_split_tool_keeps_the_result_render_on_its_finish_row`.

### US-307: Delegated (ACP) presentation parity
**As a user**, when I delegate to an external agent (`cru chat -a claude`), the turn looks exactly like one the internal agent ran — except that tool cards for tools *that agent* ran say which agent ran them, so I can tell them apart from tools Crucible ran under my own permission gate.
**Acceptance:** the card badges `[acp:<agent>]`; Core/Crucible tools stay unbadged (provenance is implicit); sessions recorded before the agent name was on the wire replay with a bare `[acp]`. **The badge is the only sanctioned frame difference for the behaviours a fixture pair covers** — today an `edit_file` turn whose diff arrives late and a `read_file` turn whose result is multi-line, each recorded from both agents and asserted byte-identical once the badge is removed. A diff the agent attaches *after* the card was announced (ACP's `tool_call_update`, which the internal agent never sends because it synthesizes diffs up front) lands on that card and is painted; a delegated tool's result collapses into the card header exactly as the internal tool's does, because the summary table keys on the leading Title-Cased run of the humanized name, which both spellings share. That table also answers to the titles a *real* agent sends, not only to titles that look like internal tool names: `Find` (Claude Code's glob) is listed alongside `Glob`, and `Read tools/hello.rn` keys as `Read`. The titles under test are read out of `assets/fixtures/acp-demo.jsonl` at test time, so a case cannot drift into asserting a spelling no agent produces.
**Not claimed.** The equality is per-pair, not a general theorem: only tools with a pair are pinned. The summary table is a best-effort synonym list, not a closed set — ACP carries no tool name on the wire, so a title Crucible has never seen (`ToolSearch`, `Get Kiln Info`, a bare shell command line) reaches no arm and renders its result in full. `Grep`'s real Claude Code title is unknown, because the one recording available contains no grep call. Two differences are deliberate and out of scope here — the permission modal shows a `ToolKind`-derived name because ACP carries no tool name on the wire (pinned separately), and the tool card carries an `[acp:<agent>]` provenance badge by design. **The statusline is no longer a divergence** — a delegated agent's `usage_update` now resolves the context limit via `ContextLimitSource::Agent`, proved end to end over a spawned mock in `acp_integration/context_usage.rs`. What the *fixture pair* cannot see is that fix: both fixtures omit the limit events, so both render the no-data path and this pair is silent on it. The daemon's `description` asymmetry costs no pixels only because `session_event_to_chat_msgs` drops descriptions on *every* path; it is left in the fixtures so wiring one arm through would fail the test.
**Tests:** T1 (`parse_tool_source` arms, `badge_label`, `source_badge_visibility` table; per-spelling summary-table tests in `components/tool_render.rs`, including `recorded_claude_code_titles_reach_their_summary_arm` and its no-widening counterweight, both driven from `acp-demo.jsonl` via `helpers::recorded_claude_code_title`), T2 badge-in-frame + daemon-event-mapping legs, both fixture-pair frame-equality legs (`acp_and_internal_agents_render_identical_frames`, `acp_and_internal_read_turns_render_identical_frames` plus its `both_read_cards_collapse_their_result_to_a_summary` counterweight), the delegated-glob leg (`a_delegated_glob_collapses_its_file_list_like_the_internal_one`) and its `Terminal` counter-case, the rendered late-diff leg, and the delegated-turn snapshot — all in `user_story_tests/acp_parity_tests.rs`; T3 invariant sweeps over all four fixtures in `fixture_replay_tests.rs` / `inter_frame_invariant_tests.rs`. Producer side pinned daemon-side in `agent_manager/tests/messaging.rs`, and the fixtures themselves are re-derived from the daemon on every run by `agent_manager/tests/parity_capture.rs`.

## 4. Interaction Modals

### US-401: Permission modal full flow
**As a user**, when the agent needs permission I get a modal with the tool, args, and a togglable diff; y approves, n denies, a allowlists — and the tool then runs or errors accordingly.
**Acceptance:** queued permissions auto-open in order; `h` toggles diff (the on-screen hint reads `press h to expand/collapse diff` — `d` is not bound, see `interaction_modal/perm.rs:68`); decision reaches the daemon; deny yields an error tool result and the turn continues; allowlist persists project-scoped, and the grant it saves is the command the modal displayed — a wider grant needs the user to `Tab` and add a `*`. A call that no grant can name (a call that nothing names, a command that Crucible cannot read, an edit with no path) gets no Allowlist option, and `a` does nothing, because a click would save nothing. A prompt that the daemon ends (a cancelled turn, or an answer from another client) leaves the modal and the queue (`interaction_completed`). An ACP call that a layer allowed with no prompt gets the `[auto]` marker from a late `tool_call_update`, and a refused ACP call shows the reason of the gate as its error.
**Tests:** T2 (modal render + diff), full approve/deny→tool-result flow + queued-ordering in `user_story_tests/permission_tests.rs`, permission_invariant_tests.

### US-402: Ask modal
**As a user**, agent questions render as single-select, multi-select (Space), or free-text-"other" modals I drive with the keyboard.
**Acceptance:** all 7 InteractionRequest variants render; Esc cancels with a cancelled response; selection posts the right payload.
**Tests:** T1 (all variants — exists), T2 (snapshots), T3 (interaction fixture — extend).

### US-405: Recover a waiting prompt on attach
**As a user**, when I open a session whose agent is waiting for an answer, the prompt appears before I type a new turn, even if it was raised more than five minutes ago.
**Acceptance:** session attach subscribes, fetches pending interactions, routes this session's requests to the modal channel, and ignores a duplicate live event from the subscribe race. A prompt stays available until answered or cancelled.
**Tests:** T1 (`rpc_client::agent::tests::pending_snapshot_reaches_the_tui_interaction_channel_once` and daemon prompt wait tests); T2 (`user_story_tests/permission_tests.rs::permission_modal_opens_and_shows_command` renders and answers the recovered channel's event shape).

### US-403: Diff preview
**As a user**, file-op permissions show syntax-highlighted line/word diffs, side-by-side when wide, unified when narrow.
**Acceptance:** create/delete/edit render distinctly; oversize falls back with a truncation footer; `:set perm.show_diff` controls initial visibility.
**Tests:** T2 (11 diff snapshots exist), T1 (perm.* settings — GAP for dispatch).

### US-404: Full command visibility in permission prompts
**As a user**, the permission prompt shows the *entire* bash command or tool arguments — wrapped across lines, never truncated — so I know exactly what I'm approving. `:set perm.full_commands=false` restores the compact one-line (ellipsized) view.
**Acceptance:** long bash commands wrap to the panel width with no content loss; tool args show every key and full string values (no `...`/3-key cap); compact mode ellipsizes to one line; knob defaults on and round-trips through `:set`.
**Tests:** T1 (`:set perm.full_commands` round-trip in `command_handling.rs`), T2 (wrapped-bash snapshot + full/compact render assertions in `interaction_modal/tests/perm.rs`).

## 5. Autocomplete & Palette

### US-501: Autocomplete triggers
**As a user**, typing `@` (files), `[[` (notes), `/` (commands), `:` (REPL), `:model `, `:set ` (and args) pops contextual completions I cycle with Tab/arrows and accept with Enter.
**Acceptance:** all 9 trigger kinds produce candidates; filtering narrows as I type; Esc dismisses without inserting; accepted completion replaces the token correctly.
**Tests:** T1 candidate-generation matrix inline in `chat_app/autocomplete.rs` (every trigger kind, filter narrowing, dismiss, token replacement), T2 (popup snapshots), T4 (one Tab-cycle smoke).

### US-504: Minimal (pmenu) popups for inline completions
**As a user**, inline completions (`@` files, `[[` notes) show a compact nvim-pmenu-style box anchored at the word I'm completing, sized to its content — while command completions (`/`, `:`) keep the full-width strip that extends the prompt. `:set completion_style=auto|panel|minimal` overrides the split.
**Acceptance:** `auto` (default) anchors inline popups at the trigger column with labels aligned to the completed word; `panel` forces the strip everywhere; `minimal` forces anchored boxes everywhere; minimal popups float on the themed `popup_bg`/`popup_selected_bg` surface, panel popups share the prompt's mode bg.
**Tests:** T1 knob classification in `commands/set.rs`; T1 anchored rendering in `tests/popup_tests.rs` (`popup_anchored_renders_content_width_at_anchor_column`); T2 composited-frame behavior in `tests/popup_tests.rs::completion_style_behavior` (default minimal + panel override).

### US-505: The completion popup never moves the prompt
**As a user**, a completion popup draws over the rows above the prompt — the transcript, or blank space — and the prompt stays exactly where it was, whether the popup is open, closed, or taller than the conversation so far.
**Acceptance:** the frame reserves the tallest popup plus the prompt region every frame, so opening a popup changes no row position and closing one gives the covered transcript rows back; the reserve is measured against the popup that actually renders (`popup_max_visible` + `popup_offset_from_bottom`), never a constant; a reserve larger than the screen is clamped to it; under an open **panel** popup the prompt's top edge is a filled row rather than a half block, so the two read as one surface with no half-lit seam between them (an anchored/minimal popup floats and leaves the edge alone).
**Tests:** T1 reserve/pad/clamp in `crucible-oil/src/output.rs` (`a_frame_shorter_than_the_reserve_is_padded_up_to_it`, `an_overlay_taller_than_the_content_does_not_grow_a_reserved_frame`, `the_reserve_never_exceeds_the_screen`); T1 filled edge in `components/input_component.rs`; T2 frame behaviour in `user_story_tests/completion_frame_tests.rs` — RED-verify by passing `0` to `set_min_viewport_rows`, which is the shape the bug had.

### US-502: Command palette
**As a user**, F1 opens a palette of commands; typing filters; Enter executes the selection.
**Acceptance:** F1 again / Esc closes; selecting a `/` or `:` entry executes it. **GAP:** the palette's entry list is a hardcoded 4-item stub (`semantic_search`, `create_note`, `/mode`, `/help`), not the full slash + REPL registry; selecting a tool entry only sets status text, it does not run the tool. (`:pick commands` lists the real registry.)
**Tests:** T1 (open/filter/execute), T2 (palette snapshot).

### US-503: Model switching with lazy fetch
**As a user**, `:model` fetches models on first access (NotLoaded → Loading → Loaded), lets me pick with autocomplete, and switches mid-session preserving history.
**Acceptance:** loading state visible; picker filters; switch confirmed in statusline; history intact after switch.
**Tests:** T1 (state machine — exists), T2, T4 (12 ignored PTY model tests — promote key ones).

## 6. Shell

### US-601: Shell modal execution
**As a user**, `!command` runs in a full-screen modal I can scroll (j/k/g/G/PgUp/PgDn); `i` inserts the output into chat input; Esc closes.
**Acceptance:** exit code shown; long output scrolls; insert puts stdout at cursor; modal restores the chat view intact underneath.
**Tests:** T1+T2 end-to-end via the `ShellModal` component in `user_story_tests/shell_tests.rs` (spawn → exit code → output → scroll); header/status + auto-follow unit-tested in `components/shell_modal.rs`. The former `i`-insert-loses-stdout bug is fixed — `insert_key_inserts_output_in_one_step` pins close-and-insert as one step. T4 (one real-command smoke).

### US-602: Shell history
**As a user**, `!` recalls my last 100 shell commands.
**Acceptance:** Up/Down recall submitted `!` commands through the general `InputBuffer` history, alongside other submissions. Dedicated shell-only recall is not implemented; its unconsumed 100-entry store has been removed.
**Tests:** T1 storage + cap/eviction in `chat_app/tests.rs`.

## 7. Notifications

### US-701: Toast lifecycle
**As a user**, transient events show toasts that auto-dismiss after 3s; severities are visually distinct.
**Acceptance:** multiple toasts stack in arrival order; expiry removes exactly the aged toast; badge count matches drawer contents.
**Tests:** T2 stacking + latest-toast + drawer + dismissal in `user_story_tests/notification_tests.rs`; 3s expiry as an `#[ignore]` slow test (timeout not injectable headlessly). T2 (badge snapshots exist).

### US-702: Messages drawer
**As a user**, `:messages` toggles a full history of notifications so nothing transient is lost.
**Acceptance:** drawer lists all session notifications with severity; toggle preserves scroll; dismiss clears the badge.
**Tests:** T2 drawer flow (`:messages` lists all, dismiss) in `user_story_tests/notification_tests.rs`.

## 8. Scrollback & Layout

### US-801: Review history without losing my place
**As a user**, I review history without losing my place. In the default full-screen mode (US-804), the TUI owns the scroll. In the inline mode (`--inline`), I review graduated history through the terminal's own scrollback, and in-app scroll regions (the shell modal) hold position while new content arrives.
**Acceptance:** in scroll regions, manual scroll disables auto-follow and jump-to-bottom resumes following. The full-screen mode meets this for the transcript, with a label for the rows below (US-804). **GAP:** the inline chat viewport binds no scroll keys and captures no mouse — PageUp/PageDn and wheel scrolling there are the terminal's, not the app's; it has no in-app "new content" indicator.
**Tests:** T1 scroll state via the shell modal's scroll region (auto-follow off on manual scroll, jump-to-top/bottom) in `user_story_tests/scroll_tests.rs` + `components/shell_modal.rs`. The **main chat viewport graduates to the terminal's own scrollback (no app-held scroll state)**, so its scroll/auto-follow is T4-only. T4 (real terminal scroll region).

### US-803: The session opens saying what it is attached to
**As a user**, the first thing in the transcript names the kilns this session draws knowledge from, so a wrong or empty attachment is visible before I spend a turn on it.
**Acceptance:** the banner names every attached kiln and its path, with the names aligned; one kiln reads "1 kiln attached"; no kiln says so in as many words rather than printing an empty list; the daemon owns the set (`kiln.list`) and a listing failure drops the banner instead of failing the session; a replay gets no banner, because it attaches nothing.
**Tests:** T1 banner text (plural, singular, empty) in `chat_app/tests.rs`; T2 the banner in a rendered frame in `user_story_tests/completion_frame_tests.rs`.

### US-804: Read, select and copy in the full-screen mode
**As a user**, I start `cru chat` and get the chat on the alternate screen. The TUI scrolls, selects and copies, and I can select text over SSH and inside Zellij or tmux.
**Acceptance:**
- The full-screen mode is the default for every entry that opens the chat TUI (`cru`, `cru chat`, `cru session resume`, `cru chat --replay`). `--inline`, or `cli.screen = "inline"` in the config, keeps the chat on the main screen. The setting is display state of the client, not a session knob.
- The inline mode stays where the full screen does not fit: the setup prompts (first-run wizard, kiln prompt, `cru init`, `cru auth`) print on the main screen before the TUI starts, and a stdout that is not a terminal gets the inline mode. A one-shot or piped query draws no TUI.
- PageUp, PageDown and the mouse wheel scroll the transcript. A scroll up stops the follow, and a label tells how many rows are below. A scroll to the bottom starts the follow again. Streamed rows do not move a reader who scrolled up. A resize keeps the reader at the same text.
- A drag selects text, a double click selects a word, and a triple click selects a logical line. The highlight is on the rows under the pointer, also in the demo's shell under its tab row.
- The highlight covers only the text. It does not cover a margin, a bullet, a prompt mark or the padding after the text. A press or a release in such a gutter moves to the nearest text.
- The button release copies the text as the source has it. A wrap becomes the text that the wrap removed, not a line break. The copy goes through OSC 52, then the native clipboard (not over SSH), then tmux.
- F2 turns mouse capture off and on. F3 prints the finished transcript into the terminal scrollback. The exit prints the rest, and nothing prints twice.
- The mode reads the same kept rows as the native view, so the two modes draw the same transcript. A tool card draws the render that the daemon sent (US-306).
- A width change lays out only the nodes on the screen, so it takes a few milliseconds at 5,000 rows. The other nodes get an estimated height until the TUI lays them out between frames. Until then, the label and the scroll limits use the estimates. A scroll, a copy and the dump lay out the nodes that they reach first, so the text on the screen and the copied text are exact.
- The web has no full-screen mode. This mode is a choice of terminal presentation, and a browser owns its own scroll, selection and copy.

**Tests:** T1 selection, copy, gutter and snap in `fullscreen/selection.rs` and `fullscreen/tests.rs`; scroll, follow, reflow and dump in `fullscreen/tests.rs`; the pane mouse rows in `fullscreen/shell.rs`; the kept rows in `fullscreen/transcript.rs`. T2 `user_story_tests/fullscreen_tests.rs` writes each frame through the row diff into vt100, then reads the text, the inverse cells of the highlight, the held top row, the tool card that draws the render table (US-306) and a plugin surface that is the whole frame (US-908). T1 the screen choice (flag, config, stdout) in `commands/chat/tests.rs`. T4 `tui_e2e_tests/screen.rs` starts the real binary: a plain `cru chat` enters the alternate screen with mouse reports and leaves it on exit; `--inline` and `cli.screen = "inline"` stay on the main screen; the first-run wizard prompts on the main screen. **GAP:** a person must check the copy in a real terminal, flicker in Zellij and the width of a ZWJ sequence (manual steps in the prototype report). There is no search, and a width change lays out the whole transcript again.

### US-802: Stable rendering across widths
**As a user**, the TUI renders correctly at narrow (50), normal (80), and wide (120) widths without flicker or duplication.
**Acceptance:** no torn frames (synchronized updates); no duplicate graduation; spacing via gap() consistent.
**Tests:** T2 width-matrix snapshots (exist), inter_frame_invariant_tests, property tests.

## 9. Session & Recovery

### US-901: Export session
**As a user**, `:export <path>` writes the session as markdown with frontmatter, thinking blocks, and tool calls; `~` expands.
**Acceptance:** file matches observe renderer output; errors (bad dir) surface as toasts.
**Tests:** T1 (GAP), golden-file compare.

### US-902: Undo a turn
**As a user**, `/undo` (and `/undo 3`) reverts the last agent turn(s) — conversation and file changes — so mistakes are cheap.
**Acceptance:** viewport reflects removed turns; workspace files restored (git and non-git); `/undo` with nothing to undo says so; undo depth reported; on a session that an external ACP agent runs, the daemon refuses and the warning shows its reason.
**Tests:** T1 `/undo`/`:undo [N]` dispatch in `chat_app/command_handling.rs`; T2/T3 UndoComplete toast, viewport truncation on daemon revert, the refusal warning, and a frame-sequence snapshot in `user_story_tests/undo_tests.rs` (fixture `undo_flow.jsonl`). T4 optional.

### US-903: Resume with full history
**As a user**, resuming a session rehydrates the viewport from daemon events with correct rendering of every historical element.
**Acceptance:** history renders identically to live (tools, thinking, modals resolved); statusline reflects restored config (model, budget).
**Tests:** T3 (hydration fixture), T2 snapshots.

### US-904: Event-stream fidelity (replay)
**As a user/developer**, any recorded session replays deterministically (`cru chat --replay`), rendering the same frames every time.
**Acceptance:** replay never re-sends RPC; golden keyword checks pass; `--replay-speed`/`--replay-auto-exit` honored.
**Tests:** T3 (fixture_replay + replay_mode exist), T5 (VHS demos), validate-demos.sh.

### US-905: Theme the TUI from Lua
**As a user**, colours, per-surface geometry and prompt glyphs come from my `init.lua` (or a `themes/*.lua` file), and a change takes effect without restarting.
**Acceptance:** `cru.colorscheme.setup{}` reaches the renderer in split-process mode (not only `--standalone`); `cru.hl.set/link` restyle named groups, with palette references re-resolving when the palette changes; `cru.geometry.setup{}` sets popup/modal/drawer/toast/prompt geometry, and a surface the theme does not name keeps its built-in; a daemon that is unreachable leaves a correct, compiled-in-themed screen rather than a blank one; a re-sent `ui.config` replaces the active theme and repaints.
**Tests:** T1 wire round-trips + group resolution/linking/cycles in `crucible-lua` (`theme_wire.rs`, `hl.rs`, `hl_lua.rs`, `ui_geometry.rs`); T2 store swap + surface application in `tui/oil/theme/{global,groups,geometry,remote}.rs` and `components/input_area.rs`; T3 delivery over real RPC in `crucible-daemon/tests/rpc_ui_config_e2e.rs` — RED-verify that suite by unwiring the handler, not the parser.

### US-906: Build a statusline
**As a user**, I compose the screen as ordered lists of rows around the input, and show values the daemon computes (a git branch) without polling.
**Acceptance:** `sl.setup{}` fills the `top`/`prompt`/`bottom` regions, and each one **actually places** its rows in the rendered frame; a region is an ordered list, so position is the arrangement and no ordering field exists; `sl.input` is an element, so rows written above or below it render above or below it; an input outside `prompt` is dropped rather than drawing two editors, an unmentioned region keeps its built-in, and a key that is not a region warns instead of silently placing nothing; built-ins (mode, model, context, cache, notification) render every frame with no RPC; `sl.any`/`sl.when` express fallback and TUI-local conditions; `sl.expr("git")` renders nothing until a value is pushed and does not reflow the bar when it arrives; a value re-pushed unchanged causes no repaint; a value the daemon RELEASES stops rendering — a provider's `cru.statusline.clear`, a plugin marked Not Active, or the session ending — because a push carries the session's whole set and the client applies it as a replacement; control characters in a value never reach the terminal; the right-hand gutter is preserved except when an active notification claims it.
**Tests:** T1 item/element wire round-trips, region parsing and `rows_below_input` in `statusline_items.rs`/`statusline_lua.rs`, registry caps + dirty check + sanitising + per-source and per-session release in `statusline_exprs.rs`, the client-side replacement in `tui/oil/theme/exprs.rs`; T2 evaluation, combinators, gutter, narrow-width badge survival and payload-to-pixels release in `components/status_items.rs` plus the statusline snapshots in `component_isolation_tests.rs`, and end-to-end region placement in `tests/region_placement_tests.rs` — RED-verify that one by rendering only one hardcoded row, which is the shape the bug had; T3 `FileChanged` dispatch in `server/file_event_hooks.rs`.

### US-907: The transcript admits when it is incomplete
**As a user**, if the daemon could not deliver every event to this console, I am told — rather than reading a conversation with a silent hole in it.
**Acceptance:** a `stream_gap` event (the daemon's per-connection marker for a lagged broadcast cursor, `daemon/src/server/core/mod.rs`) surfaces as a warning whose leading text is the dropped count, so it survives the status bar's one truncated line; the drawer carries the whole sentence including the remedy ("reload the session"); a marker with no `dropped` field still appears, because its absence means version skew and not "no loss"; an ordinary turn produces no such warning. The marker is addressed to the wildcard session — `Lagged(n)` knows a count and not which sessions it lost — so the consumer's per-session filter must admit wildcard-addressed events, which also unblocks `ui_style_changed`'s config-level pushes.
**Tests:** T1 translation + wildcard filter (and the foreign-session negative) in `chat_runner/tests/stream_gap.rs`; T2 status-bar/drawer render and the healthy-stream negative control in `user_story_tests/stream_gap_tests.rs` — RED-verify by returning an empty `Vec` from the `stream_gap` arm, which is the shape the bug had (the unknown-event arm is a silent `trace!`). Daemon side: `server::core::tests::stream_gap_tests`.

### US-908: A plugin's surface, drawn by the TUI
**As a user**, `:surfaces` shows me a panel a plugin declared — a session list, a review queue — and it stays current while I read it, without the plugin deciding what my terminal looks like.
**Acceptance:** `:surfaces` with no argument opens the first declared surface and `:surfaces <name>` opens that one; the panel takes the whole screen through the same fullscreen path as the shell modal, so the transcript is not drawn behind it; `j`/`k`, `g`/`G` move a cursor and `esc`/`q` close it; a `surface_changed` event refreshes an **open** panel in place and keeps the cursor on the same row *id*, so a row arriving above it does not move the selection; that same event must **never open** a closed panel, because a plugin pushes rows at a moment the user did not choose and a full-screen panel over their typing is not acceptable; a row's status is a declared mark (`busy`, `blocked`, `ok`, `failed`) and the TUI picks the glyph, so the plugin never ships a character; an unknown mark draws blank rather than asserting a fault; an empty surface says so instead of drawing nothing; no surface at all reports that rather than erroring. Caps and sanitising are the daemon registry's job, not the renderer's — a row carrying ESC would otherwise reach the terminal, which parses ANSI out of plain strings.
A plugin that goes inert **withdraws** its surfaces, and the registry announces each withdrawal. Inert means an uninstall, or a reload that failed — a reload that succeeds re-declares the same surfaces and keeps the rows, so it withdraws nothing. The announcement is marked `withdrawn`, so the TUI closes the panel straight off the event and never refetches: the daemon knew the surface was gone when it dropped it, and asking spends a round trip to be told the same thing. The TUI closes it only when the panel shows that surface. The comparison is on the surface *name*, never on the title: a plugin picks a title, and two plugins can pick the same one. A refetch that finds nothing still closes the panel, as the second defence for a withdrawal this client never received; a refetch that *fails* closes nothing, because a daemon that is briefly unreachable is not a withdrawal.

`declare` announces too, so a surface declared with no rows yet is one a client can list and open. A re-declare announces only when the title, shape or session moved — a reload re-declares every surface, and announcing each one costs every client a redraw to learn nothing.
**Tests:** T1 cursor, id-anchored refresh, clamp-on-drop, glyph table and window follow in `components/surface_modal.rs`; T1 open/refresh/close, command dispatch and the no-seize guarantee in `chat_app/tests.rs` — RED-verify the last one by making the reducer open unconditionally, which is the shape the bug had and did have during development; T2 frame capture in `user_story_tests/surface_tests.rs`, including the no-seize case driven through the real wire translation. **The fullscreen switch of the inline mode is T1-only**: `Vt100TestRuntime::render_frame` calls the inline render path, while the inline runner picks `Terminal::render_fullscreen` via `has_fullscreen_modal`, so no T2 frame can observe that choice — the shell modal shares this blind spot. In the default full-screen mode, T2 `fullscreen_tests.rs` draws an open surface through `FullscreenView::frame` and finds no transcript or prompt behind it. Daemon side: registry caps, escape stripping, reload survival and the Lua boundary in `crucible-lua/src/surfaces.rs`; the wire in `event_map`/`Group::of`.
Withdrawal adds: T1 the name/title split in `components/surface_modal.rs`; T1 close-on-withdrawal plus the different-surface negative in `chat_app/tests.rs`; T1 the `Ok(None)` against `Err(_)` split in `chat_runner/tests/surface_refresh.rs`; T2 withdrawal frames in `user_story_tests/surface_tests.rs`. RED-verify the negative by closing the panel for every withdrawal: both negatives fail, and the two close tests still pass. RED-verify the `Err(_)` split by mapping a failed refetch to a withdrawal. Daemon side: `a_plugin_whose_setup_raises_ends_inert_and_the_load_reports_failure` in `daemon_plugins/tests/lifecycle.rs` proves an inert plugin's surfaces are released; RED-verify it by dropping the `release_plugin` call from `make_plugin_inert`.

The `withdrawn` flag adds: T1 `a_withdrawn_event_needs_no_refetch` plus the two refetch negatives in `chat_runner/tests/surface_refresh.rs`; T2 `a_withdrawal_off_the_wire_stops_the_drawing` in `user_story_tests/surface_tests.rs`, which starts at the wire event and supplies no fetch result. RED-verify by ignoring `withdrawn` in `system_msgs` — the T1 gets a `RefreshSurface` and the T2 keeps the rows drawn, because nothing answers the refresh; then invert the branch and the two negatives fail. `declare`'s announcement is T1 in `crucible-lua/src/surfaces.rs`: `declaring_a_surface_announces_it`, `redeclaring_with_new_metadata_announces_it`, and `redeclaring_the_same_metadata_announces_nothing` for the no-op gate. RED-verify by dropping the announce (the first two fail, the third passes), then by announcing unconditionally on the update arm (only the third fails). The browser half is Vitest in `web/src/components/__tests__/SurfacesPanel.test.tsx`, and the wire frame is `routes/surface.rs` — the contract crosses a language boundary, so each side of it is tested.

### US-909: Start a chat on an agent card

**As a user**, I can run `cru chat --card researcher` and use the same daemon-resolved
card as a one-shot or web session. The initial card label gives way to the model
and mode the daemon actually resolved. Unknown cards create no session; resuming
never replaces the existing session's agent.

**Tests:** T1 flag/conflict parsing in `cli/tests/chat.rs` and progressive setup
state in `chat_app/tests.rs`; T2 `user_story_tests/agent_card_tests.rs` draws the
resolved model through the production event translator. The process boundary is
covered by the card-backed query in `oneshot_precognition_query_e2e.rs`.

### US-910: The branch diff, drawn by the TUI
**As a user**, `:diff` shows me the changes of my branch since its merge base, one file at a time, without leaving the chat. `cru diff branch` prints the same diff in a shell.
**Acceptance:** `:diff` compares the workspace with the default branch and `:diff <base>` with that base; the root is the git top level of the working directory, and the daemon admits it or refuses it; the view takes the whole screen through the fullscreen path; `n`/`p` move between files, `PgUp`/`PgDn` page through one file, `j`/`k` scroll one row, `esc`/`q` close; the view asks for the text of a file only when the user moves to it, so a branch with many files does not send all its texts; a deleted file says `delete`, a renamed file shows `old → new`; a binary or oversize file shows a line that says so. Comments in the TUI view are out of scope.
**Tests:** T1 paging, one text request for one file and the rename's old path in `components/diff_modal.rs`; T1 `snap_deleted_file`, `snap_renamed_file` and `snap_second_page` in `components/diff_view.rs`; T1 `the_diff_command_asks_the_runner_to_fetch` and the open/next/close reducer in `chat_app/tests.rs`; T2 `a_branch_diff_reaches_the_frame` in `user_story_tests/diff_tests.rs`. The process boundary is `tests/cli_e2e_diff.rs`: a real `cru diff branch` against a real daemon and a temp repository.

### US-911: Read status items at narrow widths
**As a user**, I can see pinned approval and running state in the statusline, reach informational items that do not fit, and change a plugin's approval from its item.
**Acceptance:** `sl.items` and `sl.plugin_turns` may sit in any statusline row; a smaller priority is earlier; the engine shows one pinned item for each plugin whose turn runs (`↻ goal`, `info`) or whose approval is `ask` (`goal · ask`, `warn`) or `stop` (`goal · stop`, `danger`), from the session's approval knob, and `sl.items` does not draw it again; extra informational items become `+N`, with the stable full list available from `:status`; `:plugin-mode`, or the plugin-turn item in `:status`, opens the menu of the daemon's plugins with their three values, and the choice sets the knob through the daemon; a replacement event updates the same session's display and an empty replacement removes old items; the full-screen mode draws the status row with the same text and colors as the inline mode.
**Tests:** T1 `status_event_replaces_the_rendered_list_in_the_app` and `status_command_opens_every_item_in_a_keyboard_picker` in `chat_app/tests.rs`; T1 `the_plugin_menu_sets_the_approval_through_the_handle` and `the_plugin_turn_item_opens_the_menu_from_the_status_picker` in `chat_runner/tests/knob_rpc.rs`; T2 `status_list_keeps_pins_and_counts_hidden_information` and `plugin_turn_items_draw_only_through_their_own_item` in `components/status_items.rs`, the reviewed ANSI snapshot `statusline_status_items_overflow_narrow_40` in `component_isolation_tests.rs`, and `a_status_replacement_reaches_the_narrow_frame_and_clears` and `the_full_screen_mode_draws_the_status_items_as_the_inline_mode_does` through vt100 in `user_story_tests/status_items_tests.rs`. Daemon: `agent_manager/tests/status_items.rs` drives the item from the real knob and a real plugin turn, reads it at attach, and sends a Lua `publish` through the daemon's notifier. Web: `SessionStatusChips.test.tsx` sets the knob from the item's menu.

### US-HERO: One session, many consoles (cross-surface)
**As a user**, work I start in the terminal is fully continuable in the browser and back again — the session lives in the daemon (the "hypervisor"), the TUI and web are stateless consoles, and kiln files are a shared buffer.
**Acceptance:** a session created + advanced in `cru chat` resumes in `cru web` with turn 1 hydrated both sides; a note the terminal wrote via the shell modal opens in the web editor; the browser's edit to that note is visible from a later `cru chat --resume` via `!cat`; both consoles see the same 3-turn history and the same bytes on disk.
**Tests:** the flagship live journey — TUI legs `hero_leg_1`/`hero_leg_3` in `tests/tui_e2e_tests/hero.rs` (driven, not standalone), orchestrated by `web/e2e/live/hero.live.spec.ts`. Deterministic turns come from a fake Ollama server (`web/e2e/live/fake-ollama.ts`) + a temp `init.lua` (`hero-setup.ts`), whose arrival the setup proves by reading the daemon's effective config. Run with `just web-test hero`.

---

## Coverage matrix maintenance

When a story ships or a gap closes, update the tier annotations here — this file is the coverage matrix of record. New TUI features require a story here plus at least T1 + T2 coverage before merging (see AGENTS.md TUI Testing Workflow).

## See Also
- [[Web User Stories]] — browser chat + kiln editing stories
- [[Help/TUI/E2E Testing]] — PTY harness reference
- [[Meta/Product]] — feature inventory these stories mirror
