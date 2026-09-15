---
title: Web UI Review — 2026-09-15
description: Screenshot review of the web UI on desktop and phone, a comparison with Codex desktop, Cursor, Obsidian, Claude desktop and Hermes Agent, story gaps, and the design directions built for inspection
tags: [meta, ux, web, review, design]
status: draft
updated: 2026-09-15
---

# Web UI Review — 2026-09-15

This review looks at the web UI at commit `6d01622` on master. It compares the
UI to five neighbour apps. It lists the user stories that are missing or have
no proof. It ends with design directions that a reader can inspect in a
browser and pick from.

Related: [[Web User Stories]], [[Product]], [[Mobile Shell]], [[TUI-Style-Guide]].

## Method and provenance

- The reviewer built the frontend bundle from the working tree and ran
  `cru --standalone web` against a temporary copy of the docs kiln on port
  3100, with the daemon's normal config. One real turn ran on GLM-4.7.
- Screenshots: 17 at 1440×900 and 6 at 390×844. They are served on the LAN at
  `http://192.168.0.16:8090/gallery.html`.
- Assessment A (design review) and Assessment B (mechanical evidence) ran as
  two isolated sub-agents. Neither saw the other's output before synthesis.
  Neither drove the browser; both worked from the screenshots and the source.
  The impeccable detector ran once over `src/`. It reported 8 findings, 6 of
  them in test files. The detector is not the source of the findings below.
- Prior-art facts come from official docs and changelogs. Each claim in the
  comparison section has a source in the research file. Items the research
  could not verify are marked as such.

## Verdict

The product identity lives in the cells, not in the grid. The palette, the
type pairing, the tool cards, the review chips and the precognition badge are
specific to Crucible. The shell is not: two ribbons, three tab strips, edge
panels, splits and a corner bar. VS Code, Obsidian and Zed ship the same frame.

The strongest screen is a running turn. Tool cards name the tool and the file,
a cancel control is present, the token count and the timestamp are honest.

The weakest screens are the first ones. First load shows a layout the user did
not choose, three empty regions, a settings form, and a chat whose composer
says "Select a session first" under a full transcript. The phone first load
says "No note is open." and offers nothing.

Three defects contradict the product principle "show the proof":

1. The Backlinks panel renders a 404 as "Linked mentions (0)".
2. A rejected provider key (HTTP 401 at boot) leaves an empty model chip and no
   message.
3. Two 422 responses on session load never reach the user.

## Heuristic scores (0–4)

| # | Heuristic | Score | Reason |
|---|---|---|---|
| 1 | Visibility of system status | 2 | Tool cards and token counts are excellent. Backlinks report a false zero. The composer contradicts the visible transcript. |
| 2 | Match with the real world | 1 | Eighteen internal terms appear with no reachable definition. |
| 3 | User control and freedom | 3 | Cancel, close, split and layout reset work. A note edit autosaves with no visible undo. |
| 4 | Consistency and standards | 2 | One control carries two labels. The two themes draw different underlines. One colour token carries six meanings. |
| 5 | Error prevention | 2 | The new-session form validates nothing. Failed requests never reach the user. |
| 6 | Recognition over recall | 2 | Eleven surfaces hide behind an unlabelled button. Tab titles truncate past recognition. |
| 7 | Flexibility and efficiency | 4 | Palette, shortcuts, vim keys, splits, floating windows and a persisted layout serve the audience. |
| 8 | Aesthetic and minimalist design | 2 | First load shows three empty regions and clipped terminal text. |
| 9 | Error recovery | 1 | The 404 and the three 422s are swallowed. |
| 10 | Help and documentation | 1 | No inline help. No shortcut list. |

## Findings

Severity: **B** blocker, **M** major, **m** minor. Paths are under
`crates/crucible-web/web/src/` unless stated.

### Honest state

