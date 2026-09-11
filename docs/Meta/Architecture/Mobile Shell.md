---
title: Mobile Shell
description: A draft design for the small-screen web shell — edge drawers, a tab stack, one tree of kilns and projects, a plain editor, a stepped new-session flow, an offline kiln with a sync, and what Oil views cost on a phone.
tags: [meta, architecture, web, mobile, ux, draft]
status: draft
---

# Mobile Shell

This note drafts the small-screen form of the web UI. **It elaborates a
decision already on record; it does not make one.** `docs/Meta/Product.md`
carries **Mobile Shell** at `P2`, and `docs/Meta/Product Decision Log.md`
records five choices dated 2026-08-13: a separate shell, one origin with no
`/m`, an online-only first pass, hash deep links, and a pinned manifest `id`.

`crates/crucible-web/web/PRODUCT.md` read "**Undecided:** the mobile layout"
until 2026-09-11, a month after the decision log settled it. An earlier revision
of this note took that line at face value and re-derived decisions the log had
already made, with weaker evidence. The line now points at the record. Section 2a lists where this draft now departs from
the record, so none of the departures is silent.

Nothing here is built yet. Read it as a proposal. See [[Web User Stories]] for
the story format that the work must add to, and [[State Stores]] for the store
conventions.

## 1. What the code holds today

These facts come from the tree at `crates/crucible-web/web`.

| Fact | Evidence |
|------|----------|
| The shell has no WIDTH breakpoints. | Zero Tailwind breakpoint prefixes in `src/`. It does carry `@media(hover:none)` rules, so it is not innocent of touch — only of width. |
| The shell is a window manager. | `src/components/windowing/WindowManager.tsx` |
| Rails are fixed-width and collapsible. | `EdgePanel.tsx` — 250 px default width |
| Panels come from one registry. | `src/lib/panel-registry.ts`, `register-panels.tsx` |
| Layout state lives in one store. | `src/stores/windowStore.ts` |
| The PWA already ships. | `vite.config.ts`, `src/pwa-options.ts` |
| The manifest asks for `display: standalone`. | `src/pwa-options.ts` |
| The viewport meta tag is correct. | `index.html` line 5 |
| The editor turns vim mode ON by default. | `src/lib/settings.ts` — `vimMode: true` |
| Settings persist in `localStorage`. | `src/lib/settings.ts` |
| The root roster groups Projects, Worktrees and Kilns. | `src/lib/tree-root.ts` — `buildRoster` |
| The session tree groups sessions under projects. | `src/components/SessionTree.tsx` |
| The new-session surface is a center tab. | `src/components/CenterComposer.tsx` |
| The daemon creates a session only on the first send. | `src/lib/draft-session.ts` |

Two facts decide the shape of this design.

The window manager needs a mouse. It needs drag and drop, split panes, tab
bars, floating windows, resize handles and hover chrome. A phone has none of
these inputs.

The PWA manifest pins `id: '/'`. `src/pwa-options.ts` explains why. A mobile
shell must therefore keep the URL at `/`. Section 6 gives the rule.

## 2. The decision

**Ship a second shell. Do not make the window manager responsive.**

The decision log settled this on 2026-08-13, and its evidence is stronger than
the argument below: **at 780 px the desktop centre column collapses to 29 px**
and the terminal renders one column. It is also cheap, for a reason this note
missed: `panel-registry` registers panels as bare components, `Pane` renders
them through `<Dynamic>` with no pane or tab context, and only two files import
`windowing/`. Every panel mounts in a second shell as it is.

The two shells share the panel components, the contexts, the API client and
the theme. They do not share layout.

Supporting reasons:

1. The window manager's whole value is many surfaces at once. A phone shows
   one surface at a time. A responsive window manager keeps the cost and
   drops the value.
2. `windowStore` models panes, tab groups, floating windows and drop targets.
   A phone needs none of them. A shared store grows dead branches.
3. Product principle 4 says the web UI is a peer, not a lite view. A separate
   shell obeys that principle better than a squeezed one. It can carry full
   depth in a form that a thumb can reach.

## 2a. How this draft meets the record

The P2 entry and decision row 78 describe the first pass as: "editor as the main
area, file tree and session as edge drawers with tabs but no tab MOVEMENT, a
kiln/project picker in the left drawer, no terminal, no vim mode". An earlier
revision departed from that in four places. Three are now resolved, on
2026-09-11, and the fourth is scoped out.

| The record | Resolved as | Where |
|---|---|---|
| File tree and session as the two drawers | **Adopted.** Sessions left, files right, each with tabs that never move | Sections 4, 7 |
| A kiln/project picker | **Adopted,** in the files drawer, above the tree. The sessions drawer gains its own **project switcher**, so a user can leave the recency list | Section 7 |
| No vim mode | **Changed:** vim is OFF by default on a phone, with its own setting. The decision log records the change | Section 8 |
| Online only | **Kept.** Sections 11 and 13 design the `P3` entries Offline Kiln Cache and Offline Note Capture. Build the shell without them | Sections 11, 13 |

**One detail the record leaves open, and this draft picks:** which side each
drawer takes. The record puts the picker "in the left drawer" without naming the
drawer. This draft follows the desktop rails — `sessions` registers `left`,
`files` registers `right` — so the picker sits on the right. The desktop already
offers **Swap Side Panels** for a user who wants the file tree under the other
thumb; the compact shell should honour the same preference rather than invent a
second one.

The record counted "all 15 panels" as mountable. There are 18 now:
`plugin-blocks` and `surfaces` arrived after it (section 10).

## 3. The switch

Add `src/stores/deviceStore.ts`.

```ts
// A phone, or a small window with a coarse pointer.
export const isCompact = () => /* matchMedia('(max-width: 767px)') */;
```

Rules for the switch:

- Read `matchMedia` once. Subscribe to the change event. `src/lib/theme.ts`
  already shows this pattern.
- Fall back to `false` where `matchMedia` is absent. Tests and embedded
  runtimes need this. `theme.ts` line 63 records the same need.
- `App.tsx` renders `<MobileShell />` or `<WindowManager />`. It renders one,
  never both. The providers stay above the choice.
- Do not run `loadLayoutOnStartup` or `setupLayoutAutoSave` in the compact
  shell. A phone must not overwrite the desktop layout.

## 4. The layout

Three regions, and one content surface.

```
┌──────────────────────────────────┐
│ ☰   Session title          ⋯  ▸ │  app bar, 48 px
├──────────────────────────────────┤
│                                  │
│                                  │
│          content surface         │  one panel, full width
│                                  │
│                                  │
├──────────────────────────────────┤
│  composer / editor toolbar       │  contextual, above the keyboard
└──────────────────────────────────┘
```

- **The app bar** holds the sessions-drawer button on the left. This is the
  minimum requirement. It holds the title in the middle. It holds an overflow
  menu and the files-drawer button on the right.
- **The left drawer is Sessions.** It switches projects and lists that
  project's sessions. Section 7 describes it.
- **The right drawer is Files.** It picks a root and shows its tree. Section 7
  describes it.
- **Each drawer carries tabs, and the tabs never move.** This is the record's
  "edge drawers with tabs but no tab MOVEMENT". The sessions drawer holds
  Sessions and Surfaces. The files drawer holds Files, Backlinks, Changes,
  Activity and Plugin Blocks — the right-rail panels of the desktop.
- **The content surface** shows exactly one registered panel.

Both drawers are overlays. They do not push the content. Each drawer uses
`min(85vw, 320px)`. A scrim covers the content behind an open drawer.

### Safe areas

The app bar and the composer must respect the device inset. `index.css`
carries no `env(safe-area-inset-*)` rule today. Add these tokens with the
mobile work:

```css
--inset-top: env(safe-area-inset-top, 0px);
--inset-bottom: env(safe-area-inset-bottom, 0px);
```

## 5. The drawers

### The gesture

- A swipe from the left edge opens the left drawer. A swipe from the right
  edge opens the right drawer.
- The edge zone is 20 px wide.
- A drawer follows the finger. It does not snap open on touch.
- Release past 40 % of the drawer width opens it. Release before that point
  closes it.
- A velocity above 0.5 px/ms wins over the position test.

### The button

The gesture is the preferred input. The button is the guaranteed input.

