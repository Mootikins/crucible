---
title: Web User Stories
description: User stories for the web UI's chat and kiln-editing flows, with acceptance criteria and test-tier mapping
tags: [meta, ux, web, user-stories, testing]
updated: 2026-07-02
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
**Acceptance:** first user message triggers auto-title exactly once (never overwrites manual titles); switching loads `/history` correctly; end/archive states visible.
**Tests:** W2 (partial), W4 (resume against real persistence — GAP).

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
**As a user**, the file tree shows workspace files and kiln notes as separate sections with type icons; clicking opens the file.
**Acceptance:** tree loads `/api/kiln/files` + `/api/kiln/notes`; loading/error states render; folders expand/collapse.
**Tests:** W4 live (`kiln-truth.live.spec.ts` — the `/api/kiln/notes` listing against a real daemon). **GAP:** the browse/tree *UI* (`FilesPanel`, workspace vs. kiln sections, type icons, expand/collapse) is NOT covered — the editor harness mounts only `EditorPanel`, and the live spec asserts the notes-listing API, not the tree component.

### WS-202: Edit a note and save it
**As a user**, I open a note in the CodeMirror editor, type, see a dirty ● on the tab, save (button/Cmd-S), and the dot clears.
**Acceptance:** edits hit `PUT /api/notes/:name`; success clears dirty; failure keeps dirty + shows a toast; content round-trips exactly (frontmatter, unicode, wikilinks untouched).
**Tests:** W2 full round-trip via the real editor harness (`editor-roundtrip.story.spec.ts` — open → dirty ● → PUT body → clean; save-failure keeps dirty), **W2 through the SHIPPED app** (`editor-shipped-app.story.spec.ts` — real `App` mount, file opened via the product `openFileInEditor` path, real `FileViewerPanel`, Save button + Cmd-S), W4 save-lands-on-disk (`kiln-truth.live.spec.ts` — byte-exact temp-kiln file).
**Product bugs — FIXED (were bugs 3/4/8):** (3) `App.tsx` now mounts `<EditorProvider>` around `WindowManager`, so the editor is reachable in the shipped app; (4) `FileViewerPanel` now has a Save button + Cmd/Ctrl-S wired to `EditorContext.saveFile`; (8) content load switched from `getNote()` (metadata only — `get_note_by_name` returns no `content`) to `getFileContent()` (`GET /api/kiln/file`), so the editor hydrates real bytes. Save still uses `PUT /api/notes/:name` (unchanged, works).
**Latent loop fixed alongside:** with the editor finally reachable, `FileViewerPanel`'s dirty-sync effect read `windowStore` (via `findTabByFilePath`) and wrote it (via `updateTab`) in one tracked scope → self-retriggering stack overflow; the write is now `untrack`ed, and `Pane.renderContent` re-renders only on tab identity/type change (not on `updateTab` ref churn) so edits are not discarded.

### WS-203: Multi-file tabs with unsaved-changes safety
**As a user**, multiple open files show as tabs; each tracks its own dirty state; closing a dirty tab warns me.
**Acceptance:** switching tabs preserves per-file undo/content; dirty markers independent; close-with-unsaved prompts (or documents that it discards).
**Tests:** W2 via the real editor harness (`editor-tabs.story.spec.ts` — content preserved across tab switches).
**Product bugs found (still open — bugs 5/6):** (5) the reused single CodeMirror instance re-dispatches its doc on active-file change, so the update listener spuriously marks the incoming/second-opened file dirty; (6) `closeFile` has no unsaved-changes guard (silent discard).

### WS-204: Syntax-aware editing
**As a user**, markdown, rust, and js/ts files highlight appropriately in the editor.
**Acceptance:** language detected by extension; theme consistent (one-dark); large files stay responsive.
**Tests:** W1 (exists), W3 markdown highlight baseline (`editor-roundtrip.story.spec.ts`).
**Product gap found (still open — bug 7):** `getLanguageExtension()` handles only `.md`; rust/js highlighting is unimplemented despite the CodeMirror lang deps being installed. The baseline is markdown-only for that reason.

### WS-205: Writes are kiln-safe
**As a user/operator**, the browser can never write outside the kiln or workspace roots.
**Acceptance:** traversal names (`../`, absolute) rejected by `PUT /api/notes/:name` and `PUT /api/kiln/file` with 4xx; oversized content rejected (MAX_CONTENT_SIZE).
**Tests:** Rust route tests (exist), W4 live (`kiln-truth.live.spec.ts` — traversal note name rejected 4xx by the real daemon; nothing written outside the kiln).

### WS-206: Chat and editor share one kiln truth
**As a user**, a note the agent creates in chat appears in the file tree, and a note I edit is what the agent reads next turn.
**Acceptance:** tree refreshes on note creation events; agent tool reads reflect saved edits (through daemon, no cache staleness).
**Tests:** W4 (the flagship live-stack story — `kiln-truth.live.spec.ts`: a note written through the browser is the one shared truth — appears in the kiln tree and is byte-exact on disk, what an agent tool reads next turn).

---

## Infra requirements these stories impose (status)

1. **vitest gates CI** — DONE: `just ci` runs `web-test-unit`; the GitHub `test-web` job runs `bunx vitest run` (617 tests).
2. **Story specs run with video + trace ON** and step screenshots — DONE: the `stories` Playwright project (`e2e/stories/**`) with `createStory().step()`; the existing default project keeps its cheap settings.
3. **W4 harness** — DONE: `playwright.live.config.ts` + `e2e/live/global-setup.ts` boot `cru web` on an isolated socket against a `TempDir` kiln; teardown stops the daemon and kills the tree. Gated on a `cru` binary (`CRU_BIN`); skips cleanly otherwise. NOTE: the daemon can't be pointed at `mock-acp-agent` from the web session route (hardcoded internal agent), so the live specs cover the deterministic kiln-truth path rather than agent turns.
4. **Visual baselines** committed under `e2e/__screenshots__/` — DONE: markdown editor + chat mid-stream/complete, each eye-verified before commit.

## See Also
- [[TUI User Stories]] — terminal counterpart
- [[Meta/Product]] — Web & Desktop feature inventory