| Sev | Where | What | Fix |
|---|---|---|---|
| B | `components/BacklinksPanel.tsx:80-86`; screenshot d11 | Every failure of `getBacklinks` renders the empty state. A 404 prints "Linked mentions (0) / No notes link here yet." | Split the catch. A transport or server error renders an error row with Retry and "Index kiln". Only a real empty result renders the empty state. |
| B | `lib/local-cache.ts:33-35`, `components/CenterComposer.tsx:140-142` | `swrLocal` drops every fetch error. The composer filters `[error]` models out. A 401 from the provider leaves an empty model chip. | Surface provider failures as a toast with "Open settings". Keep the built-in model list as the fallback, but say so. |
| M | `contexts/ChatContext.tsx:79-90`, `components/SessionStatusChips.tsx:80-89` | `GET /api/session/<id>/modes` returns 422 for a persisted session. Both callers swallow it. The policy chip renders `null`. | Show "policy unknown" with the cause, or resolve the 422 in the daemon for legacy sessions. |
| M | `components/ChatPanel.tsx`; screenshot d01 | The chat panel renders a transcript while the composer reads "Select a session first…" and is disabled. | Bind the composer to the session the panel renders. If no session is bound, render the empty transcript, not a stale one. |
| M | `stores/attentionStore.ts:109-122` | Four `createMemo` calls at module scope. Solid warns four times at boot. | Wrap them in `createRoot`. |
| M | `lib/terminal-availability.ts:33-59` | `/api/config` is refetched every 3 s while `remoteShell()` is undefined. The loop never stops after a failed response. | Fetch once, then subscribe to config change events. |

### Starting a session

| Sev | Where | What | Fix |
|---|---|---|---|
| B | `components/SessionsPanel.tsx:26-32`, `windowing/EdgePanel.tsx:414-421`, `lib/keyboard-shortcuts.ts:40-47` | No visible new-session control on desktop. The only pointer route is a hover control on a project row. The ribbon button was removed. Ctrl+Shift+N is reserved by Chrome and Firefox. With no project registered no row exists to hover. | Put a primary "New session" button at the top of the Sessions panel and a "+" in the rail. Bind Ctrl+N. |
| M | `components/CenterComposer.tsx:414-441`; screenshot d06 | Six scope controls before the first message: kiln, Session folder, Run on, Internal agent, Agent card, Auto. None carries a value a new user can judge. | Show the model chip and the mode chip. Fold the rest behind one "More" disclosure that states the resolved values. Default to the last project and kiln used. |
| M | `CenterComposer.tsx:415`; screenshots d06 and d09 | The same chip reads "Session folder" before the first turn and "kiln" after it. | Label the chip with its role and its value in both states. |

### Shell and navigation

| Sev | Where | What | Fix |
|---|---|---|---|
| M | `lib/keyboard-shortcuts.ts:8-38`, `App.tsx:100-108` | Ctrl+K clears the chat. Ctrl+P opens the palette. Ctrl+T toggles thinking. Ctrl+W closes a tab. Shift+Tab cycles the chat mode. `matchShortcut` maps Meta to Ctrl. | Ctrl+K opens one palette for notes, sessions and commands. Ctrl+P and Ctrl+O stay as aliases. Move Clear chat to the session menu with a confirm. Free Shift+Tab, Ctrl+T and Ctrl+W in a normal tab. |
| M | `lib/keyboard-shortcuts.ts`; no reader in `components/` | Fifteen chords have descriptions that nothing renders. | Add a shortcuts sheet on Ctrl+/ (Codex and Claude desktop use the same key). |
| M | `components/shell/ProjectMenu.tsx`; screenshot d13 | Seventeen projects, then the same seventeen under "Open in a new window". The menu overflows the viewport. | Cap the pin list to recents with a filter. Move "open in a new window" to a per-row action. |
| M | `components/mobile/MobileShell.tsx:31-43`; screenshot m04 | The phone "More" sheet lists eleven panels in alphabetical order with no grouping. Two names are internal: "Surfaces" and "Plugin Blocks". | Group by purpose: Work (Changes, Conflicts, Inbox, Activity), Knowledge (Graph, Search), Extend (Skills, Plugins, Plugin blocks, Surfaces), Settings. Rename the two internal names. |
| M | `components/windowing/TabBar.tsx:115`, `WindowManager.tsx:55`; screenshot d10 | Tab titles truncate at 120 px. "e_refere…" and "AI Features.m" are not identifiable. | Raise the cap to 200 px, elide from the middle, show the full title on hover. |
| M | `components/SettingsPanel.tsx:784-785`; screenshot d06 | The settings table cannot shrink below its min-content width. At 370 px the control column leaves the pane. | Replace the table with a grid that stacks label and control below 480 px. |
| m | d01 through d17 | No "Crucible" wordmark on the desktop shell. The phone shell has one. | Put the wordmark in the status bar or the rail. |
| m | `components/windowing/EdgePanel.tsx`; screenshots d13 and m04 | The "…" rail button opens a project list on desktop and a panel list on the phone. | One control, one meaning. |