The app bar always shows the drawer button on the left. A user with a stylus,
a mouse or a screen reader must never depend on a swipe.

### The implementation

Use pointer events, not touch events. Pointer events cover a stylus and a
mouse with one code path.

- Set `touch-action: pan-y` on the content surface. The browser then gives the
  horizontal axis to the drawer.
- Animate `transform: translateX(...)`. Do not animate `width`. `EdgePanel.tsx`
  line 562 records the same reason for the desktop rails.
- Honour `prefers-reduced-motion`. `EdgePanel.tsx` line 578 shows the check.
- Trap focus inside an open drawer. Return focus to the button on close.
- Close the drawer on `Escape`, on a scrim tap, **and on the hardware back
  button** — a phone has no `Escape`, which leaves Android users only the scrim.
- **The drawer consumes back BEFORE the tab stack.** Push a history entry when
  it opens; pop it on close.

## 6. The tab stack, and the URL constraint

The compact shell keeps tabs. It draws one tab at a time.

Reuse `Tab` and `TabGroup` from `src/types/windowTypes.ts`. The compact shell
holds exactly ONE `TabGroup` and no panes. `activeTabId` names the visible
surface.

- The app bar shows the tab count on the right. A tap opens the tab overview.
- The overview is a card list, most recent first. Each card shows the icon, the
  title and a close button. A phone browser uses the same pattern.
- A tree tap opens a tab. The drawer then closes.
- The hardware back button moves to the previous tab in visit order. It does not
  close the tab.
- **It must eventually leave the app.** "Back never exits" traps a user with ten
  tabs behind ten presses. Walk the visit order at most once: when back reaches a
  tab already seen in this traversal, stop consuming and let the browser go.
- A tab with `isModified` shows a dot. Its close action asks first.
  `src/lib/tab-guards.ts` holds that RULE in `confirmTabClose` — but reuse the
  rule, not the implementation: it calls `window.confirm` (`:9`), which a
  standalone PWA may suppress outright. The compact shell asks with its own
  sheet.

Reuse the `Tab` TYPE, and expect no more than that from it. **The three openers
are welded to `windowStore`, so each still needs an `isCompact()` branch**:
`file-actions.ts:20` calls `openFileInGroup(editorGroupId(), …)`;
`draft-session.ts:58` calls `findDraftTab()`, `openTabBesideEditor` and
`windowActions.updateTab`; `session-actions.ts:58` likewise. Type reuse buys a
shared shape and shared tab chrome. It does not save the branch.

**One concrete bug this creates if unhandled.** `findDraftTab` scans only
`windowStore.tabGroups`, so the compact shell's draft is invisible to it and the
one-draft-at-a-time rule in section 9 fails silently — both shells can hold a
draft at once. Route the lookup through whichever store is active, or the
retarget path in `openDraftSession` aims at the wrong tab.

`windowStore` itself stays out. It models panes, splits, floating windows and
drop targets. The compact shell has none of them. It owns a small
`tabStackStore` over the same `Tab` type.

Persist the tab list under its own key. Never write the desktop layout key. A
phone must not overwrite a desktop layout.

**The URL must not change.** The service worker answers navigations only for
`/`. `pwa-options.ts` sets `navigateFallbackAllowlist: [/^\/$/]`. A new path
would miss the allowlist, reach the network, and break the offline shell. A
new path would also break `id: '/'` for an installed app.

**Deep links ride the hash.** Decision row 81 fixed this before anything
shipped, because published URLs are permanent: `/#note=…`, never `/note?…`. A
path-based link works online and fails offline, which is exactly when a
home-screen shortcut matters.

So the shell calls:

```ts
history.pushState({ depth }, '');   // no URL argument: the URL stays as it is
```

The state carries the depth. A `popstate` listener pops the stack.

**An earlier revision passed `location.pathname` as the third argument. That
strips the hash**, so the first tab a user opened would have erased the deep
link that brought them in. Omitting the argument keeps the whole URL, hash
included. At startup the shell reads the hash once to route a deep link, then
leaves it alone.

## 7. The two drawers

The record's split, adopted: sessions in one drawer, files in the other. They
follow the desktop's rails, where `sessions` registers `left` and `files`
registers `right` (`register-panels.tsx`).

### The sessions drawer (left)

```
┌ crucible ▾ ───────────────── ⊕ ┐   project switcher + New Session
│ INBOX  (2)                      │   every project: waiting or busy, last 24 h
│   ⏸ Review the link index   web │
│   ● Fix the release notes       │
│ CRUCIBLE                        │   the chosen project, most recent first
│   ○ Draft the mobile shell      │
│   ○ Port the kanban plugin      │
│   ○ …                           │
│ ▸ Archived  (4)                 │
└─────────────────────────────────┘
```

**The project switcher is the header, and it is what lets a user leave the
recency list.** A tap opens a bottom sheet with every project and worktree, plus
**All projects**. The list below then shows that project's sessions, most recent
first, with the rest reachable by scroll.

- It reuses `ProjectContext`'s `projects`, `currentProject` and `selectProject`.
  `selectProject` is browser-local — a signal and a remembered pin
  (`contexts/ProjectContext.tsx:123`) — so the phone's choice changes no other
  client and no daemon state.
- **All projects** shows the grouped `SessionTree` the desktop draws, one project
  tier per group. It is the fallback, not the default: on a phone, one project at
  a time is the readable form.
- **The Inbox ignores the switcher.** A session that waits on the user matters
  whatever project is on screen, so the Inbox stays cross-project, as it is on
  the desktop (`SessionsPanel.tsx`). Each Inbox row names its project.
- **⊕ New Session aims at the chosen project.** It calls
  `openDraftSession({ workspace })` with that project's path, so the draft opens
  already pointed there. Under **All projects** it opens unaimed, and step 2 of
  section 9 asks.
- Worktrees list as their own rows, labelled `mainrepo > rel/path`, as
  `buildRoster` names them. A session in a worktree belongs to the worktree.

### The files drawer (right)

A root picker above a tree — the desktop `FilesPanel` shape, and the record's
"kiln/project picker".

- **The picker** is `RootDropdown` over `buildRoster`
  (`src/lib/tree-root.ts:116`): three groups, **Projects**, **Worktrees** and
  **Kilns**. `RosterGroup.label` is that exact union (`:23`), so no group may be
  folded into another.
- **The tree** is `FileTreeView` over the chosen root. It is the tree view this
  draft set out to provide; the picker decides which root it shows.
- **It follows the active session** unless the user pins a root, exactly as
  `treeRootStore` does on the desktop. Open a session in `crucible-docs` and the
  tree shows `crucible-docs`.
- A file tap opens the editor on the content surface, then closes the drawer.

An earlier revision put sessions, projects and kilns in one tree with no picker.
That tree was long, and it ran against the record. The picker costs one tap and
keeps each tree short.

### Density rules

- Row height 44 px. **Cite the standard correctly**: WCAG 2.1 has no Level AA
  target-size criterion. 2.5.5 Target Size is 44x44 CSS px at Level **AAA**;
  WCAG 2.2's 2.5.8 Target Size (Minimum) is Level AA at **24x24**. `PRODUCT.md`
  commits to 2.1 AA, so 44 px exceeds the commitment rather than meeting it.
  Keep 44 px — it is the right number for a thumb — and do not justify it with a
  criterion that does not exist.
- **Hover-only actions are already handled — do not "fix" them.**
  `SessionTree.tsx:86` and `:467` carry `[@media(hover:none)]:opacity-100`, as
  do `Message.tsx:158` and `AssistantTurn.tsx:271`, plus `index.css:1199` and
  `:1289`. A coarse pointer already reveals them.
- A long press opens a bottom sheet with the row's actions. `FileTreeContextMenu`
  supplies the action list.
- Chevrons get their own 44 px hit area. A chevron tap expands. A row tap opens.

## 8. The editor

### Vim mode is a separate setting, off by default on a phone

**Decided 2026-09-11, and recorded in the decision log.** The P2 record said
"no vim mode". It now says vim is off by default on the compact shell, with its
own setting. A user who pairs a keyboard with a tablet can still turn it on.

`AppSettings.editor.vimMode` defaults to `true`. That default is right for the
desktop audience. It is wrong for a phone, because a phone has no `Escape` key
and no modifier row.

Add a second key rather than a shared one:

