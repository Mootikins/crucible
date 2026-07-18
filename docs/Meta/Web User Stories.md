---
title: Web User Stories
description: User stories for the web UI's chat and kiln-editing flows, with acceptance criteria and test-tier mapping
tags: [meta, ux, web, user-stories, testing]
updated: 2026-07-15
---

# Web User Stories

Stories for the two web surfaces that matter now: **chat** and **kiln/note editing**. Test tiers:

| Tier | Mechanism | Determinism |
|------|-----------|-------------|
| **W1 unit** | vitest + @solidjs/testing-library (jsdom, mocked fetch) | full |
| **W2 e2e-mock** | Playwright against `bun run dev`, `page.route()` + mock SSE frames | full |
| **W3 visual** | Playwright `toHaveScreenshot()` baselines at key states; video+trace ON for story specs | full (pixel-tolerant) |
| **W4 e2e-live** | Playwright against real `cru web` + daemon + `mock-acp-agent` + temp kiln | deterministic agent, real stack |

W2 specs double as **image sequences**: `screenshot()` after each scripted step into the test's artifact dir; W3 pins the key frames; video records the whole story.

## Coverage governance

The tiers only help if scenarios move between them deliberately. These rules decide when.

**Promote a GAP / manual scenario to an automated tier when all three hold:**
- **Deterministic** — the outcome is fixed given the inputs. Mock the model at the wire (fake-ollama / mock-acp-agent) so replies are scripted; wait on conditions (`expect.poll`, `toBeVisible`), never `page.waitForTimeout`.
- **Acceptance-criteria-shaped** — the story has a concrete "then" you can assert via a role/label/text/testid locator, not a raw CSS class and not an eyeball.
- **Broke once** — a real regression slipped through here. Promotion buys the most where it already cost us.

Until a GAP meets all three, leave it marked GAP with a one-line note on what blocks automation.

**Graduate a mock tier (W2 e2e-mock) to the live tier (W4 e2e-live) when the assertion depends on state that crosses the daemon boundary** — real session persistence, resume from history, kiln-file bytes on disk, or cross-console visibility with the TUI. W2's `page.route()` mocks fake the daemon's responses; only W4 (real `cru web` + daemon + temp kiln) proves the state is actually there. If a story's "then" is "the same state is visible on disk / from the other console", it belongs in W4.

**Every new feature adds a story and a tier before it merges.** A behavior with no WS entry and no tier is untested by definition. Add the story (with acceptance criteria), pick the lowest tier that can prove it, and — if it crosses the daemon boundary — add the W4 leg too.

---

## 1. Chat

### WS-101: Send a message and watch it stream
**As a user**, I type a message, hit send, and watch the reply stream in token-by-token with a working indicator.
**Acceptance:** input clears on send; deltas append without re-render jumps; completion shows token usage; SSE reconnects transparently on drop.
**Tests:** W2 (exists), W3 (streaming mid-state + complete baselines — `chat-stream.story.spec.ts`). W4 live streaming is intentionally NOT covered: the web session route hardcodes the internal agent, so `mock-acp-agent` is unreachable from the browser and there is no deterministic in-tree provider; the streaming path is covered deterministically at W3.

### WS-102: Thinking blocks
**As a user**, extended thinking renders as a collapsible block distinct from the answer.
**Acceptance:** collapsed by default with token count; expanding reveals content; hidden from copy/export of the answer.
**Tests:** W1 (exists), W2 story.

### WS-103: Tool calls as cards
**As a user**, tool executions render as cards with name, status spinner → result, and expandable output.
**Acceptance:** parallel calls render as separate cards keyed by call_id; errors styled distinctly; large output truncated with expansion.
**Tests:** W1 (ToolCard pinned 90%+), W2 (exists), W3 baseline.

### WS-104: Approve or deny a permission from the browser
**As a user**, when the agent requests permission mid-turn I get a modal with tool, args, and a diff preview; my choice (allow once/session/project, deny) resumes the turn.
**Acceptance:** modal opens from `interaction_requested` SSE; diff renders old/new for file writes; choice POSTs `/api/interaction/respond`; deny yields an error tool card and the turn continues; queued requests open sequentially.
**Tests:** W2 full flow (`permission.story.spec.ts` — allow-once/scope/deny payloads + write-diff + queued sequence). W4 live is excluded for the same reason as WS-101 (mock-acp-agent unreachable from the web session route).

### WS-105: Answer agent questions (Ask)
**As a user**, single-select, multi-select, and free-text questions render as modals I can answer or cancel.
**Acceptance:** all Ask variants render; cancel sends a cancelled response; answer resumes the turn.
**Tests:** W1 (exists), W2 (`ask.story.spec.ts` — single/multi-select + free-text). GAP documented: `AskInteraction` has no cancel affordance, so "cancel sends a cancelled response" is unimplemented.