### Colour, type and theme

| Sev | Where | What | Fix |
|---|---|---|---|
| M | `components/CommandPalette.tsx:53`; screenshot d05 | The "CMD" badge uses `--color-precog`. The token now carries six meanings. | Use the neutral badge style. Reserve precog for precognition. |
| M | screenshots d09 vs d17, d10 vs d16 | Light theme underlines headings and wikilinks. Dark theme draws neither. `index.css:343` promises identical geometry. | One link treatment in both themes. |
| M | screenshot d09; `lib/markdown.ts` | A wikilink whose title has a space and an extension renders as plain text plus a link: "Getting" + "Started.md". | Resolve `[[Getting Started.md]]` as one link with the note title as text. |
| m | screenshot d01; `--color-term-blue` | The terminal prompt is the loudest saturated area on first load. The terminal clips its own text. | Warm the default prompt slot. Wrap or scroll the terminal content. |
| m | `components/blocks/PluginBlockPanel.tsx`, `PluginCommandDialog.tsx` | `text-[10px]` survives below the 11 px floor. | Move to `--text-floor`. |
| m | 14 files, for example `SkillsPanel.tsx:118`, `Message.tsx:90` | `outline-none` without the shared `focus-ring` utility. | Apply the utility. |

### Accessibility and performance

| Sev | Where | What | Fix |
|---|---|---|---|
| M | `components/NotificationToast.tsx:118`, `ui/ConnectionBanner.tsx:58` | Only two live regions exist. A streamed reply, a tool result and a save result announce nothing. | Add `aria-live="polite"` to the turn container and the save indicator. |
| M | `ui/IconButton.tsx:21` | Desktop icon buttons are 24 px and 28 px. The phone shell is clean at 44 px. | Raise the desktop hit area to 32 px minimum with padding. |
| M | `lib/keyboard-shortcuts.ts` | Shift+Tab removes reverse focus movement for keyboard users. | Free Shift+Tab. Use Ctrl+. for the mode cycle as Cursor does. |
| M | `dist/assets/index-*.js` 2.28 MB | No `lazy()` and no `manualChunks`. shiki, katex and xterm ride the entry bundle. | Lazy-load the terminal, the graph, the canvas and math. Split shiki. |

### Phone shell

| Sev | Where | What | Fix |
|---|---|---|---|
| M | `components/mobile/MobileShell.tsx:269-284`, `SessionsTab.tsx:86`; screenshots m03 and m05 | The drawer stays open after the user picks a session or a note. It covers 320 of 390 px. | Pass an `onPicked` callback and close the drawer. |
| M | screenshot m01 | First load shows "No note is open." | Two actions: "Open a note" and "Start a session". |
| m | screenshot m01 | A focus ring sits on the files button after navigation. | Do not autofocus the drawer button on load. |

## Comparison with the neighbours

Sources are in the research file at the end of this document. "Unverified"
means the research found no primary source.