```ts
interface EditorSettings {
  /** Modal vim keybindings on a desktop shell. */
  vimMode: boolean;
  /** Modal vim keybindings on the compact shell. Off by default. */
  vimModeCompact: boolean;
  …
}
```

Default `vimModeCompact: false`.

**Show both toggles in the Editor section of Settings**, labelled for the shell
they govern — "Vim mode (desktop)" and "Vim mode (phone)". One unlabelled toggle
would change whichever key the current shell reads, and a user on a phone would
see a desktop-only switch that seems to do nothing.

Resolve the value at the call site. `FileViewerPanel.tsx` line 488 passes
`settings.editor.vimMode` to the editor. It must pass a resolver instead:

```ts
const effectiveVim = () =>
  isCompact() ? settings.editor.vimModeCompact : settings.editor.vimMode;
```

One boolean for both shells cannot work. The same browser profile serves a
laptop window and a phone-width window, and `localStorage` holds one value.

### Other compact editor defaults

| Setting | Desktop | Compact | Reason | Needs a split key? |
|---------|---------|---------|--------|--------------------|
| `vimMode` | true | false | No `Escape`, no modifier row | **Yes** — `vimModeCompact` |
| `autosaveSeconds` | 0 | 3 | A phone has no `Ctrl+S` | **Yes**, and the draft did not say so |
| `maxLineWidth` | 768 | 0 | The viewport is already narrow | **No** — derive it, do not store it |
| `showSaveButton` | true | true | unchanged | No |

**The split-key argument applies to more than vim mode, and the earlier draft
only followed it once.** `localStorage` holds one settings object per browser
profile (`web/src/lib/settings.ts:80-88`), so a laptop and a phone sharing a
profile share every value. That is the exact reason `vimModeCompact` exists, and
`autosaveSeconds` has the same problem: 3 seconds is wrong on a desktop and 0 is
data loss on a phone. Give it a key too.

`maxLineWidth` does not need one. A narrow viewport already clamps the column,
so the compact shell can ignore the setting rather than store a second value.
`showSaveButton` changes nothing and is listed only so nobody adds a key for it.

### The read/write switch

The desktop editor toggles live preview with `Mod-Shift-E`. The compact editor
shows a two-item segmented control in the app bar: **Read** and **Write**.
Read renders `MarkdownPreview`. Write opens CodeMirror.

The composer toolbar sits above the keyboard. It holds the wikilink insert,
the heading level, the list toggle and the save action.

**Two changes are already on master.** Both surfaces share
`renderFrontmatterCardHtml`, so Read mode inherits them free, and Write mode is
where the second one applies:

- A note may carry `properties: expanded` to open its own Properties card
  (`web/src/lib/frontmatter.ts:176`).
- `editor.hideFrontmatterGap` hides the blank lines between frontmatter and the
  first content in live preview. It is ON by default
  (`web/src/lib/settings.ts:43,88`). The reading view never
  showed the gap, because markdown discards leading blank lines.

A phone has less room for a gap than a desktop, so the default is right here and
needs no compact override.

## 9. The agent intro and the new-session flow

`CenterComposer.tsx` shows five chips, a prompt box and a model picker in one
row-based layout. That layout needs width. The compact shell breaks it into
three steps in one full-height sheet.

### Step 1 — the agent

A card list. Each card carries the agent icon, the name and one line of
description. `iconForAgent` in `src/lib/agent-icons.tsx` supplies the mark.
`listAgents` supplies the list.

The first card is **Crucible** — the internal agent. It is the default. A user
who taps Send at step 3 without a visit here gets it.

This step is the agent intro. It is the one screen that explains what each
agent is before a session starts. The desktop chip cannot do this, because a
chip has room for a name and nothing else.

### Step 2 — the context

Four rows, each with its current value on the right:

```
Project     crucible          ›
Kiln        crucible-docs     ›
Model       default           ›
Runtime     this machine      ›
```

Each row opens a bottom sheet with the options. Every row shows a resolved
default, so a user may skip the whole step.

Keep the two axes apart. **Workspace** answers where the files live.
**Runtime** answers where the process runs. `CenterComposer.tsx` lines 96–107
records why they are two controls.

Keep the empty-value contract. An untouched runtime is not the same as an
explicit `host` pick. The sheet must offer "Leave to the project" as a real
row.

### Step 3 — the prompt

A full-height text area. The Send button sits in the bottom bar.

### What stays the same

Lazy creation stays. Nothing reaches the daemon until the first send.
`draft-session.ts` records the contract, and the compact flow must keep it.

The compact flow opens the sheet as a tab of type `chat-draft`, exactly as the
desktop does. On send, it replaces that tab with the chat tab. `draft-session.ts`
already holds the one-draft-at-a-time rule, and the compact shell keeps it.

## 10. Panels on the compact shell

| Panel | Compact shell | Reason |
|-------|---------------|--------|
| chat | content surface | The primary surface. |
| chat-draft | full-height sheet | Section 9. |
| file | content surface | Section 8. |
| sessions | left drawer, Sessions tab | Section 7. |
| files | right drawer, Files tab | Section 7. |
| search | content surface | Results need width. |
| inbox | content surface | |
| activity | right drawer, a tab | A right-rail panel on the desktop. |
| backlinks | right drawer, a tab | A right-rail panel on the desktop. |
| changes | right drawer, a tab | A right-rail panel on the desktop. |
| settings | full-height sheet | |
| skills | content surface | |
| plugins | content surface | |
| graph | content surface, read only | Pan and zoom work. Node drag does not. |
| plugin-blocks | right drawer, a tab | Registered `right` (`register-panels.tsx:56`). A plugin block is working context. |
| surfaces | left drawer, Surfaces tab | Registered `left` (`register-panels.tsx:60`). A surface is a list of rows with a closed set of marks; it reflows to any width without help (section 12). |
| canvas | not offered | It needs drag and a large field. |
| terminal | not offered | It needs a keyboard. |

A panel that the compact shell does not offer must say so. It must not fail
silently.

## 11. An offline kiln, and the sync

A phone loses the network. The shell must still open a kiln and edit a note.

### The decision the record asked for, and what is now answered

The product record says Offline Kiln Cache is "**blocked on a decision, not on
effort**": caching kiln content puts authenticated responses in a
same-origin-writable store, which is the threat `pwa-scope.test.ts` pins. It
asks three questions.

**First, two words this section uses, in plain terms.** They are the two things
a phone keeps, and they are not the same kind of thing:

- **The mirror** is a copy of notes the phone downloaded. The daemon already has
  the originals. Delete the mirror and nothing is lost: the phone downloads the
  notes again.
- **The outbox** is edits the user made on the phone while offline, which the
  daemon has not received yet. They exist ONLY on the phone. Delete the outbox
  and the user's writing is gone.

A "wipe the cache on sign-out" rule treats both as cache. Only the mirror is.
Every rule below keeps that difference.

**1. What may be cached — answered.** The working set below: pinned notes, the
last 20 opened, and anything with an outbox entry. The index of note names is
cached for every kiln the user opens.

**2. For how long — still open.** A mirror with no expiry is a copy of the kiln
on a device that may be lost. Nothing here picks a limit.

**3. What happens on sign-out — answered, 2026-09-11: there is no sign-out, and
nothing is wiped automatically.**

The facts that make this simple:

- **The browser never holds the key.** It POSTs the key once, and the server
  answers with an HttpOnly, `SameSite=Strict` cookie that carries a minted
  session token, not the key (`crucible-web/src/routes/auth.rs:1-12`). The token
  lasts 30 days (`SESSION_TTL`, `middleware/auth/session.rs:26`).
- **The server can end a token** — `POST /api/auth/logout` exists — but the web
  client never calls it, and the compact shell does not need to.
- **Revocation already exists, at the key.** "A token … dies with the key it was
  minted from." Rotating `api_key` ends every browser session at once.

So the offline rules are:

- **A 401 during the drain keeps the outbox.** The token expired, or the key
  rotated. The shell shows the existing `AuthTokenPrompt`, the user enters the
  key, and the drain resumes. Unsynced writing is never discarded because a
  credential lapsed.
- **Nothing deletes the mirror or the outbox on its own.** Only the user does,
  from Settings, and the outbox's delete names how many unsynced edits it will
  destroy.
