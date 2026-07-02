---
title: TUI User Stories
description: Complete user stories for every implemented TUI feature, with acceptance criteria and test-tier mapping
tags: [meta, ux, tui, user-stories, testing]
updated: 2026-07-02
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
**As a user**, I type `/` commands (`/help`, `/clear`, `/quit`, `/mode`, `/undo`, `/sessions`, `/files`, registry commands) and they execute locally or route to the agent.
**Acceptance:** unknown command suggests nearest match (levenshtein); `/help` lists commands; `/clear` empties viewport; registry commands forward to agent.
**Tests:** T1 (dispatch per command — GAP today beyond help/clear/model/mode), T2 (help render).

### US-104: REPL `:set` runtime config
**As a user**, I use vim-style `:set key=value` (and `?`, `??`, `&`, `<`) to change runtime config (thinking budget, context strategy/budget/window, autocompact threshold, precognition, perm.*).
**Acceptance:** each documented key round-trips (set → query shows new value); invalid keys error with a message; session-scoped keys sync to the daemon; `:set key?` shows value, `&` resets.
**Tests:** T1 per-key dispatch matrix (GAP), T2 (`:set` result notification render).

### US-105: Double Ctrl+C quit
**As a user**, one Ctrl+C clears input or warns; a second within 300ms quits, so I can't quit by accident.
**Acceptance:** first press with text clears it; with empty input shows the quit warning in the status bar; second within window exits cleanly.
**Tests:** T1 (timing state machine), T2 (ctrl-C notification snapshots exist), T4 (real SIGINT path).

### US-106: Bracketed paste
**As a user**, pasting multi-line text inserts it as one input block without executing lines.
**Acceptance:** paste of N lines yields one buffer with N lines; no premature send; paste inside `:`/`!` modes stays literal.
**Tests:** T1 (paste event handling — GAP: zero today), T4 (real bracketed-paste sequences).

### US-107: Message queueing while streaming
**As a user**, I can type and queue my next message while the agent streams; Ctrl+Enter force-sends.
**Acceptance:** input stays responsive during streaming; queued message sends when the turn ends; Ctrl+Enter interrupts-and-sends.
**Tests:** T1, T3 (stream fixture + queued input), T2 (queued indicator).

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
**Acceptance:** collapsed/expanded states render correctly; toggle applies retroactively to visible blocks; thinking never leaks into graduated content when hidden.
**Tests:** T2 (collapsed/expanded snapshots exist), T1 (toggle state).

### US-204: Markdown rendering
**As a user**, responses render styled markdown: bold/italic, inline code, highlighted code blocks, lists, tables.
**Acceptance:** code blocks are single nodes (no inter-line gaps); syntax colors match theme; wide tables truncate gracefully at narrow widths.
**Tests:** T2 styled snapshots at widths 50/80/120 (partially exists), markdown_fuzz_tests.

### US-205: Context usage + statusline
**As a user**, the status bar shows mode, model, token usage (used/total), and cache hit rate; Lua `crucible.statusline.setup()` reorders it.
**Acceptance:** usage updates after each `message_complete`; Lua config drives layout with builtin fallback; overflow degrades gracefully at narrow widths.
**Tests:** T1 (statusline config), T2 (status_bar width snapshots exist).

## 3. Tools, Subagents & MCP

### US-301: Tool call lifecycle display
**As a user**, running tools show a spinner and smart summary (file/line/match counts); completion shows check or X with collapsible output.
**Acceptance:** parallel calls tracked independently by call_id; tail display capped (50 lines); >10KB output spills to file with a pointer; MCP prefix stripped from names.
**Tests:** T2 (pending/complete snapshots exist), T3 (parallel tool fixture — extend).

### US-302: Subagent display
**As a user**, spawned subagents show status (spawned/completed/failed), elapsed time, and truncated prompt.
**Acceptance:** concurrent subagents render as separate rows; failure state distinct; completion collapses to a summary.
**Tests:** T3 (delegation-demo fixture exists — assert states), T2 snapshots (GAP: no user-story flow).

### US-303: MCP server status
**As a user**, `:mcp` lists configured MCP servers with live connection status that updates at runtime.
**Acceptance:** connected/disconnected/connecting states visible; status refresh on `McpStatusLoaded`.
**Tests:** T1 (status msg handling), T2 (list render — GAP).

## 4. Interaction Modals

### US-401: Permission modal full flow
**As a user**, when the agent needs permission I get a modal with the tool, args, and a togglable diff; y approves, n denies, a allowlists — and the tool then runs or errors accordingly.
**Acceptance:** queued permissions auto-open in order; `d` toggles diff; decision reaches the daemon; deny yields an error tool result and the turn continues; allowlist persists project-scoped.
**Tests:** T2 (modal render + diff exist), **T1+T3 full approve/deny→tool-result flow (GAP)**, permission_invariant_tests.