| Flow | Crucible today | Codex desktop | Cursor | Obsidian | Claude desktop | Hermes | Recommendation |
|---|---|---|---|---|---|---|---|
| New session | Hover control on a project row; palette; Ctrl+Shift+N (reserved by browsers) | Ctrl+N; the composer holds the project and model pickers | Cmd+T new chat tab | Ctrl+T new tab; Ctrl+O creates a note from the switcher | Cmd+N | `/new [name]` | A primary button in the Sessions panel and Ctrl+N. Composer first, defaults from the last session. |
| Model and mode | Chips in the composer; Shift+Tab cycles modes | Composer pickers; three permission presets | Shift+Tab cycles modes; Cmd+. opens the menu | n/a | Selector next to send; Cmd+Shift+M | `/model`, `/reasoning` | Keep the chips. Move the mode cycle to Ctrl+. and show the current mode in the status bar. |
| Tool calls while running | One bordered row per call, check mark, chevron; no duration or size | Diffs inline in the thread (rendering unverified) | Files as inline pills (rendering unverified) | n/a | Collapsible steps with a summary | Streams tool output in the TUI | Group steps under a plain-words headline with duration and size. Thinking joins the group. |
| Waiting and steering | Cancel button; three-dot loader | Work continues in a worktree; the draft is kept | Enter queues, Cmd+Enter steers | n/a | Auto-compaction | Interrupt and redirect | Queue on Enter while running; Ctrl+Enter steers. Show elapsed time on the running step. |
| Permission | Inline card; Inbox | Enter approves, Escape declines | Manual, allow-list, auto-run | n/a | Five modes | Approve once, Always approve, Cancel | Keep the inline card. Add Y / A / N keys. Never let "always" write a permanent config key. |
| Review of edits | Changes panel in a separate tab; hunk merge view; unit-tested only | Review panel Ctrl+Shift+G; review, approve, revise, reject; comments inline | Cmd+Return accept all, Cmd+Backspace reject all; the review pane can vanish on Escape | n/a | `+12 −1` stat opens the viewer; comment on a line, send the batch | `/memory pending`, `/skills diff`, approve, reject | A review bar under the turn with the diff stat as the button. Per-hunk accept. Comments become instructions. Never dismiss on Escape. |
| Session list | Right panel grouped by project; "NO SESSIONS 18" and "ARCHIVED 369" groups; archive and delete on hover | Threads under projects; a Triage inbox | Agents list across workspaces | n/a | Sidebar filtered by status, project, environment | `/sessions`, `/title` | Status first. Show "waiting on you" and "2 edits waiting" in the row. Add rename. |
| Palette and keys | Ctrl+P commands, Ctrl+O notes, Ctrl+K clears | Ctrl+K menu, Ctrl+P files, Ctrl+/ shortcuts | Cmd+K inline edit, Cmd+L panel | Ctrl+P commands, Ctrl+O switcher, recents first | Cmd+K search and navigation, Cmd+/ shortcuts | 309 slash commands | Ctrl+K one palette with recents first and typed prefixes. Ctrl+/ shortcut sheet. Keep Ctrl+P and Ctrl+O as aliases. |
| First run | Persisted layout; three empty regions; no onboarding | Unverified | Unverified | Empty vault with a create-note prompt | Unverified | Setup wizard in the CLI | A Home surface: last session, open notes, one composer, provider status. Revive WS-301. |
| Errors | Silent zero on 404; silent 401; silent 422 | Unverified | Unverified | n/a | Unverified | n/a | Every failed request the UI depends on has a visible shape with the next action. |
| Knowledge | File tree, backlinks, graph without labels or a local view | None | None | Linked mentions, unlinked mentions, local graph, groups by query and colour, linked views | Projects | Staged memory writes with approve and reject | Local graph with labels on the neighbourhood. A linked graph pane that follows the open note. Backlinks with paragraph context. Show the notes a session read on the graph. |
| Phone | Header buttons, drawers that stay open, alphabetical More sheet | Remote in the ChatGPT app; Live Activity for progress | Unverified | Bottom navigation bar, Quick Action pull-down, editing toolbar | Same session on every surface; notifications on completion or a question | Messaging gateways | Bottom tab bar, drawers close on selection, notifications when a session waits on you. |

## Story gaps

The full coverage matrix is in the story-gap file. The rows below are the
flows a user of the neighbour apps expects and Crucible either lacks, ships
without a story, or ships without automated proof.