- **Rotating the key stops sync; it does not erase the phone's copy.** Notes
  already in the mirror stay readable by whoever holds the device. No offline
  cache can be erased remotely by revoking a credential, and the shell must not
  imply otherwise. What remote revocation of a lost phone should mean is
  **not yet decided** — open question 13.

One risk belongs beside rule 3. The outbox is durable and replays with the
user's authority **after the page that queued a write is gone**. A script that
reaches this origin once can queue writes that drain later. The same origin can
already write directly, so this adds persistence rather than capability — but
persistence is exactly what the service worker section of `pwa-options.ts`
spends most of its length bounding. Show the outbox's contents before a drain
that follows a fresh login, so a user can see what is about to be sent.

### What blocks this today

Four facts, each verified in the tree.

1. **The write endpoint has no conflict control.** `put_kiln_file` in
   `crates/crucible-web/src/routes/kiln.rs` calls `fs::write` on the whole
   file. It reads no `If-Match` header and no base hash. The last writer wins,
   and the loser gets no signal.
2. **The read endpoint returns content alone.** `get_kiln_file` answers
   `{ "content": … }`. It sends no hash and no modification time, so a client
   cannot record what it edited from.
3. **The hash the daemon already has is the WRONG hash.**
   `GET /api/notes/{name}` returns `content_hash`, but that is the *indexed*
   hash, written asynchronously by the file watcher. `put_kiln_file` does a bare
   `fs::write` and updates no record, and `routes/search.rs:409-412` states the
   rule for its twin: "The file watcher then runs the note through the daemon
   pipeline (real content hash…). We deliberately do NOT upsert a NoteRecord
   here."
   So the index hash LAGS the disk. A client that uses it as `base_hash` gets
   **false 409s right after every save** and **false 200s inside the reindex
   window**. Conflict control must hash the disk bytes inside the
   read-modify-write. That is work in the write path, not a field to expose.
4. **The service worker must not carry this.** `src/pwa-options.ts` defines no
   `runtimeCaching`, and `src/test/pwa-scope.test.ts` asserts that it stays
   undefined. `/api/*` is deliberately outside the worker's request surface.
   The chat SSE stream depends on that.

Fact 4 decides the mechanism. **The offline store is app-level, not a service
worker cache.** The worker keeps precaching the shell and nothing else.

`localStorage` cannot hold it either. `swrLocal` uses `localStorage` for small
catalog data, which is correct there. A kiln body set is too large for a 5 MB
synchronous string store. Use **IndexedDB**. The tree uses no IndexedDB today.

**The mirror copies `swrLocal`'s shape but cannot copy its guarantee.**
`web/src/lib/local-cache.ts:13-22` applies the cached value SYNCHRONOUSLY,
before first paint, and that is the whole reason it exists — "A hard reload was
showing 'Loading…' text all over the shell". IndexedDB is asynchronous, so the
first frame has nothing. Keep the small catalog reads on `localStorage` for the
warm start, and let IndexedDB hold only what `localStorage` cannot: note bodies
and the outbox.

### The prerequisite daemon change

Ship this BEFORE any offline write. It belongs in the daemon, because the
daemon owns business logic.

```
GET  /api/kiln/file  → { content, content_hash }
PUT  /api/kiln/file  ← { path, content, base_hash }
                     → 200 { ok, content_hash }
                     → 409 { current_hash }   when base_hash != the disk hash
```

`base_hash` stays optional. An absent `base_hash` keeps the present blind
overwrite, so the desktop editor does not change on the same commit.

**Three routes blind-write, not one.** Gating only the first leaves two doors
open:

| Route | Blind write |
|---|---|
| `PUT /api/kiln/file` | `routes/kiln.rs:353` |
| `PUT /api/notes/{name}` | `routes/search.rs:415` |
| `PUT /api/canvas` | `routes/canvas.rs:172` |

The daemon compares the hashes. The browser must never make that decision. A
client-side test would be a second copy of the rule, and it would run on the
one machine with a stale view of the disk.

### The three stores

```
mirror   path → { body, baseHash, mirroredAt, lastOpenedAt, pinned }
outbox   path → { body, baseHash, queuedAt }
index    kiln → { NoteEntry[], indexedAt }
```

### Index everything. Mirror a working set.

An Obsidian user keeps a few notes hot and the rest cold. The mirror follows
that shape, and the two halves have very different costs.

**The index is small, so mirror all of it.** `listNotes` returns `NoteEntry` —
name, path, title, tags and `updated_at`. No body. A kiln of 5000 notes indexes
in well under a megabyte. So the whole tree browses offline, and every note is
visible.

**A body is large, so mirror only the working set.** The set is:

```
pinned  ∪  the last 20 opened  ∪  anything with an outbox entry
```

Cap it, evict by least-recent-open, and never evict a pinned note or one the
outbox still holds.

**Seed the set from the server, not from this device.** `/api/recents` is
already the source of truth for recents, and `src/lib/recent-files.ts` records
why: the list lives next to the layout blob so it survives across browsers and
ports. It therefore crosses devices already. A phone that has never opened a
note still mirrors what the desktop opened this week. `MAX_RECENTS` is 20
today, which is a reasonable first cap.

**Say which notes are available.** A cold note in the tree draws dimmed with a
"not downloaded" mark. A tap on it while offline says so, and offers to fetch it
when the network returns. A row that looks the same and then fails is the
failure mode this rule exists to stop.

**Let the user pin.** A long press on a note offers "Keep offline". That is the
one control the heuristic cannot replace, and Obsidian's own mobile users reach
for it.

Rules:

- A read fills the mirror. An open note reads the mirror first, then corrects
  from the network. This is `swrLocal`'s rule, over IndexedDB.
- **A refresh MUST skip any path the outbox holds, and a queued entry's
  `baseHash` is immutable.** This is the clearest data-loss path in the design.
  Without the rule: edit offline; reconnect; open the note, so the mirror
  corrects to the other writer's hash H1; the drain then reads `baseHash` from
  the mirror, the server compares H1 to H1, returns 200, and **the other
  writer's change disappears with no conflict copy.** The base is what the user
  edited FROM. Nothing may move it afterwards.
- An offline save writes the outbox. It copies `baseHash` at queue time.
- The app drains the outbox when the network returns. It sends `base_hash`.
- A 200 answer updates the mirror, then clears the entry — in that order.
- **One queue, ordered per path.** A body write and an anchored batch for the
  same note must drain in the order they were made, or the batch's anchors run
  against text the body write has not yet laid down. Keep one outbox with a
  sequence number, not two queues by kind.

### The conflict rule

On a 409 the app writes a **conflict copy** beside the note:

```
Release Notes.md
Release Notes (conflict, phone, 2026-09-09).md
```

The user then merges by hand. Obsidian resolves a sync conflict the same way.

**Clear the outbox entry only after the conflict copy lands.** The copy is a
second network write, and if it fails after the entry is cleared, the user's
text is gone — the exact loss this section exists to prevent. If the
conflict-copy path itself collides, suffix it and retry; never overwrite.

**No CRDT.** State the reason plainly: the truth is markdown bytes on disk,
which the agent, the TUI, the CLI and any editor may rewrite at any moment. A
CRDT needs every writer to speak it. Most writers here never will. A conflict
copy loses nothing and needs no writer to co-operate.

### What offline does NOT do

An offline kiln is a note store. It is not a Crucible.

| Offline | Reason |
|---------|--------|
| Read a note in the working set | The mirror holds its body. |
| Edit a note in the working set | The outbox holds the write. |
| Browse the WHOLE kiln tree | The index holds every note, body or not. |
| **No cold note** | Its body was never fetched. The tree says so. |
| **No agent turn** | The model call, the tools and the turn loop are daemon-side. |
| **No search** | `grepSearch` and `semanticSearch` are daemon calls. |
| **No graph, no backlinks** | `link_index.rs` is daemon-side SQLite. |
| **No note in an unindexed kiln** | The app does not know that kiln's shape. |

The app bar must show an offline marker and the outbox count. A queued write
that looks saved, and is not, is the failure this whole section exists to
prevent.

## 12. Plugin blocks on a phone