### US-402: Ask modal
**As a user**, agent questions render as single-select, multi-select (Space), or free-text-"other" modals I drive with the keyboard.
**Acceptance:** all 7 InteractionRequest variants render; Esc cancels with a cancelled response; selection posts the right payload.
**Tests:** T1 (all variants — exists), T2 (snapshots), T3 (interaction fixture — extend).

### US-403: Diff preview
**As a user**, file-op permissions show syntax-highlighted line/word diffs, side-by-side when wide, unified when narrow.
**Acceptance:** create/delete/edit render distinctly; oversize falls back with a truncation footer; `:set perm.show_diff` controls initial visibility.
**Tests:** T2 (11 diff snapshots exist), T1 (perm.* settings — GAP for dispatch).

## 5. Autocomplete & Palette

### US-501: Autocomplete triggers
**As a user**, typing `@` (files), `[[` (notes), `/` (commands), `:` (REPL), `:model `, `:set ` (and args) pops contextual completions I cycle with Tab/arrows and accept with Enter.
**Acceptance:** all 9 trigger kinds produce candidates; filtering narrows as I type; Esc dismisses without inserting; accepted completion replaces the token correctly.
**Tests:** **T1 candidate-generation matrix (GAP: zero inline tests)**, T2 (popup snapshots), T4 (one Tab-cycle smoke).

### US-502: Command palette
**As a user**, F1 opens a palette of every command; typing filters; Enter executes the selection.
**Acceptance:** palette lists slash + REPL commands with descriptions; execution matches typing the command; F1 again / Esc closes.
**Tests:** T1 (open/filter/execute), T2 (palette snapshot).

### US-503: Model switching with lazy fetch
**As a user**, `:model` fetches models on first access (NotLoaded → Loading → Loaded), lets me pick with autocomplete, and switches mid-session preserving history.
**Acceptance:** loading state visible; picker filters; switch confirmed in statusline; history intact after switch.
**Tests:** T1 (state machine — exists), T2, T4 (12 ignored PTY model tests — promote key ones).

## 6. Shell

### US-601: Shell modal execution
**As a user**, `!command` runs in a full-screen modal I can scroll (j/k/g/G/PgUp/PgDn); `i` inserts the output into chat input; Esc closes.
**Acceptance:** exit code shown; long output scrolls; insert puts stdout at cursor; modal restores the chat view intact underneath.
**Tests:** **T1+T2 end-to-end through the app (GAP)**, T4 (one real-command smoke).

### US-602: Shell history
**As a user**, `!` recalls my last 100 shell commands.
**Acceptance:** up/down cycles history in shell mode; history persists within session; duplicates collapse.
**Tests:** T1 (GAP).

## 7. Notifications

### US-701: Toast lifecycle
**As a user**, transient events show toasts that auto-dismiss after 3s; severities are visually distinct.
**Acceptance:** multiple toasts stack in arrival order; expiry removes exactly the aged toast; badge count matches drawer contents.
**Tests:** **T1 lifecycle (stack/expiry — GAP)**, T2 (badge snapshots exist).

### US-702: Messages drawer
**As a user**, `:messages` toggles a full history of notifications so nothing transient is lost.
**Acceptance:** drawer lists all session notifications with severity; toggle preserves scroll; dismiss clears the badge.
**Tests:** T1, T2 (drawer render — GAP as a flow).

## 8. Scrollback & Layout

### US-801: Review history without losing my place
**As a user**, I scroll up through graduated history (PageUp/Dn, mouse), the view holds position while new content arrives, and End/indicator jumps me back to live.
**Acceptance:** scroll disables auto-follow; "new content" indicator appears; jump-to-bottom resumes following.
**Tests:** **T1 scroll state (GAP as user story)**, T4 (real terminal scroll region).

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
**Acceptance:** viewport reflects removed turns; workspace files restored (git and non-git); `/undo` with nothing to undo says so; undo depth reported.
**Tests:** **T1+T3 (GAP: zero undo tests)**, T4 optional.

### US-903: Resume with full history
**As a user**, resuming a session rehydrates the viewport from daemon events with correct rendering of every historical element.
**Acceptance:** history renders identically to live (tools, thinking, modals resolved); statusline reflects restored config (model, budget).
**Tests:** T3 (hydration fixture), T2 snapshots.

### US-904: Event-stream fidelity (replay)
**As a user/developer**, any recorded session replays deterministically (`cru chat --replay`), rendering the same frames every time.
**Acceptance:** replay never re-sends RPC; golden keyword checks pass; `--replay-speed`/`--replay-auto-exit` honored.
**Tests:** T3 (fixture_replay + replay_mode exist), T5 (VHS demos), validate-demos.sh.

---

## Coverage matrix maintenance

When a story ships or a gap closes, update the tier annotations here and the gap list in `thoughts/shared/research/user-story-test-coverage_2026-07-02-0910.md`. New TUI features require a story here plus at least T1 + T2 coverage before merging (see AGENTS.md TUI Testing Workflow).

## See Also
- [[Web User Stories]] — browser chat + kiln editing stories
- [[Help/TUI/E2E Testing]] — PTY harness reference
- [[Meta/Product]] — feature inventory these stories mirror