| Expected flow | Crucible today | Suggested story |
|---|---|---|
| A provider check on launch | `local-cache.ts:33-35` drops the error | WS-324: Tell me when no model provider answers |
| A Home surface | WS-301 is not implemented | WS-301 (revived): Land on Home and pick up where I left off |
| A shortcut list | Chord table with no reader | WS-325: Read every keybinding without leaving the app |
| Fork a session | No `fork` symbol in web `src/` | WS-326: Branch a session from an earlier turn |
| Undo the agent's turn from the browser | Daemon and CLI only (`Product.md:210-217`) | WS-327: Take back the last turn from the browser |
| Rename a session | Titles come from the daemon only | WS-328: Name a session myself |
| Sign out of a remote server | Route exists, no button (WS doc:406) | WS-329: Sign out of a remote server |
| Notifications | Shipped, claimed in Product.md, no story | WS-330: See what a background session needs |
| Surfaces and plugin blocks | Both registered, no story, no spec | WS-331, WS-332 |
| Multi-window | Shipped at `ProjectMenu.tsx:48`, no story, no spec | WS-333: Open a second project in its own window |
| Local graph | WS-222 acceptance has no neighbourhood view | WS-334: See the notes around this one |
| Layout reset | Shipped, no end-to-end test | WS-335: Put the layout back |
| Review in the browser | WS-223, WS-232 and WS-321 share one W2 gap; accept, reject and undo are unit-tested only | Close the W2 gap with a composed-diff fixture |
| Note create, delete, new folder | `window.prompt` and `window.confirm` (WS doc:193) | Replace with in-app dialogs; add a W2 journey |
| Search | WS-229 is a stub; the doc says Navigator, the code registers it centre | Update the doc; add a W2 journey |

Five highest-friction flows, in order: a bad or absent LLM key fails in
silence; starting a session is hidden; review has the largest surface and the
least proof; keyboard discovery is impossible in the app; note creation and
deletion use browser dialogs.

## Design directions

The mockups are served on the LAN at `http://192.168.0.16:8090/`. Every page
has a bar at the bottom right: toggle the theme, add a note, press "Pick
this". Picks land in `choices.json` on the host.

### Scope decision (2026-09-15, after the first pass)

The owner set the scope to **refinement**: keep the current layout and the
window manager, make the styling consistent and thought out, and fix the
noted rough edges. The five style directions below stay online as a source of
details to borrow. None of them is a proposal to replace the shell.

### Refinement decisions (`/decisions.html`, sections R1–R6)

1. **R1 Composer corner radius.** Today `.composer-surface` is a 9999px pill that drops to 10px when multi-line. With the chip row the single-line field is about 80px tall, so the ends are 40px semicircles. Options: keep; a fixed radius from the smallest single-line control (18px, new token); the existing 14px `--radius-3xl`.
2. **R2 One empty-state pattern.** Four shapes coexist. The Activity panel centres a 32px icon over two lines; other panels print one muted line; the empty pane draws a card with hints. Options: keep; one left-aligned sentence plus one action, no icon; one centred shape with a 16px icon inline with the title.
3. **R3 Permission location.** Today the card is inline only. Options: keep; inline card plus a docked "waiting on you" strip above the composer that replaces the "gated" chip while a request is open; move the card into the composer slot with a one-line record in the transcript.
4. **R4 Phone file tree sizing.** Today the phone tree reuses 26px desktop rows with a 14px chevron. Option: 44px rows, 15px text, 28px chevron target, whole-row tap, set by a `data-density="touch"` attribute.
5. **R5 Phone properties block.** Today a mono "▾ 4 properties" line with a 12px target and a phantom bar. Option: a 44px header row that opens key/value rows with tags as chips, the same component at desktop density in the reading view.
6. **R6 Styling consistency sheet.** One pick: one radius per control class, `focus-ring` everywhere with a lint test, one meaning per badge colour ("gated" becomes neutral), one wikilink treatment in both themes, one `EmptyState` component, 200px middle-elided tab titles, 44px on the phone and 32px hit areas on the desktop, the 11px floor everywhere, chips that always show role and value, three row-height tokens. Six tokens and two lint tests. No layout changes.

Sections 1–8 of the same page remain for reference. Options 1C, 3B and 3C
are layout changes and are out of scope.

### Style directions (`/directions/`, reference only)

All five show the same content: one session with a pending permission, one
note with backlinks, the tree, the graph, and the status facts. Each has a
desktop page and a phone page.

| Card | Direction | Detail worth borrowing |
|---|---|---|
| The roll | Reference edition, centre rail (`edition`) | Tool calls as apparatus lines with outcome, duration and size; alternate states shown as labelled specimens. |
| Dealt | Dispatch board (`board`) | State as a colour contract; "waiting on you" pinned above the transcript. |
| Dealt | Cartographer's plate (`map`) | Tool calls shown on the graph as pins; location stated in three registers. |
| Impeccable's pick | Bench instrument (`bench`) | A readout that always says what waits on you; wikilinks as one chip per title. |
| Canon | IDE-native, played straight (`canon-ide`) | A 24px status bar that owns location and cost; step rows for tool calls. |