**This section changed twice: the decision was made, and then it shipped.** An
earlier draft argued Oil against a native component and left the choice open.
The choice is settled and **the work is on master** — `web/src/components/oil/`
is gone and `web/src/components/blocks/` is in its place. Read
`docs/Meta/Analysis/The Plugin Contract.md` for the design and
`docs/Meta/Analysis/Plugin API Plan.md` for the sequence.

The rule is one sentence: **a plugin owns data, and each frontend draws it
natively.** A plugin publishes opaque JSON through `cru.plugin.publish`; the
browser draws it with a TS component, and falls back to a plain table when the
plugin ships none.

On master today: `KanbanBlock`, `GraphBlock`, `GenericBlock`, `PluginBlock`,
`PluginBlockPanel`, `PluginCommandDialog`, plus `registry.ts` and
`usePublication.ts`. The command dialog means the typed-parameter step landed
too, so a frontend can offer a primitive it has never seen.

### What that settles for this draft

Three worries in the earlier draft are now closed, and the resolution is better
than the workaround it proposed.

| Earlier worry | Outcome |
|---|---|
| A board a thumb drags — `Node::Action` has no drag | `KanbanBlock.tsx` has real drag and drop |
| A view cannot respond to width | The component owns layout; columns wrap |
| Terminal colours, not theme tokens | The component uses theme tokens |
| A tap target has no size | The component sets it, so the 44 px floor in section 7 applies normally |

A phone therefore needs no new plugin mechanism. It needs the components to be
built responsively, which is an ordinary front-end requirement and not an
architectural one.

### Lua still does not run in the browser

The conclusion survives the pivot, by a shorter route than before. A plugin
publishes **data**; it does not ship a tree to render and it does not ship code
to run. `POST /api/plugins/command` invokes a primitive that runs on the daemon.

So: no VM, no WASM Luau runner, no change on a phone. The argument against a
browser VM is unchanged and is not about the CSP — `script-src` already carries
`'wasm-unsafe-eval'` for shiki and the local Whisper. It is that
`crucible-lua/src/modules.rs` owns module lookup because lookup is import
authority, and a browser VM is a second one.

### Offline is the seam between the two designs

The plugin contract's write path is online by construction:

> A frontend never edits the plugin's data. It invokes a plugin **command**,
> the plugin performs the write and republishes, and every client redraws from
> the push.

Offline there is no daemon, so there is no command, no write and no republish.
The plugin plan says so and scopes it out:

> **Offline queueing.** Refusing offline is cheap and belongs with the first
> button; an outbox, replay and conflict resolution is separate product work
> whose failure mode is silent data loss.

**This draft is that separate work.** The two designs meet here and nowhere
else, so keep the split clean:

- **Refusing offline belongs to them, and ships first.** `navigator.onLine`
  plus a failed fetch settles it. A primitive that cannot run greys out and says
  why.
- **Queueing belongs to section 11.** It is an outbox, a replay and a conflict
  answer.
- **A read degrades on its own.** A publication is data, so the last one caches
  and redraws read-only with an offline marker. That is cheaper than the Oil
  tree cache the earlier draft proposed, because there is no tree to
  re-evaluate.

**Do not queue a plugin command.** The reason is a property of the two
primitives, not a policy, which is what makes the rule durable.

An anchored batch **carries its own conflict test**. Each `{expect, replace}`
re-validates against whatever the file says at replay time, so a stale edit
fails loudly instead of clobbering.

A command carries none. Replaying `kanban_move("alpha.md", "doing")` an hour
later cannot tell whether the ticket already moved, was renamed, or was deleted.

So: queue *note* writes, which section 13 makes anchored and checkable. Refuse
commands, and grey the control with a reason.

### Surfaces: the second plugin UI kind, and the easy one for a phone

Master also carries **plugin surfaces** (`cru.surface`,
`docs/Help/Extending/Scripted UI.md`). A block is embedded in a note; a surface
is a panel the plugin declares, drawn full-screen by `:surfaces` in the TUI and
by the **Surfaces** panel in the browser.

A surface row carries `id`, `text`, an optional `detail`, and an optional `mark`
from a closed set — `busy`, `blocked`, `ok`, `failed`. **The client picks the
glyph.** That is the shape a phone wants most: a list reflows to any width, a
screen reader reads rows as rows, and a mark the build does not know renders
blank rather than as a fault. `set_rows` broadcasts `surface_changed` with the
version and never the rows, so a phone on a slow link refetches only what it
opens.

Offline, a surface degrades the way a publication does: the last rows cache and
redraw read-only. A surface has no write path of its own, so it needs nothing
from section 11.

### One live finding, now confirmed from two directions

Section 12's earlier CSP finding stands, and the plugin plan reached the same
place independently. Both are about the same missing boundary.

- This draft: `server.rs:241` sets `script-src 'self' 'wasm-unsafe-eval'`, so a
  plugin bundle the daemon serves IS `'self'`. The policy admits it and protects
  nothing. The service worker's root scope leans on that same control.
- The plugin plan, step 1: "**Until blocks are isolated**, a header is
  forgeable by any script on the origin, so this is not a security boundary"
  (`Plugin API Plan.md:57`). Keep the conditional — the claim is about today,
  not forever. Its own words: "Say that in the code, or someone will later
  believe it is a gate."
- The plugin plan, step 4: `Capability` has ten variants and is enforced in two
  places, both `InterceptTools`. `filesystem`, `kiln` and `config` gate nothing
  today.

The three describe one gap. **Nothing separates plugin code from app code on
this origin**, and every proposed control assumes an isolation that does not
exist yet. `Plugin API Plan.md:124` leaves the fix undecided — "a sandboxed opaque origin
with a `postMessage` bridge, versus something else". Only half of that exists here, and the two exemplars are not the same strength:

- `routes/kiln.rs:209` returns `sandbox; frame-ancestors 'none'` — **no
  `allow-*` token at all**, so the document gets a unique opaque origin and its
  script "cannot reach the API, the session cookie, or the app's DOM".
- `canvas/CanvasNodeView.tsx:133` uses
  `allow-scripts allow-popups allow-popups-to-escape-sandbox allow-forms`. Still
  an opaque origin, because `allow-same-origin` is absent — but scripts DO run.
  This is the closer precedent for a plugin bridge, and the weaker one.

**`postMessage` appears exactly once in `crates/crucible-web/web/src`, and it is
a comment saying the bridge does not exist**: `blocks/registry.ts:18` — "…
postMessage bridge before it can be loaded. Until that exists, a plugin that …".
Both exemplars above are one-way renders. The earlier draft cited them as
precedent for a bridge; they are precedent for isolation, which is a different
claim.

Nothing in this draft depends on that isolation landing. It is recorded because
a mobile shell adds a service worker to the same origin, and a cache that
survives a reload is a worse thing to lose than a tab.

## 13. Markdown as a primitive

Name the class that wants client computation: a set of markdown files used as
records. A todo list. A kanban board. Movie reviews. A task plan. A view reads
the set, arranges it, and writes one field back.

This class looks like it needs a runtime in the browser. It does not.

### The class reduces to two operations

The kanban plugin on `spike/oil-document-blocks` is one instance of it, and it
is small enough to read whole. Its two halves are:

- **Read** — `tickets()` lists a folder, reads each `.md`, and hand-parses two
  scalar keys out of the frontmatter block.
- **Write** — `move()` runs one substitution:
  `content:gsub("(\nstatus:%s*)[%w_-]+", "%1" .. to, 1)`.

The plugin's own comment says why it rewrites a LINE and never re-emits the
document:

> Re-emitting is what made the old todo-list writer delete every heading in the
> file, and a ticket's body is the part a human wrote.

So the whole class is two operations:

1. **A query over the index** — filter, sort and group by frontmatter fields,
   and by marked lines. See "Two things called a task plugin" below.
2. **A small batch of line edits on one note** — one or two lines, applied
   together.

Neither is arbitrary computation. Both are data operations.

**One line is the common case, not the rule.** Real task plugins change one or
two lines at a time:

| Action | Lines |
|--------|-------|
| Move a ticket | `status:`, and often `updated:` — two |
| Check a task | the checkbox line — one |
| Complete a recurring task | the checkbox line, plus the next occurrence after it — one edit, two lines out |
| Archive a task | remove the line here, append it there — two files |

A design that writes exactly one field cannot express the third row, and it
writes the first row as two calls that can half-apply. The unit is a **batch**.