### WS-106: Switch model mid-conversation
**As a user**, a model picker below the input lets me switch models without losing history.
**Acceptance:** picker lists `/api/session/:id/models`; switch POSTs and is reflected in subsequent turns; history intact.
**Tests:** W2 (exists — verify post-switch turn), W3.

### WS-107: Sessions: create, switch, resume, auto-title
**As a user**, I create sessions, switch between them, resume old ones with full history, and see auto-generated titles I can override.
**Acceptance:** the daemon auto-titles a session on its first completed turn and broadcasts `title_changed`; the web renders the pushed title everywhere (tab, session list, inbox) and never generates titles client-side (never overwrites manual titles); switching loads `/history` correctly; end/archive states visible.
**Tests:** W2 (partial; `title-generation.spec.ts` — `title_changed` SSE renames tab+list, untitled fallback label, no client calls to the title endpoints — realigned 2026-07-12 after the daemon took ownership of titling), W4 (resume against real persistence — GAP).

### WS-108: Cancel a turn
**As a user**, a stop control cancels the in-flight turn, preserving partial output.
**Acceptance:** cancel POSTs `/api/session/:id/cancel`; stream closes cleanly; partial message retained with a cancelled marker.
**Tests:** W2 (`cancel.story.spec.ts` — stop control → `/cancel` POST → `[cancelled]` marker).

### WS-109: Export a session
**As a user**, I export the conversation as markdown from the browser.
**Acceptance:** export dialog offers formats; downloaded content matches `/api/session/:id/export`.
**Tests:** W1 (exists), W2 download flow (`export.story.spec.ts` — preview + downloaded bytes match `/export`).

## 2. Kiln & Note Editing

### WS-201: Browse workspace files and kiln notes
**As a user**, the file tree shows a real hierarchical folder tree for the selected root; clicking opens the file. (Superseded/expanded by **WS-217** — the unified explorer.)
**Acceptance:** kiln roots build the whole tree from `list_notes` (folders split from kiln-relative paths); project roots lazy-load one level via `fs.list_dir`; folders expand/collapse with type icons; click opens via `openFileInEditor`.
**Tests:** W1 SHIPPED 2026-07-18 — the tree *UI* GAP is now covered: `FileTreeView.test.tsx` (real zag machine — `role=tree`, `aria-level`, folders-first order, click→open-leaf-exactly-once routing-seam guard, branch-click-does-not-open, `aria-current="page"`), `kiln-builder`/`collection`/`reconcile`/`tree-root`/`treeRootStore` suites. **Remaining GAP:** W2 Playwright (context-menu interactive open, drag) and W4 live (`fs.list_dir` project walk against a real daemon) still deferred; the daemon `fs.list_dir` security properties are Rust-unit-covered (`server::fs::tests`).

### WS-202: Edit a note and save it
**As a user**, I open a note in the CodeMirror editor, type, see a dirty ● on the tab, save (button/Cmd-S), and the dot clears.
**Acceptance:** edits hit `PUT /api/notes/:name`; success clears dirty; failure keeps dirty + shows a toast; content round-trips exactly (frontmatter, unicode, wikilinks untouched).
**Tests:** W2 full round-trip via the real editor harness (`editor-roundtrip.story.spec.ts` — open → dirty ● → PUT body → clean; save-failure keeps dirty), **W2 through the SHIPPED app** (`editor-shipped-app.story.spec.ts` — real `App` mount, file opened via the product `openFileInEditor` path, real `FileViewerPanel`, Save button + Cmd-S), W4 save-lands-on-disk (`kiln-truth.live.spec.ts` — byte-exact temp-kiln file).
**Product bugs — FIXED (were bugs 3/4/8):** (3) `App.tsx` now mounts `<EditorProvider>` around `WindowManager` (+ a `crucible:open-file` event to open a path in the editor programmatically), so the editor is reachable in the shipped app; (4) `FileViewerPanel` now has a Save button + Cmd/Ctrl-S wired to `EditorContext.saveFile`; (8) the editor now loads AND saves by absolute path through `/api/kiln/file` (`getFileContent`/`saveFileContent`) — the old `getNote()` returned metadata only (`get_note_by_name` has no `content`), and `saveNote()`'s note-name route broke for notes in subdirectories (URL-encoded slash). `GET`/`PUT /api/kiln/file` were also fixed server-side to accept absolute paths (containment is still enforced by `find_enclosing_kiln` + within-kiln canonicalization); previously the absolute-path ban made those routes reject every real editor request.
**Latent loop fixed alongside:** with the editor finally reachable, `FileViewerPanel`'s dirty-sync effect read `windowStore` (via `findTabByFilePath`) and wrote it (via `updateTab`) in one tracked scope → self-retriggering stack overflow; the write is now `untrack`ed, and `Pane.renderContent` re-renders only on tab identity/type change (not on `updateTab` ref churn) so edits are not discarded.