## Refinement pass on `design/web-refinements` (2026-09-15)

The picks R1–R6 landed on the branch `design/web-refinements`, in five
file-disjoint lanes. The review instance on port 3100 serves the result.

| Pick | What changed | Where |
|---|---|---|
| R1 | The prompt field holds only the textarea and the send button. Model, mode, mic and the scope chips sit on one row under it. The radius is `--radius-composer` (18px) at every height; the pill and the multi-line morph are gone. | `composer/ComposerCard.tsx`, `ChatInput.tsx`, `CenterComposer.tsx`, `styles/refine-composer.css`, `index.css` |
| R2 | One `EmptyState` component: a 16px icon inline with the title, one body line, an optional action. Five call sites use it. A backlinks failure renders an error tone with the HTTP status and Retry. | `ui/EmptyState.tsx`, `ActivityPanel.tsx`, `BacklinksPanel.tsx`, `FilesPanel.tsx`, `SessionsPanel.tsx`, `mobile/MobileShell.tsx` |
| R3 | The permission or ask card docks on the prompt: square bottom corners on the card, square top corners on the field, no gap. The transcript keeps a one-line record ("Asked to use update_note · waiting"). | `ChatInput.tsx`, `MessageList.tsx`, `interactions/PermissionInteraction.tsx`, `styles/refine-composer.css` |
| R4 | `data-density="touch"` on the phone tree: 36px rows, 14px text, 22px chevron slot, 20px indent. Desktop rows stay 28px. | `files/FileTreeView.tsx`, `files/FileTreeNode.tsx`, `mobile/SessionsTab.tsx`, `styles/refine-touch.css` |
| R5 | The properties row keeps the document font, carries a hairline border, spans the column, and is 44px on touch. The grey bar under it was the active-line wash on the hidden gap line; a rule clears it. | `styles/refine-touch.css`, `lib/frontmatter.ts` |
| R6 | `focus-ring` on every interactive element, with a gate. 11px floor everywhere, with a gate. CMD and "gated" badges neutral; the tool badge on the permission card neutral. Wikilinks identical in both themes; headings never underlined. Tab titles cap 200px, middle-elided, full title in `title`. IconButton 32px hit area. Chips show role and value. Palette rows 36px. Tokens `--radius-control/card/composer` and `--row-sm/md/touch`. | `style-consistency.test.ts`, `CommandPalette.tsx`, `SessionStatusChips.tsx`, `windowing/TabBar.tsx`, `ui/IconButton.tsx`, `editor-theme.ts`, `wikilink-extension.ts`, `index.css` |

Two defects surfaced during the in-context check and are fixed on the branch:

- **A held permission did not survive a reload.** The stream carries only new
  requests, so after a reload the composer showed no card while the daemon
  waited, and every send answered 422. `ChatContext` now reads the pending
  aggregate once when it binds to a session. Test:
  `contexts/__tests__/ChatContext.pending-restore.test.tsx`.
- **Tailwind drops an `@import` that follows `@plugin`, silently.** The four
  lane stylesheets vanished from the bundle once. A gate in
  `style-consistency.test.ts` fails when any `@import` follows the first other
  at-rule in `index.css`.

Later in the same pass, on the owner's request:

- The permission card type moved to the transcript sizes (11px mono for the
  argument listing, 12px for the rest).
- The chip row under the prompt is one line that scrolls sideways; an edge
  fades only while a chip hides behind it.
- The review-policy chip reads "Review · gated" in the same role · value form
  as the other chips.

### Second refinement round (owner feedback, same day)