### Two things called a task plugin

The name covers two features. Only one of them belongs to this section.

**The agent's own task list is render, not data.** An agent that keeps a todo
list while it works reports that list; the UI draws it. It writes nothing to a
kiln, it needs no query, and no operation in this section applies to it. On a
phone it matters for a different reason: "what is the agent doing right now" is
the primary glance, and an ordered task list is the best form that glance takes.
Crucible still carries no task or plan event — the session event vocabulary has
no such variant, and neither does the web's event reducer. **Session STATUS,
though, now has a render channel.** `EventName` gained daemon-wide
`SessionCreated` and `SessionEnded` (`session:created`, `session:ended`), and
`runtime/plugins/session-board/` publishes a surface of sessions with `busy`,
`blocked`, `ok` and `failed` marks. On a phone that is most of the glance: which
session is working and which is waiting on you. A task list would be the next
level of detail, and it would arrive the same way — as rows a plugin publishes,
not as a new panel.

**A kiln-wide checkbox collation is a query.** "Show me every unchecked box in
this kiln, grouped by note" is operation 1. It is Dataview-shaped: a view over
notes the user wrote, with no state of its own.

The parser half already exists. `CheckboxStatus` in
`crucible-core/src/parser/types/lists.rs` is canonical and carries five states:

```
[ ] Pending   [x] Done   [/] InProgress   [-] Cancelled   [!] Blocked
```

The INDEX half does not. `notes` stores `tags` and `properties`; `note_blocks`
stores one row per top-level block. A task list is one list block carrying its
items, and `markdown_it/converter.rs` says so in a test name:

> `a_task_list_is_one_list_block_carrying_its_items` — Checkbox state is not a
> block property.

So a checkbox query needs an item-level index that no table holds yet. That is
a daemon change of a different size from the two below, and this draft does not
scope it.

**Its write side is the easy half, and it needs nothing new.** Checking a box
from a collated view is one anchored edit against the note that owns the line.
Three boxes in three notes are three independent calls, so per-file atomicity is
enough and open question 10 does not apply.

**Its offline behaviour is the honest limit of section 11.** A kiln-wide
collation is a daemon query over every note. Offline, the client sees the
working set and nothing else, so the same view returns a smaller list. It must
say "12 of your notes are not downloaded" rather than quietly showing fewer
tasks. This is the one place where the working-set model visibly under-delivers,
and it is worth stating before someone builds the view.

### The daemon already indexes half of it

`crates/crucible-daemon/src/storage/sqlite/note_store.rs` declares:

```sql
CREATE TABLE IF NOT EXISTS notes (
    path TEXT PRIMARY KEY,
    content_hash BLOB NOT NULL,
    …
    tags TEXT NOT NULL,
    properties TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

There is a general property store beside it, keyed
`(entity_id, namespace, key)`. Frontmatter is indexed today.

**Two routes, and the draft named the wrong one for four revisions.**

- `GET /api/kiln/notes` maps through `note_to_file_json` and returns
  `{name, path, is_dir}` only (`routes/helpers.rs:42`, called at
  `routes/kiln.rs:79`).
- `GET /api/notes` maps through `note_to_metadata_json` and returns
  `{name, path, title, tags, updated_at}` (`helpers.rs:27`). **This is the
  index route.** `listNotes` calls it (`web/src/lib/api.ts:1829`).

Neither carries `properties`, and no type in the chain has a slot for it.
`NoteListItem` is a five-tuple (`helpers.rs:22`); `NoteInfo` carries
name/path/title/tags/created_at/updated_at
(`crucible-core/src/traits/knowledge.rs:68`).

**So this is not one field.** It reaches `crucible-core`, the daemon RPC
(`rpc_client/client/storage.rs:248`), the tuple, and the JSON. Size it as four
layers.

### `properties` cannot ship raw — it is a containment stamp

`properties.scope` holds `{"kind":"workspace","path":"/abs/host/path"}`, and
that field IS the same-workspace SQL visibility filter
(`crucible-daemon/src/storage/sqlite/note_store.rs:176`).

Returning the column verbatim would put an absolute host path on the wire and
hand a client the key to the check that hides other workspaces' notes.

**This column is not data with a sensitive field in it. It is the predicate.**
Returning it verbatim hands a client the same-workspace check itself, not merely
knowledge that one exists.

So: **allowlist at the SERIALISATION boundary, with a test that fails on an
unknown key.** A caller gets the keys a note's author wrote — `status`,
`rating`, `due`. Everything the daemon stamps stays behind. A denylist inverts
the failure: the next field added to `scope` would ship by default. A test that
fails closed on an unrecognised key is what keeps that true a year from now.

### The two operations to add

Both belong to the daemon, and both serve the desktop equally.

1. **Carry `properties` on the note index.** `GET /api/kiln/notes` returns the
   column it already stores. This is the query half, and it costs one field.
2. **Add a batched line edit.**

```
PATCH /api/kiln/file
  ← { path,
      edits: [
        { expect: "status: todo",   replace: "status: doing" },
        { expect: "updated: 09-01", replace: "updated: 09-09" }
      ],
      base_hash?: "…" }
  → 200 { ok, content_hash }
  → 409 { failed: [ { index: 0, matches: 0 } ], current_hash }
```

Four rules make this safe.

- **Anchor on text, never on a line number.** An agent that inserts a line above
  invalidates every number below it. It does not invalidate the text.
- **Match whole lines, not substrings.** A bare substring both over-matches and
  under-matches, and the kanban `gsub` already guards the first case: it anchors
  on a leading newline AND a terminating character class
  (`runtime/plugins/kanban/init.luau:159`). Without that, `expect: "status:
  todo"` also matches `status: todoish`, and matches inside a fenced code block
  or in prose. Worst case: the frontmatter already reads `status: doing` and a
  code fence reads `status: todo` — exactly one match, wrong region, silent
  corruption. **Exclude fenced regions**, and align on line boundaries.
- **Each `expect` must match exactly once — and carry a tiebreak when it
  cannot.** Two identical `- [ ] Buy milk` lines give two matches and a refusal,
  which breaks the single most common case in the table above. An edit therefore
  carries an optional `occurrence` index. Neighbouring-line context is more
  expressive and was considered; it loses, because it makes a queued edit
  unreadable in an outbox a human may have to inspect. Zero matches still
  refuses. An ambiguous match with no tiebreak still refuses.
- **Normalize line endings, preserve the file's own.** Compare with `\n`, and
  write back the ending the file already used. A multi-line `expect` otherwise
  never matches a CRLF file. Trailing whitespace is significant: `status: doing `
  is not `status: doing`.
- **Apply against the ORIGINAL text, refuse overlapping spans — and note that
  the already-applied rule below depends on this, not merely coexists with it.**
  Resolve every anchor against the file as read, then splice by offset, then
  refuse if two spans overlap. Sequential application breaks two things at once:
  a later `expect` can match text the batch itself wrote, so its match count is
  taken against a document that never existed on disk; AND the already-applied
  check stops working, because it can no longer tell its own write from another
  writer's. **Anyone "optimising" to sequential application silently converts a
  sync no-op into a false conflict.** These are one rule, written together on
  purpose.
- **All edits apply, or none.** One read, one write, one answer. This is what
  the first and third table rows above need, and it is why the batch is the
  unit.
- **"Already applied" is not "moved on".** Both give zero matches, so a replay
  cannot tell them apart without a rule — and the *normal* offline success path
  is the one that looks like a conflict. The rule: if `replace` is already
  present at the anchor site, treat that edit as applied and succeed. Without
  it, a second device syncing the same change writes a spurious conflict copy
  every time. This is the second half of the original-text rule above; it has no
  independent existence.
- **`replace` may carry newlines.** One edit turns one line into two, which is
  how a recurring task writes its next occurrence.

**`base_hash` reports; it does not gate.** Since conflict control has to hash
the disk bytes inside the read-modify-write anyway, the anchors already do
detection. What the hash adds is a better message: it separates "the file moved
on" from "your anchor moved on", which is the difference between showing a user
a conflict copy and showing them one failed edit. Keep it for the message. Do
not build the gate on it.

A frontmatter field set is then sugar. The daemon may still accept
`set: { status: "doing" }` and compile it to an edit, including the
create-the-key-inside-the-existing-block case that `move()` handles today.

