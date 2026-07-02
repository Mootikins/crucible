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

---

## 1. Chat

### WS-101: Send a message and watch it stream
**As a user**, I type a message, hit send, and watch the reply stream in token-by-token with a working indicator.
**Acceptance:** input clears on send; deltas append without re-render jumps; completion shows token usage; SSE reconnects transparently on drop.
**Tests:** W2 (exists — extend with reconnect), W3 (streaming mid-state + complete baselines), **W4 (real daemon + mock agent — GAP)**.

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
**Tests:** **W2 full flow (GAP: no E2E today)**, W3 modal baseline, W4 (real permission from mock agent).

### WS-105: Answer agent questions (Ask)
**As a user**, single-select, multi-select, and free-text questions render as modals I can answer or cancel.
**Acceptance:** all Ask variants render; cancel sends a cancelled response; answer resumes the turn.
**Tests:** W1 (exists), **W2 (GAP)**.

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
**Tests:** W2 (GAP), W4.

### WS-109: Export a session
**As a user**, I export the conversation as markdown from the browser.
**Acceptance:** export dialog offers formats; downloaded content matches `/api/session/:id/export`.
**Tests:** W1 (exists), **W2 download flow (GAP)**.

## 2. Kiln & Note Editing

### WS-201: Browse workspace files and kiln notes
**As a user**, the file tree shows workspace files and kiln notes as separate sections with type icons; clicking opens the file.
**Acceptance:** tree loads `/api/kiln/files` + `/api/kiln/notes`; loading/error states render; folders expand/collapse.
**Tests:** W2 (exists partially via bypassed renderer — **rewrite against the real panel**), W3 tree baseline.

### WS-202: Edit a note and save it
**As a user**, I open a note in the CodeMirror editor, type, see a dirty ● on the tab, save (button/Cmd-S), and the dot clears.
**Acceptance:** edits hit `PUT /api/notes/:name`; success clears dirty; failure keeps dirty + shows a toast; content round-trips exactly (frontmatter, unicode, wikilinks untouched).
**Tests:** **W2 full round-trip (GAP: zero browser coverage today)**, W1 (EditorContext exists), **W4 (save reflected on real disk in temp kiln — GAP)**.

### WS-203: Multi-file tabs with unsaved-changes safety
**As a user**, multiple open files show as tabs; each tracks its own dirty state; closing a dirty tab warns me.
**Acceptance:** switching tabs preserves per-file undo/content; dirty markers independent; close-with-unsaved prompts (or documents that it discards).
**Tests:** **W2 (GAP)**, W1 (partial).

### WS-204: Syntax-aware editing
**As a user**, markdown, rust, and js/ts files highlight appropriately in the editor.
**Acceptance:** language detected by extension; theme consistent (one-dark); large files stay responsive.
**Tests:** W1 (exists), W3 (highlight baselines).

### WS-205: Writes are kiln-safe
**As a user/operator**, the browser can never write outside the kiln or workspace roots.
**Acceptance:** traversal names (`../`, absolute) rejected by `PUT /api/notes/:name` and `PUT /api/kiln/file` with 4xx; oversized content rejected (MAX_CONTENT_SIZE).
**Tests:** Rust route tests (exist), W2 (UI surfaces the error sanely).

### WS-206: Chat and editor share one kiln truth
**As a user**, a note the agent creates in chat appears in the file tree, and a note I edit is what the agent reads next turn.
**Acceptance:** tree refreshes on note creation events; agent tool reads reflect saved edits (through daemon, no cache staleness).
**Tests:** **W4 (the flagship live-stack story — GAP)**.

---

## Infra requirements these stories impose

1. **vitest must gate CI** — add to `just ci` and the GitHub workflow (it currently runs nowhere).
2. **Story specs run with video + trace ON** and step screenshots (image sequence per story) — a dedicated Playwright project so the existing 78 keep their cheap settings.
3. **W4 harness**: Playwright `globalSetup` that builds/boots `cru web` against a daemon configured with `mock-acp-agent` and a `TempDir` kiln; teardown kills both. Small spec count, tagged `@live`.
4. **Visual baselines** committed per-OS/per-browser as the repo's snapshot policy demands: verified by eye before acceptance.

## See Also
- [[TUI User Stories]] — terminal counterpart
- [[Meta/Product]] — Web & Desktop feature inventory
