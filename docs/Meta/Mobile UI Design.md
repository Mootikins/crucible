---
title: Mobile UI Design
tags:
  - meta
  - design
  - mobile
---

# Mobile UI Design — "Thread"

> Design brief for a mobile companion UI, synthesized from a three-agent
> exploration (competitive UX research, three divergent concepts, technical
> scoping) on 2026-07-20. Status: **sketch — not scheduled**.

## Scope

Exactly two jobs, UX quality first:

1. **Reading** notes from a kiln (markdown, wikilinks, backlinks, search)
2. **Interacting** with sessions (list, transcript, send, streaming, permission approvals)

No editing, no file management, no graph canvas (v1).

## The concept: Thread

The agent is the front door; notes are a layer you pull **over** the
conversation. The app opens into the most recent live session, not a home
screen. Reading never navigates away from the chat — a wikilink tap slides a
**reading peek** sheet up over the dimmed transcript; drag half for a skim,
full for study, down to return exactly where you were. Chains of wikilinks
re-render inside the peek with a breadcrumb back-trail; `⤢ open` promotes to
a full reading stack for real study; `➤ cite` drops `[[the note]]` into the
composer.

This was chosen over two alternatives — "Companion" (classic three-tab
notes/sessions/inbox split) and "Stream" (one unified reverse-chron feed
where sessions are peer cards with notes) — because Crucible's thesis is
knowledge-*grounded agents*: the agent is the product, notes are the
substrate. Thread is the only structure that encodes that, and a phone is a
resume-and-ask device. Stream's **pinned attention card** is grafted in (see
Approvals); its "orbit" graph gesture and Companion's note→session swipe are
held as v2 candidates (orbit: pull the peek past full height to bloom the
node's graph neighborhood).

## Surfaces

- **Main thread** — transcript of the active session. Assistant turns carry
  the ember left border; each turn's injected context renders as a collapsed
  purple **precognition band** (`pulled [[Auth]] · [[JWT]]`). Tool calls are
  collapsed chips ("Ran bash · exit 0"), expandable, never raw dumps.
- **Reading peek** — bottom sheet over the thread (half/full/dismiss).
  Backlinks are a collapsed, snippet-showing accordion at the note foot.
- **Sessions overlay** — slides from the left; Telegram-style rows: bold
  title + last-activity preview + relative time + **status pill**
  (running / awaiting-approval / done / error). Awaiting-approval floats.
- **Search** — `⌕` opens full-screen search (server-side semantic + title);
  `>` in the composer runs the same search inline, results pickable.
- **Composer** — pinned bottom, grows to ~5 lines, `[[` autocompletes note
  citations, send morphs to stop mid-stream. Model switch is a header chip.

## Approvals (the highest-stakes surface)

Three postures, one visual language (amber `--color-attention`):

1. **In-thread**: inline card at the pause point — tool name, full args in
   mono (never truncated), `⚠ writes <path>` with expandable diff when
   present. Buttons: **Deny / Allow once**, quieter "Allow for this
   session ›" scope row. Destructive-looking commands force the button path;
   swipe-to-approve (if kept) needs an undo grace.
2. **Elsewhere in app**: a pinned amber banner above the composer whenever
   *any* session is blocked ("1 action needs approval"), tapping through.
3. **App closed**: actionable push (future; needs secure context). Cold open
   always drains `GET /api/interactions/pending`, so missed requests re-pin.

## UX ground rules (from the research pass)

- **Poppable nav stack with restored scroll offsets** is the backbone of
  reading; OS back gesture pops the trail, never exits.
- **Auto-scroll only when pinned to bottom** (`isPinnedToBottom` flag);
  jump-to-bottom FAB with "N new" otherwise. Never yank a reading user.
- When a response starts, **pin the user's message near the top** and let
  the answer grow below it (ChatGPT pattern).
- Typography: 17px body, 1.5–1.6 line-height, ~16px side margins, ≤65ch
  column. Off-white on dark-grey, never white-on-black; slightly heavier
  body weight in dark mode; **desaturate ember for long link runs**,
  saturated ember only for small accents.
- Bottom thumb arc owns the frequent actions; ≥44px targets (wikilinks get
  padded tap areas); one scroll region per screen; vertical scroll only.
- PWA hygiene: `svh`/`dvh` never `vh`; `viewport-fit=cover` +
  `env(safe-area-inset-*)`; VisualViewport-aware composer; stale-SW
  refresh prompt; re-subscribe SSE on `visibilitychange` and reconcile via
  session history (mobile backgrounds kill EventSource silently).

## Technical shape (scoped against the repo)

**Second Vite entry point in `crates/crucible-web/web/` → `dist/m/`, served
by the same crucible-web server.** Same origin, same cookie auth, one build,
one rust-embed; the separate entry code-splits away the desktop stack
(WindowManager/solid-dnd/CodeMirror/d3-force/xterm). The embedded-asset
fallback learns `/m/*` → `m/index.html`; a second PWA manifest scopes to
`/m/`.

Reused unchanged: `lib/api.ts` (full typed client incl. SSE reconnect
layer), `contexts/chatEventReducer.ts` (pure SSE→state fold),
`lib/markdown.ts` + `callouts.ts` + lazy `shiki.ts`, DTO types, session/chat
contexts, attention store. The mobile shell writes only its own chrome.

**API gaps (small, additive):**
1. `GET /api/search/notes?kiln=&q=` with **server-side embedding** — today
   only `POST /api/search/vectors` exists and expects a client-computed
   vector (nothing calls it).
2. `limit`/`offset` (or a light projection) on `GET /api/notes` — currently
   returns the whole tree.
3. **QR login**: `cru web qr` printing a QR of `/m/#key=…`, consumed once
   into the cookie and stripped (keys never persist in URLs). Typing the
   API key on a phone is the current flow and it's hostile.

**LAN reality:** plain-HTTP LAN is **browse-only** — SW + PWA install
require a secure context, and the localhost exemption doesn't cover a phone
hitting a LAN IP. Home-screen install needs `cru tunnel` (HTTPS) or
mkcert. Fetch/SSE work fine over plain HTTP.

## v2 candidates

- **Orbit**: pull the reading peek past full height and the note lifts to
  center with its graph neighborhood (backlinks, outbound links, citing
  sessions) as flickable satellites. The most differentiated idea from the
  exploration; gated on the core loop proving out.
- Companion swipe (note → citing session), Web Push approvals, voice input
  (Whisper is already dynamically imported), offline note cache.

## Mockup

An interactive HTML mockup of the Thread concept (main thread, reading
peek, approval card, sessions overlay, pinned banner) accompanies this doc —
built as a phone-viewport artifact with the ember tokens.