- Mic back inside the prompt beside send; the chip row holds session facts only.
- Chips carry the value only ("GLM-4.7", "Ask", "kiln", "default"); the icon and the tooltip name the axis. The row wraps instead of scrolling.
- The review-policy chip ("gated") is gone from the row; the policy rides the wrapper as data.
- Empty states carry no icon; every empty pane reads the same way.
- Type scale up one step: reading 13px, title 14px, `text-xs` rides the reading size.
- Wikilinks: one look in the transcript and in notes (ember on a faint wash, underline on hover). The "Getting" + "Started.md" split was `linkify` treating `.md` and `.ai` as country domains; the fuzzy TLD list is now the stock default and a link title drops its extension.
- Tab titles fade only when the box cuts them; a short title on an active tab no longer fades.
- The turn footer (copy, regenerate, time) is always visible and shows the turn's duration ("4.2 s", "1 m 16 s") when the daemon stamped the end; a turn rebuilt from history keeps the clock time.
- The permission card type sits at transcript sizes.
- Chat measure raised from 46rem to 64rem.
- Swap sides: collapse travels with the pane (the old rule kept it on the side, on purpose, so one flip revealed the stowed tree; the owner chose the pane rule). A swap no longer remounts the panes: the shell row and every split render as keyed lists, so a flip is a reorder and transcripts keep their state.
- A live turn no longer renders its answer twice: at `message_complete` a spent placeholder gives up the canonical id and the answer claims it; `stripFrozenPrefix` forgives a lost trailing space at a segment seam.

Not done, on the owner's stop: the store clamps every edge panel to 600px
(`layoutActions.ts:168`) while the resize handle allows `innerWidth - 320`.

### Adversarial review (same day)

A reviewer agent attacked the branch diff and reported 21 findings. Fixed:
the `@layer` statement now precedes Tailwind's import so `cru-theme` is the
first layer; a reopened session can no longer lose its tab; a request the
user answered during the pending fetch cannot come back; only a
client-minted placeholder (flagged `placeholder: true`) may give up the
canonical response id; bare modern domains (`example.dev`) link again while
`md` and `ai` stay out; `sessionsSide()` falls back to the files rail; the
dead vertical-split branch is gone and a stacked centre has a test; a tab
tooltip appears only when the label is cut; the turn reserves the footer's
height; the touch tree reads a font token; stale comments corrected.

Noted, not changed: the panel id persists across a swap (nothing resolves a
panel by id; a gate would be the fix); the Inbox panel and the docked card
can both answer one request; the phone drawer selects its tab through a DOM
click; the `focus-ring` utility sets `border-radius: inherit`; a live turn
shows its duration and keeps the clock time in the tooltip only.

### Token contract for plugin overrides

`index.css` now declares 84 `--cru-*` tokens in `@layer cru-theme`, with
both theme blocks inside that layer. The `@theme` block holds aliases that
read the contract names, so Tailwind utilities and plain CSS resolve to the
same value, and a plugin's unlayered `:root` rule wins in both themes with no
`!important`. The legacy `--color-*`, `--radius-*` and `--row-*` names remain
as aliases. Two gates guard it: no raw px, hex or rgba in a component outside
a named allow-list, and every token defined in both themes. The contract and
a worked plugin stylesheet are in [[Web Theme Tokens]]. The delivery path (a
fragment field, a CSS-only route and a `<link data-plugin-theme>`) is
described, not built; the daemon is untouched.

Two deviations to decide:

- The empty-state title is 13px, a fourth size next to the three the type
  scale documents. Keep it, or set it to the reading size with weight 600.
- The collapsed properties row stops 56px short of the column edge on desktop
  because `.fm-card` reserves the gutter for the floating mode toggles. Full
  width needs a new anchor for those toggles.

## Recommended sequence

1. Fix the three honest-state defects (backlinks 404, provider 401, session 422). These are bugs, not design.
2. Put a visible "New session" control in the Sessions panel and bind Ctrl+N. Fold the composer chips to two plus "More".
3. Move the palette to Ctrl+K, keep the aliases, and add a Ctrl+/ shortcut sheet. Free Shift+Tab.
4. Close the phone drawer on selection. Group the More sheet.
5. Pick R1–R6 on the LAN page, then write the token and lint changes from R6 as one change, and the R1–R5 fixes as one change each.
6. Add the missing stories from the table above to [[Web User Stories]], starting with WS-324 and WS-301.

## Files from this review

- Screenshots: `.playwright-mcp/review/` (not committed) and the LAN gallery.
- Assessment A, Assessment B, prior-art research and the story-gap matrix are
  in the session scratchpad and are reproduced on the LAN report page.