**Containment is already in one place — inherit it, do not rebuild it.**
`move()` still hand-validates its `file` argument against `/`, `\` and `..`,
because a rendered tree carries params a note author can hand-write. That check
predates `scoped()`. An anchored edit routed through the same `scoped()` path
gets the guarantee for free, and the hand-rolled check can go.

**On block addressing.** `note_blocks` already stores
`(note_path, span_start, span_end, kind, content_hash)`, and `BlockHash` is the
repo's one content hash. That is the same idea at a coarser grain: a block is a
paragraph or a list, not a line. Anchoring on the expected text needs no new
index and no new migration, so prefer it. Revisit block anchors if a caller
needs to address a block it cannot quote.

### How this meets the plugin contract

Two write paths, and they do not compete.

| | Online | Offline |
|---|---|---|
| **Plugin-owned data** (a board move) | `POST /api/plugins/command` → the plugin writes → republishes | refuse, and say why |
| **A note the user owns** (a checkbox, a body edit) | `PATCH /api/kiln/file` | queue the anchored batch |

The split is **authorship, not file type**. Kanban's tickets are markdown notes,
and a user may edit one in the editor — that is the second row. The same file
moved by the board is the first row, because `kanban_move` is the plugin's write
and only the plugin knows what it meant.

**A query needs no such split.** Reading is not authorship. A client filtering
the mirrored index does the same thing online and offline, and returns a shorter
list offline because the mirror is smaller.

**Authorship is definitional, not inferred.** A write arriving through
`PATCH /api/kiln/file` is user-authored; a write arriving through a command is
plugin-authored. The path decides it, so there is no classification step that
could get it wrong.

### Field ownership is not decidable, and that is acceptable

Nothing stops a user's anchored batch from rewriting `status: doing` in a ticket
file, bypassing `kanban_move`. The file cannot say that one line belongs to a
plugin's state machine and the rest belongs to the person. Same bytes.

This is fine for kanban, and the reason names an invariant rather than an
exception. **Kanban owns the transition logic, not the bytes.** It re-derives
its board from the files on the next read, so a user who edits `status:` by hand
has made a legal move that the plugin observes.

> **The invariant: a plugin's published state must stay derivable from its
> files.** A plugin whose state is not recoverable that way desynchronises
> silently the first time a user edits the file in an editor.

**This invariant is not in `The Plugin Contract.md` today.** It was agreed in
correspondence and may land there; until it does, treat it as this draft's
assumption about another design, not as a citation.

It constrains future plugins rather than this one. It also constrains offline directly: an offline note edit
reaches a plugin's files with the plugin not running, so a plugin that fails the
invariant fails hardest exactly here. The honest test for such a plugin is
whether it survives its own user's text editor.

### The primitive belongs in `cru.fs`, not only in HTTP

**Asked for in correspondence with the plugin work, not in its published docs.**
Those ask for the more general thing: a scoped read/write of any shape
(`The Plugin Contract.md:227-231`) and "a refusal a UI can act on"
(`:225-226`). This design is one answer to both, and it is not yet written into
either document. Do not read it as a settled cross-branch dependency.

**Half of this gap closed on master, and the half that matters did not.**

`cru.fs` now has scoped `read` and `write` (`crucible-lua/src/fs.rs:225,240`),
each routed through `scoped(lua, &path, …)` so the call is confined to the
kilns, the workspace and that plugin's own state directory.
`docs/Help/Lua/Language Basics.md:66-68` states the reason: "`io.open` says
nothing about who is calling or where they may reach."

So the containment argument this section used to make is **already answered** —
containment IS in one place now, and it is `scoped()`. Do not re-argue it.

What did NOT close is conflict detection. `cru.fs.write` takes a whole file, so
a plugin still does read-modify-write with no base and no check. Kanban rewrites
one `status:` line with a `gsub` and writes the whole file back
(`runtime/plugins/kanban/init.luau:159`). That is last-write-wins. A phone that
moves a ticket while an agent rewords the body loses exactly the way section 11
describes.

So the anchored batch is one primitive with two callers, and it now sits
**beside** an existing scoped pair rather than filling an empty namespace:

```
cru.fs.read / cru.fs.write   -- on master, scoped, whole-file, no base
cru.fs.edit(path, edits)     -- ADD: the plugin's checked write
PATCH /api/kiln/file         -- the editor's write, and the outbox replay
```

An HTTP-only version leaves the two halves of the app disagreeing about what a
safe write is, and leaves the plugin half on the unsafe side. Build the core
operation once and give it both callers.

`cru.fs.edit` is a smaller ask than it was: the scoping, the path resolution and
the declaration pattern all exist and it follows them.

### The refusal must be structural

Also asked for in correspondence, and it composes with all-or-none.

A batch that refuses knows exactly why: which edit index failed, and whether its
`expect` matched zero times or more than once. Return that, rather than an error
string a UI has to parse.

```
409 { failed: [ { index: 0, matches: 0 } ], current_hash: "…" }
```

`FsMoveOutcome` is the shape the plugin work names, but note what it is: a
TypeScript client interface, `{moved, rewritten_sources?, skipped?}`
(`web/src/lib/api.ts:2159`). There is no Rust type behind it, and it
enumerates no per-item failure. Copy the *spirit* — a structured outcome a UI
can branch on — and design the Rust shape here. An anchored batch is the first operation that can answer the
question properly, so it should set the pattern rather than inherit the gap.

### What this primitive does NOT buy: precision

Recorded because an earlier draft, and correspondence with the plugin work,
both overstated it in the same direction.

The batch is better than a whole-file write on its **conflict unit**. It is
worse than kanban's `gsub` on its **anchor precision**, and the two properties
come from different places:

| Property | Comes from | This primitive |
|---|---|---|
| Precision | a line-anchored pattern | must be built in — see the rules above |
| Detection | re-validating the anchor at apply time | **this is the contribution** |

`kanban_move` anchors on `"(\nstatus:%s*)[%w_-]+"` limited to the first
occurrence (`init.luau:159`) — a leading newline, the key, and a character class
that terminates the value. A bare-substring `expect` is strictly weaker.

**Do not adopt the batch expecting kanban's precision to come along.** A caller
that replaces a hand-tuned pattern with a naive `expect` trades accuracy for
detection, which is not the trade this section proposes. The precision rules
above are load-bearing, not polish.

### A line edit is also the better conflict unit

This reaches back into section 11. A whole-file `PUT` conflicts with ANY
concurrent edit, so a phone that moved one ticket loses to an agent that
reworded the body.

An anchored edit conflicts only when the anchored text changed. Moving a ticket
while the agent rewrites the ticket body is then not a conflict at all — the
common case for exactly this class.

So the offline write path has two units, not one:

| Write | Base | Conflicts when |
|-------|------|----------------|
| A body edit from the editor | `base_hash` | the file changed at all |
| A line batch from a view | each `expect` | that text moved on, or doubled |

The two compose. A view that wants both sends `base_hash` AND the anchors.

**A cross-file move is not atomic, and this draft does not make it so.** The
archive row above touches two files, so it is two calls. The second can fail
after the first succeeded, and a task then exists twice or not at all. Either
the operation grows a two-file form, or the caller repairs it. Open question 10
carries this.

## 14. The file plan

New files:

```
src/stores/deviceStore.ts          the compact test and its subscription
src/mobile/MobileShell.tsx         app bar, drawers, content surface
src/mobile/Drawer.tsx              the gesture, the scrim, the focus trap
src/mobile/NavStack.ts             the popstate bridge for the back button
src/mobile/SessionsDrawer.tsx      project switcher, cross-project Inbox, sessions
src/mobile/FilesDrawer.tsx         RootDropdown + FileTreeView; the right-rail tabs
src/mobile/BottomSheet.tsx         the option pickers and the action menus
src/mobile/NewSessionSheet.tsx     the three steps in section 9
src/mobile/MobileEditorBar.tsx     Read/Write and the toolbar
src/mobile/TabOverview.tsx         the tab card list
src/stores/tabStackStore.ts        one TabGroup, no panes
src/lib/offline/db.ts              the IndexedDB schema: mirror, outbox, meta
src/lib/offline/mirror.ts          read-through the mirror, then the network
src/lib/offline/outbox.ts          queue a write, drain it, handle a 409
src/lib/offline/conflict.ts        the conflict-copy name and the write
src/lib/offline/working-set.ts     pins, the recents seed, the cap, eviction
src/lib/offline/query.ts           filter/sort/group the mirrored index
src/lib/offline/line-edit.ts       queue an anchored batch, drain it, re-anchor
```

Changed files:

```
src/App.tsx                        pick the shell
src/lib/settings.ts                add vimModeCompact and the compact defaults
src/components/FileViewerPanel.tsx read the resolver, not the raw setting
src/components/settings/sections.tsx  a compact section
src/index.css                      the safe-area tokens
src/lib/api.ts                     carry content_hash and base_hash
crates/crucible-core/src/…               NoteInfo gains an allowlisted properties map
crates/crucible-daemon/src/…             the anchored-edit CORE operation
crates/crucible-lua/src/fs.rs            cru.fs.edit — signatures live here (:85-190)
crates/crucible-lua/src/host_api.rs      the declaration the ratchet test requires
crates/crucible-web/src/routes/kiln.rs   PATCH, the 409, the hash fields (thin)
crates/crucible-web/src/routes/search.rs `properties` on GET /api/notes
```

**The core operation lives in `crucible-daemon`, not in an Axum route.**
Section 11 says the daemon owns this logic, and section 13 asks for two callers;
an operation inside `routes/kiln.rs` cannot serve the Lua one. The route is a
thin adapter over it.

`cru.fs.edit` is not free on the Lua side either. `cru.fs` is declared in
`crucible-lua/src/fs.rs` (`mkdir` at `:90`, `remove_all` at `:118`), and
`crucible-lua/src/host_api.rs` holds `UNSIGNED` behind a ratchet test
(`crucible-daemon/tests/plugin_stubs_contract.rs`): a VM function that
is neither declared nor listed **fails the build**.

`docs/Help/Lua/Language Basics.md:66-81` is the doc that goes stale — it now
documents `cru.fs.read` and `cru.fs.write` as the canonical access, and an
`edit` beside them belongs in the same passage.

Nothing in `src/lib/offline/` is safe to write until all three blind-write
routes can refuse a stale base.

`register-panels.tsx` does not change. The compact shell reads the same
registry.

## 15. Tests

Follow the tiers in [[Web User Stories]].

- **W1** — unit tests with a stubbed `matchMedia`. `theme-preference.test.ts`
  already stubs it, so copy that helper. Cover the drawer state machine, the
  tab stack and the vim resolver. Cover the outbox drain and the 409 path with
  a fake IndexedDB.
- **W2** — Playwright with a phone device profile. Cover the swipe, the drawer
  button, a tree tap, and the three-step session flow.
- **W3** — screenshot baselines for the drawer open, the drawer closed and each
  step of the session sheet.
- **W4** — the offline sync needs a live tier, and it is the one part that
  does. The conflict rule is only real against a real file on disk. Cover
  three cases: a clean drain, a 409 that writes a conflict copy, and a drain
  that survives a reload. A mocked tier cannot prove any of them.

Section 13 adds its own gates:

- A Rust test that `PATCH /api/kiln/file` changes only the anchored lines and
  leaves the rest byte-identical. The kanban comment records the regression it
  prevents: an earlier writer re-emitted the document and deleted every heading.
- **A test per anchor rule, each red-proved, and EXACTLY-ONCE first** — every
  other rule rests on it. Cover: zero matches refuses; two matches refuses
  without a tiebreak and succeeds with one; an anchor inside a fenced block is
  not matched; a batch whose second edit fails writes NOTHING; `replace` already
  present at the anchor succeeds as already-applied.
- A gate that PINS the CRLF and trailing-whitespace decision. Unpinned, the
  first implementation defines it silently and the second changes it.
- **One test crossing the Rust-to-Lua boundary for `cru.fs.edit`.** `AGENTS.md`
  requires one for any feature crossing a language boundary.
- A Rust test that the PATCH refuses `..`, `/` and `\` in a path. The kanban
  plugin checks this by hand today, and the point of moving it is that no
  caller has to.

Two more gates, both cheap:

- A Rust unit test for the hash comparison in `kiln.rs`. Break the check and
  watch it fail before you keep it.
- `src/test/pwa-scope.test.ts` must still pass. It asserts that
  `runtimeCaching` stays undefined. If the offline work ever needs to touch it,
  the design in section 11 is wrong.

Add a story per FEATURE to [[Web User Stories]] — not one per section, which is
what an earlier draft said. Each needs acceptance criteria and a **named lowest
tier**, plus a W4 leg when it crosses the daemon boundary.
`crates/crucible-web/web/e2e/live/kiln-truth.live.spec.ts` is the precedent —
it covers **WS-201/202/205/206**, and `WS-202: a note saved through the browser
lands on real disk` is already the on-disk byte-exactness W4 leg.

For the W1 work: `theme-preference.test.ts` lives at `web/src/lib/__tests__/`,
not `src/test/`, and its `matchMedia` stub is not exported. "Copy that helper"
means duplicating it, or extracting it first.

## 16. Open questions

1. **The breakpoint.** Is `767px` right, or should a coarse pointer decide it
   alone? A tablet with a keyboard wants the window manager.
2. **A tablet form.** This draft covers a phone. A tablet could keep one rail
   and drop the rest. It is out of scope here.
3. **The graph.** Is a read-only graph worth the bundle on a phone?
4. **Settings storage.** `localStorage` holds one settings object per browser
   profile. A desktop and a phone that share a profile share every other
   setting too. Only vim mode gets a split key in this draft. Fonts, autosave
   and the terminal font do not.
5. **The mirror cap.** Section 11 mirrors the last 20 opened notes, which is
   `MAX_RECENTS` today. Twenty is a starting number, not a measured one.
6. **Eviction, and the browser's own eviction.** IndexedDB has no fixed quota,
   and a browser may drop a whole origin under storage pressure. Call
   `navigator.storage.persist()`, and decide what the app says when the browser
   refuses. An outbox that a browser evicts loses a user's writing.
7. **A read-only session transcript offline.** A session history is markdown
   too. It could mirror on the same mechanism. It is not in this draft.
8. **A file change does not republish.** Section 13's invariant makes a
   plugin's state *recoverable* from its files, not *refreshed*. A user-authored
   note write triggers no republish, and no file-change-to-republish trigger
   exists in either design — so a board stays stale until something re-invokes
   the plugin. Offline makes it worse: the edit lands with the plugin not
   running at all. Owner: `The Plugin Contract`, but this draft is where it
   bites.
9. **Who owns a saved query — and it is one question with scoped publications.**
   Section 13 gives the client a query over the index, and a user will want to
   name one and keep it. The plugin work has the same shape from the other
   side: a publication has exactly one scope today, global per plugin, while a
   review index wants a *session* scope and tree expansion wants a *viewer*
   scope. A saved query is a third case. **Design the scope vocabulary once,
   jointly, rather than growing one on each side.** Owner: this draft and
   `Plugin API Plan.md` together.
   **One constraint is already agreed and is not open: the binding resolves
   server-side.** A scope a caller asserts is a caller reading another caller's
   state. So `data/session/<id>/…` takes `<id>` from the request's resolved
   session, and a literal `me` is substituted by the server — the client never
   writes an id it chose. The shape stays open; this does not.
10. **A cross-file move.** Section 13 keeps a batch atomic within ONE file. An
   archive moves a task between two. Either `PATCH` grows a multi-file form, or
   the caller repairs a half-applied move. Pick one before a task plugin ships
   an archive action.
11. **An item-level task index.** A kiln-wide checkbox collation needs one,
   and no table holds it. `CheckboxStatus` already parses five states, so the
   parser is ready and the index is not. Size it before promising the view.
12. **A desktop PWA.** The manifest already installs on a desktop. An installed
   desktop PWA gets the window manager, not the compact shell, because the
   breakpoint decides. Section 11's offline store would then apply to a shell
   this draft never designed for it.
13. **A lost phone.** Rotating the key cuts the phone off from the daemon, but
    the mirror stays readable on the device. Should a revoked phone erase its
    mirror the next time it reaches the daemon? It cannot erase it before then.
    The outbox must never be erased this way, because it may hold the only copy
    of the user's writing.