### WS-203: Multi-file tabs with unsaved-changes safety
**As a user**, multiple open files show as tabs; each tracks its own dirty state; closing a dirty tab warns me.
**Acceptance:** switching tabs preserves per-file undo/content; dirty markers independent; close-with-unsaved prompts (or documents that it discards).
**Tests:** W2 via the real editor harness (`editor-tabs.story.spec.ts` — content preserved across tab switches, clean tabs stay clean across open/switch, dirty-close confirm dismiss/accept, clean-close never prompts) + W1 (`CodeMirrorEditor.test.tsx` sync-vs-edit, `EditorContext.test.tsx` close guard, `tab-guards.test.ts`).
**Product bugs — FIXED 2026-07-12 (were bugs 5/6):** (5) programmatic doc swaps into the reused CodeMirror instance are now tagged with a `contentSync` annotation and ignored by the update listener, so only real user edits mark a file dirty. Both panels' duplicated inline editors were consolidated into one shared `components/editor/CodeMirrorEditor.tsx` (which also initializes the view from the ref callback — vitest's solid pipeline fires `onMount` before refs, so an onMount-based init never mounts under jsdom). (6) `closeFile()` now confirms before discarding a dirty file (`{force}` opt for callers whose close was already confirmed), and window-tab close paths (`TabBar`, `WindowManager` closeActiveTab) route through `confirmTabClose()`, which checks the tab's `isModified` mirror.

### WS-204: Syntax-aware editing
**As a user**, markdown, rust, and js/ts files highlight appropriately in the editor.
**Acceptance:** language detected by extension; theme consistent (one-dark); large files stay responsive.
**Tests:** W1 (`CodeMirrorEditor.test.tsx` — per-extension language table), W3 markdown highlight baseline (`editor-roundtrip.story.spec.ts`).
**Product gap — FIXED 2026-07-12 (was bug 7):** `getLanguageExtension()` (now in the shared `components/editor/CodeMirrorEditor.tsx`) maps md/markdown, js/jsx/mjs/cjs, ts/tsx, and rs onto the installed CodeMirror language packages; unknown extensions fall back to plain text.

### WS-205: Writes are kiln-safe
**As a user/operator**, the browser can never write outside the kiln or workspace roots.
**Acceptance:** traversal names (`../`, absolute) rejected by `PUT /api/notes/:name` and `PUT /api/kiln/file` with 4xx; oversized content rejected (MAX_CONTENT_SIZE).
**Tests:** Rust route tests (exist), W4 live (`kiln-truth.live.spec.ts` — traversal note name rejected 4xx by the real daemon; nothing written outside the kiln).

### WS-206: Chat and editor share one kiln truth
**As a user**, a note the agent creates in chat appears in the file tree, and a note I edit is what the agent reads next turn.
**Acceptance:** tree refreshes on note creation events; agent tool reads reflect saved edits (through daemon, no cache staleness).
**Tests:** W4 (the flagship live-stack story — `kiln-truth.live.spec.ts`: a note written through the browser is the one shared truth — appears in the kiln tree and is byte-exact on disk, what an agent tool reads next turn).

### WS-207: Backlinks panel for the focused note
**As a user**, with a note focused in the editor, a Backlinks panel shows which notes link here (linked mentions) and which notes this note mentions without linking (unlinked mentions), each unlinked mention convertible to a real wikilink in one click.
**Acceptance:** panel fetches `GET /api/backlinks?kiln=&note=` for the focused note's kiln-relative key; linked mentions list source title + path and open the source on click (`crucible:open-file`); unlinked mentions come from the daemon's `suggest_links` over the note's content, with self-mentions filtered server-side; clicking **Link** rewrites the *open editor buffer* (`[[target]]` or `[[target|mention]]`), marks it dirty, and removes the suggestion; non-markdown/absent focus shows an empty state.
**Tests:** W1 (`BacklinksPanel.test.tsx` — sections, note-key derivation, empty states, open-event, one-click insertion; `note-actions.test.ts` — `insertWikilink` offset/drift/double-wrap cases), W2+W3 (`backlinks-panel.story.spec.ts` — real panel beside the real editor via the harness `?backlinks=1`; visual baselines `backlinks-panel.png` + `backlinks-after-link.png`; asserts the buffer rewrite, dirty ●, suggestion removal, and linked-mention open), Rust route contract tests (`route_contract_tests/kilns.rs::backlinks_*` — param validation, 404, traversal rejection, self-mention filtering, degraded unlinked on missing file). Daemon side: `get_backlinks` RPC over the `note_links` reverse index with candidate matching (stem / path / extension-less path / title, case-insensitive, fragment-stripped) — unit-tested in `kiln_manager.rs`.
**Scope note:** "unlinked mentions" are *in this note* (what `suggest_links` computes). Obsidian-style *incoming* unlinked mentions (other notes mentioning this title) would need a kiln-wide text scan — deferred.

### WS-208: Wikilink hover previews everywhere
**As a user**, hovering any `[[wikilink]]` — in a chat message, in the editor, or on a backlinks row — floats a preview card with the note's title, path, and a rendered excerpt; clicking the card title opens the note.
**Acceptance:** one document-level controller (`WikilinkHoverPreview`, mounted once in `App`) reacts to any element carrying `data-note`; preview resolves via `getNote` + `/api/kiln/file` with frontmatter stripped and the excerpt truncated on a line boundary; misses render "Note not found"; results cached per kiln+name; card dismisses on hover-away and survives hovering the card itself; aliased links (`[[note|alias]]`) display the alias but resolve the target.
**Tests:** W1 (`WikilinkHoverPreview.test.tsx` — show/dismiss/missing/click-through/data-kiln; `note-actions.test.ts` — excerpt, caching, degradation; `markdown.test.ts` — alias + fragment target parsing), W2+W3 chat surface (`wikilink-hover.story.spec.ts` — anchor renders in a streamed assistant turn, preview shows rendered markdown, dismisses, click opens an editor tab; baseline `chat-wikilink-hover-preview.png`), W2+W3 editor surface (`wikilink-navigation.story.spec.ts` — baseline `editor-wikilink-hover-preview.png`).

### WS-209: Follow wikilinks in the editor
**As a user**, `[[wikilinks]]` in a markdown buffer are visibly link-styled; Ctrl/Cmd+Click or **Mod-Enter** (cursor inside the link) opens the target note; a plain click just moves the cursor.
**Acceptance:** CodeMirror `MatchDecorator` marks links (`.cm-wikilink` + `data-note`, alias/fragment-normalized) in markdown files only; Ctrl/Cmd+mousedown and a `Prec.high` Mod-Enter keymap (so `insertBlankLine` from defaultKeymap doesn't shadow it — and still runs outside links) call the panel's follow handler; following is navigation, not an edit (buffer stays clean); missing targets warn via toast.
**Tests:** W1 (`wikilink-extension.test.ts` — decorations incl. incremental edits, alias/fragment targets, target-at-cursor, both gestures, plain-click inertness), W2+W3 (`wikilink-navigation.story.spec.ts` — decorated baseline `editor-wikilink-decorated.png`, hover preview, Ctrl+Click opens a tab, Mod-Enter follows with the source staying clean).

### WS-210: Editor feel — vim keybindings, markdown preview, shell theming
**As a user**, the editor feels like mine: vim modal editing by default (toggleable in Settings), an Edit ↔ Preview toggle that renders the note like Obsidian's reading view, and chrome that matches the ember shell instead of oneDark's blue-grey.
**Acceptance:** vim (`@replit/codemirror-vim`) is ON by default via the persisted `editor.vimMode` setting (Settings → Editor toggle, applies live to open editors); the buffer opens in normal mode (`x` deletes, `i` inserts); the preview toggle (corner button / Mod-Shift-E — plain Ctrl-E belongs to vim scroll) renders through the same markdown pipeline as chat, with frontmatter stripped and wikilinks as live `data-note` anchors (hover cards + click-to-open); switching files returns to edit mode; non-markdown files get no toggle; editor background/gutters use shell tokens (`crucibleEditorChrome` must precede oneDark — earlier CM6 extensions win); wikilinks render as ember pills with hover underline.
**Tests:** W1 (`EditorWithPreview.test.tsx` — toggle render/back, frontmatter strip, wikilink click-through, non-md exclusion, per-file reset, vim x-deletes vs no-vim inertness), W2+W3 (`editor-vim-preview.story.spec.ts` — vim normal/insert mode e2e with the product default, preview baseline `editor-markdown-preview.png`; re-pinned editor baselines carry the shell chrome + pill styling). Story harness runs vim-off by default (`{ vim: true }` opts into the product default) since most stories type plain text.
**Scope note:** superseded in part by WS-212 — the reading view remains the third mode; inline live preview shipped.

### WS-211: Panel chrome that holds still — 1px separators, fixed toggles, draggable collapsed tabs
**As a user**, the workspace chrome behaves like Obsidian's: separators between panels are hairlines (not fat bars), the collapse/expand controls stay put when a panel changes state, and a collapsed panel's tabs are still first-class — I can drag them anywhere.
**Acceptance:** edge-panel and split separators render as 1px lines with an invisible ±4px grab zone (`after:` pseudo), highlighting on hover and ember on drag; the expanded panel content draws no border of its own (the separator line is the border, and it faces the center on every side — the right panel's border sits on its left); the collapsed strip anchors its expand button in a fixed top `h-9` slot (mirroring the tab-bar row it replaces) on vertical panels and pinned to the right end on the bottom strip; the expanded edge tab bar keeps an in-place collapse button in the same corner; collapsed strip icons are draggable with the standard tab payload (drag out to any pane/bar; drop onto a strip lands in that panel and expands it).
**Tests:** W1 (`resize-handles.test.ts` — 1px + grab-zone + fixed-slot contract; `dnd-remount.test.tsx` — draggables re-register with the live group id after a layout restore swaps ids, for both collapsed strips and edge bars — the silent-no-op bug that made collapsed-pane DnD dead in the shipped app), W2 (`tab-reorder.spec.ts`, `cross-zone-dnd.spec.ts`, `windowing-comprehensive.spec.ts` cover drags across the new chrome).
**Extension (unified drag abstraction, 2026-07-16):** any surface can join the drag system by carrying a Tab payload — `DragSource` gained a group-less `'newTab'` variant placed by `placeNewTab` (pane split, tab bar, edge panel with expand-on-drop, floating window at a drop point via `resolveNewTabTarget`; duplicate identities focus instead of duplicating). W1: `tab-placement.test.ts`. Superseded as the hover-card mechanism by WS-213 (hover popovers ARE floating windows), but remains the extension point for future drag sources (file-tree rows, search results).

### WS-213: Hover popovers are windows; panels grow out of ribbons (Obsidian parity)
**As a user**, hovering a wikilink opens a real floating editor window (Obsidian Hover Editor), and every edge panel grows out of an always-visible icon bar (Obsidian ribbon) — one window abstraction, no bespoke popups, no chrome that vanishes.
**Acceptance:** hover spawns a TRANSIENT `FloatingWindow` (same component as pop-outs/tear-offs) next to the anchor with a real editor inside; it auto-closes on hover-away unless pinned (titlebar pin, or auto-pin on drag/resize — Hover Editor semantics); transient windows and their tab groups never persist to the saved layout; DOM churn under a stationary pointer (loading overlays, CodeMirror redecoration) must NOT dismiss it (note-name + anchor-rect tracking); only loading/not-found render a small card; popovers open in the configured `editor.hoverMode` — default the fully rendered reading view. Floating titlebar (slim, 1px border): pin (transient only) | title=drag | tab-bar toggle (compact popovers hide the tab bar; showing it exposes native tab DnD) | dock | roll-up | maximize-with-restore-bounds (leaves the far-left ribbon visible) | close. Edge ribbons render unconditionally at all three edges; ribbon icons toggle their panel (expand+activate / collapse-on-active-click / switch), the panel appearing between ribbon and center; the flyout subsystem is REMOVED (ribbon click = expand). Every ribbon leads with its panel's expand/collapse toggle; the left ribbon hosts command buttons (palette modal, new session; settings gear pinned at the bottom, Obsidian's own layout). Floating windows AUTO-CLOSE when their last tab is removed or dragged out. FileViewerPanel's loading overlay only covers files that have no content yet (EditorContext.isLoading is global).
**Tests:** W1 (`WikilinkHoverPreview.test.tsx` — spawn/dismiss/pin/dedupe/missing-card; `floatingWindow.transient.test.ts` — pin, persistence exclusion, maximize restore-bounds; `resize-handles.test.ts` — ribbon contract), W2+W3 (`wikilink-hover.story.spec.ts` — popover open/dismiss/pin in chat, baseline `chat-wikilink-hover-preview.png`; `wikilink-navigation.story.spec.ts` — popover in the editor harness, baseline `editor-wikilink-hover-preview.png`; windowing e2e realigned to ribbon semantics).

### WS-212: Live preview — prose-first markdown editing (Obsidian-style)
**As a user**, markdown notes open in live preview: styled prose — sized headings, bold/italic/strikethrough, inline code as a mono chip, wikilinks as pills showing their display text — with the syntax characters hidden; the ONE construct my cursor is in shows its raw source for editing, and ONLY inline code/code blocks render monospace. The raw mono flow is a mode toggle (source), and the fully rendered reading view stays on Mod-Shift-E.
**Acceptance:** live preview is the DEFAULT for `.md`/`.markdown` (never active for other files, which stay source with no mode controls); a ViewPlugin over the lezer syntax tree hides `#`/`**`/`*`/`~~`/backticks/link+wikilink brackets and styles content, revealing marks when a selection touches the construct (whole line for headings); aliased wikilinks display only the alias; bare `[text]` (a URL-less lezer Link node) is NOT treated as a link; fenced code and blockquotes get line styling; the content font is prose (Plex Sans) with mono reserved for code; typing inside a revealed construct keeps it raw (edit-in-place); markdown parses with the GFM base (strikethrough, tables, task lists); switching files returns to the default mode; live preview matches the reading view's chrome — NO line-number gutter (source mode keeps it) and a configurable readable line width (`editor.maxLineWidth`, default 768px, 0 = full) shared with the rendered view; markdown TABLES render as real HTML tables (sanitized through the chat pipeline) until the selection enters the table — clicking a rendered table drops the cursor in for raw editing (block widgets via StateField; CM6 forbids block decorations from ViewPlugins).
**Tests:** W1 (`live-preview.test.ts` — hide/style per construct, cursor-reveal exclusivity, heading line-reveal, aliased wikilinks, edit-in-place, no-extension source behavior; `EditorWithPreview.test.tsx` — live default, mode toggle round-trip, non-md exclusion), W2+W3 (`editor-live-preview.story.spec.ts` — baselines `editor-live-preview.png` (styled) and `editor-live-reveal.png` (cursor inside inline code, everything else styled); `wikilink-navigation.story.spec.ts` + `backlinks-panel.story.spec.ts` realigned to pill display with cursor-reveal).

### WS-214: Saving without a save bar
**As a user**, buffers save the way I configure — not via a toolbar bolted onto every editor: Mod-S or Mod-Enter from the keyboard, an idle autosave interval, and an optional status-bar affordance showing the active buffer's dirty state.
**Acceptance:** FileViewerPanel has no save toolbar; Mod-Enter saves except when the cursor is ON a wikilink (follow wins; insertBlankLine is deliberately shadowed); `editor.autosaveSeconds` (Settings → Editor, 0 = off) saves a dirty buffer after that many idle seconds, each keystroke resetting the countdown; `editor.showSaveButton` toggles a status-bar dirty-dot + Save for the active file that disappears once clean.
**Tests:** W1 (`FileViewerPanel.save.test.tsx` — no toolbar, autosave fires/holds at 0; `EditorWithPreview.test.tsx` — Mod-Enter saves off-link), W2 (`editor-shipped-app.story.spec.ts` — dirty surfaces in the status bar, status-save PUTs and clears, Mod-S path; `hero.live.spec.ts` realigned).

### WS-215: One design system — tokens, one collapse control, motion
**As a user**, the shell looks like one product: every surface, border, text tint, hover, and status color comes from the ember design tokens; each panel has exactly one collapse control; structural surfaces animate in instead of popping.
**Acceptance:** all colors route through the `@theme` tokens in `index.css` (shell-bg/panel, surface-elevated/overlay, control, shell-ink/body, muted/muted-dark, hairline/hairline-strong, hover-wash, error/ok/attention/precog; sky-* exempt as the running-agent hue until a blue token exists); no raw Tailwind gray/status palette classes and no white-alpha borders in `src/`; the ribbon toggle is the only in-panel collapse control (the header keeps Obsidian's corner toggles); floating windows/hover popovers pop in, edge panels slide out of their ribbon, palette/modal fade+pop, drop indicators fade — mount-only, `prefers-reduced-motion` respected, keyframes animate `scale`/`translate` (never `transform`, which would clobber Tailwind's centering transforms); markdown buffers parse a leading `---` block as YAML frontmatter, kept raw mono in live preview; live preview wraps prose lines.
**Tests:** W1 (`style-consistency.test.ts` — repo-wide no-raw-palette + no-white-alpha-border sweep, motion-class presence, keyframe property contract; `resize-handles.test.ts` — no duplicate collapse control; `live-preview.test.ts` — frontmatter + wrapping), W2 (`editor-live-preview.story.spec.ts` — frontmatter mono band + `cm-lineWrapping`). The dead FlexLayout CSS layer (~1200 lines, incl. the unwired light theme) was deleted outright.

### WS-216: A real terminal, a clean panel roster, one background hierarchy
**As a user**, the Terminal tab is a real shell (xterm.js over a PTY), every tab in the default layout is an implemented panel, and panel chrome shares one background hierarchy.
**Acceptance:** the terminal panel runs the user's `$SHELL` in a PTY over `/api/terminal/ws` (localhost-only, same gate as `/api/shell`) with ANSI colors from the ember palette, resize sync, scrollback, and a reconnect affordance when the session ends; the placeholder panels (Explorer/Search/Source Control/Outline/Problems/Output "Coming Soon" stubs) are DELETED — components, registrations, and default-layout tabs — leaving Sessions+Files / Backlinks+Activity / Terminal+Chat as the default; backgrounds follow one rule — app canvas `shell-bg` (header, ribbons, tab bars, status bar), panel content `shell-panel` (editor, terminal, panels; the active tab carries `shell-panel` so it fuses with its content, Obsidian-style), `surface-elevated` for raised rows, `surface-overlay` only for popups/floating windows.
**Tests:** W2 (`terminal.story.spec.ts` — mocked PTY WebSocket: prompt renders in xterm, input echoes, drop → reconnect), W1 (windowing/store suites updated to the implemented-only roster; `panel-placeholders.spec.ts` deleted with its subject).

### WS-217: Unified file-tree explorer — pick any kiln or project root
**As a user**, a top-right dropdown lets me browse any registered **project** or **kiln** as a real hierarchical file tree, and the tree stays live as files change.
**Acceptance:** the dropdown groups roots as Projects / Kilns (from `project.list` + `kiln.list`, deduped), selection persists across reload (localStorage); selecting a kiln builds the whole tree from `list_notes`, selecting a project lazy-walks one folder level per expand via the new daemon `fs.list_dir` RPC; keyboard/ARIA tree (`role=tree`, roving focus, arrows in/out of folders, Enter=open, F2 reserved for Phase-2 rename), sort (name/modified asc/desc), collapse-all, reveal-active, read-only right-click menu (Open/Copy path/Reveal-in-tree); kiln trees patch in place on live `GET /api/fs/events` SSE (non-`.md` leaves ignored), project roots refresh-on-interaction (manual refresh + window-focus refetch of expanded folders). **Security (daemon-side):** `fs.list_dir` is registry-allowlisted (fail-closed), rejects `rel_path` traversal before disk, canonicalize-and-contains, drops per-entry symlinks escaping the root, and hides dotfiles + gitignored entries by default (`show_ignored` reveals).
**Tests:** W1 (`FileTreeView`/`RootDropdown`/`kiln-builder`/`collection`/`reconcile`/`tree-root`/`treeRootStore` — 173 assertions incl. path-splitting, roster grouping/persistence/dedup, reconciler idempotence + `moved`==delete+create convergence), Rust (`server::fs::tests` — nested walk, gitignore/dotfile hiding, symlink-escape exclusion, traversal rejection, dirs-first sort; `web::fs_events::tests` — daemon→SSE event mapping). **GAP:** W2 Playwright interactive (context-menu open, keyboard journey) and W4 live (`fs.list_dir` + live SSE against a real daemon) deferred; project live-watching (`ProjectManager` watcher) deferred to a follow-up — see the Phase-1 plan.
**Scope:** Phase 1 of a 3-phase plan (`thoughts/shared/plans/web-file-tree-explorer-phase1_*`). **Phase 2** = drag-to-move, rename, mkdir, delete-to-trash, DnD drop-lines; **Phase 3** = deterministic resolved-link index + wikilink auto-rewrite on move. Seams laid: node `status` field (future git/diff decoration), `FileDragSource`/`FileDropTarget` payload types, daemon-as-single-write-authority.

### WS-HERO: One session across web and terminal (cross-surface)
**As a user**, a session and its kiln notes are shared truth across the browser and the terminal — the daemon is the hypervisor, the consoles are stateless.
**Acceptance:** the web console resumes a session started in `cru chat` (turn 1 hydrates both sides); opening `from-tui.md` in the real editor shows the terminal's write; editing + saving changes the bytes on disk; a web-sent turn 2 is later visible from `cru chat --resume`; final history is 3 turns and the file carries both the terminal and browser edits.
**Tests:** the flagship live journey `e2e/live/hero.live.spec.ts` (serial), orchestrating TUI legs (`tests/tui_e2e_tests/hero.rs`) around the web console. Deterministic LLM turns via the fake Ollama server (`e2e/live/fake-ollama.ts`) + injected `config.toml` (`hero-setup.ts`). Run with `just hero`. This is the first live-tier story to exercise real agent turns (see infra #3 note).

---

## 3. Shell surfaces (Crucible Shell design)

The four-surface shell from the "Crucible Shell Options" design (turn 5): Home → Inbox → Session ↔ Edit, one connected app. All state stays daemon-side; the surfaces are views over tabs in the window manager.

### WS-301: Land on Home and pick up where I left off
**As a user**, opening the web UI with an empty workspace lands me on Home: a greeting, kiln stats, resume-a-session, recent notes, and a needs-you strip when something waits on me.
**Acceptance:** Home tab auto-opens when the restored layout has no center tabs; resume rows open the session; recent notes (sorted by `updated_at`) open in the editor; the needs-you strip appears only when the attention count > 0 and opens the Inbox; new-session and open-editor actions work.
**Tests:** W1 (`HomePanel.test.tsx` — greeting/relative-time helpers, all-clear vs needs-you, entry points, note ordering via mocked `listNotes`).

### WS-302: See and answer everything waiting on me in the Inbox
**As a user**, the Inbox shows every pending interaction at the top — answerable in place without switching tabs — and every session with live status below.
**Acceptance:** pending permissions render the full interaction (args, scope, diff for writes) via the same `InteractionHandler` as the chat; responding POSTs `/api/interaction/respond`, clears the in-chat prompt (via `crucible:interaction-resolved`), drops the badge, and shows a resolved note; session rows show WAITING/STREAMING/state and open the session.
**Tests:** W1 (`InboxPanel.test.tsx` — all-clear, in-place permission with broadcast + badge drop; `attentionStore.test.ts` — polled-aggregate merge, local-shadows-remote). Rust: `agent_manager::tests::permissions::list_all_pending_permissions_aggregates_across_sessions`, `web::events` wire-parity tests.
**Resolved GAP (2026-07-11):** sessions without an open tab now surface via `session.pending_interactions` (daemon RPC) → `GET /api/interactions/pending` → attention-store polling (10s, visible-tab only); open-tab state shadows the poll. The same change fixed live interaction rendering: the SSE `interaction_requested` payload is now normalized server-side from the daemon wire shape (`{request_id, request:{kind, action:{type,…}}}`) to the flat shape the frontend renders — previously only the e2e mocks' hand-built flat frames ever rendered.

### WS-303: The header is the shell — Home, Edit ↔ Session, Inbox badge
**As a user**, the global header gives me the whole app: logo → Home, an Edit ↔ Session mode pill on the same content, a context line for where I am, and an Inbox button with an attention badge.
**Acceptance:** the active surface is derived from the focused center tab (chat → session, file → edit, home/inbox → themselves; neutral tabs don't change it); goSession focuses the active session's tab or starts a new session; goEdit focuses a file tab or opens the notes tree + empty editor; the badge equals the number of sessions with a pending interaction.
**Tests:** W1 (`shellStore.test.ts` surface mapping/sync, `attentionStore.test.ts` badge counting). W2 GAP: header click-through journey (Home → Inbox → Session ↔ Edit) — promote when it breaks once.

### WS-304: Go anywhere from the omnibox
**As a user**, Ctrl+P opens one omnibox that reaches every surface, note, session, and command, with `>` scoping to commands and `[[` scoping to notes.
**Acceptance:** GO items always present; kiln notes load on open ([[ quick switcher) and open in the editor; sessions resume; prefix routing scopes sections; filtering matches label/description/keywords; query resets on close.
**Tests:** W1 (`CommandPalette.test.tsx` — 18 tests incl. prefix routing and note loading).

### WS-305: The composer shows the session's context
**As a user**, the chat composer shows chips for the workspace the session acts in (⌁) and the kiln it knows (◆).
**Acceptance:** chips render the session's real `workspace`/`kiln` (basename, full path in the title attr); no chip renders when the field is absent.
**Tests:** W1 (`ChatInput.test.tsx` chips block).
**GAP:** attach/detach (`+ kiln`, ✕) from the design is display-only for now — the daemon has no RPC to change a live session's kiln set; blocked on daemon support.

### WS-306: The status bar knows where I am
**As a user**, the status bar's left side shows the active surface (⌂/▤/✎/◆ + name) and the session's workspace · knows-kiln context.
**Acceptance:** surface indicator follows the header pill; context segment renders only when a workspace/kiln is known; static filler (Ready/UTF-8/TypeScript) is gone.
**Tests:** covered indirectly by W1 store tests; W3 baselines pin the rendered bar.

### WS-307: Sessions name themselves after their topic
**As a user**, once a session's first turn completes, it gets a short topic-based title everywhere (tab, Home resume, Inbox) — I never see a wall of "Session chat-202…" again.
**Acceptance:** the daemon generates the title with the session's own LLM provider on the first `message_complete` of an untitled session (truncation of the first user message as fallback) and broadcasts `title_changed`; the open tab, Home resume card, and Inbox lists update without a refresh; untitled sessions fall back to "Untitled · <date>" instead of colliding id slices.
**Tests:** daemon `agent_manager::title` unit tests (sanitize/truncate); web route `auto_title_delegates_to_daemon_generate_title`; reducer `title_changed` matrix + SSE parity tests.

### WS-308: Old sessions archive themselves out of my way
**As a user**, sessions idle for 3 days disappear from Home and the Inbox into a collapsed ARCHIVED section, where I can restore, delete one, or clear the whole history.
**Acceptance:** the daemon sweep archives idle sessions in storage (not just in-memory ones) after `auto_archive_hours` (default 72); Home resume and Inbox RECENT list only non-archived sessions sorted by last activity (Inbox capped at 30 with a count); the ARCHIVED section lazy-loads on expand and offers RESTORE, two-click DELETE, and a two-click CLEAR HISTORY bulk delete.
**Tests:** daemon `test_sweep_archives_stale_persisted_sessions_not_in_memory` pins the storage-sweep gap; sort/fallback helpers in `lib/session-display.ts`. GAP: no component test drives the ARCHIVED section UI yet (manual + live verification only).

---

## Infra requirements these stories impose (status)

1. **vitest gates CI** — DONE: `just ci` runs `web-test-unit`; the GitHub `test-web` job runs `bunx vitest run` (617 tests).
2. **Story specs run with video + trace ON** and step screenshots — DONE: the `stories` Playwright project (`e2e/stories/**`) with `createStory().step()`; the existing default project keeps its cheap settings.
3. **W4 harness** — DONE: `playwright.live.config.ts` + `e2e/live/global-setup.ts` boot `cru web` on an isolated socket against a `TempDir` kiln; teardown stops the daemon and kills the tree. Gated on a `cru` binary (`CRU_BIN`); skips cleanly otherwise. NOTE: the web session route hardcodes the internal agent, so `mock-acp-agent` is unreachable — but the hero tier (`playwright.hero.config.ts`) now makes real internal-agent turns deterministic by pointing the daemon's Ollama provider at a fake Ollama server (`e2e/live/fake-ollama.ts`) via an injected `config.toml`. The base live tier still covers the no-LLM kiln-truth path.
4. **Visual baselines** committed under `e2e/__screenshots__/` — DONE: markdown editor + chat mid-stream/complete, each eye-verified before commit.

## See Also
- [[TUI User Stories]] — terminal counterpart
- [[Meta/Product]] — Web & Desktop feature inventory
- [[Meta/Web UI Feature Spec]] — long-run web UI feature map and design brief
