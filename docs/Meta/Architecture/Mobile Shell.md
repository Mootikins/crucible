---
title: Mobile Shell
description: A draft design for the small-screen web shell — edge drawers (sessions and files as tabs on the left, backlinks on the right), a tab stack, a plain editor that autosaves notes, a stepped new-session flow, an offline kiln with a sync, and what plugin blocks and surfaces cost on a phone.
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

**Built as of 2026-09-12.** Track A (the `P2` shell): the shell switch, the
drawers, the tab stack and its host, the project switcher, the overflow menu,
the plain editor, the three-step session flow, and settings as a drill-down
(section 10a). Track B (section 13's write primitive): anchored edits in
`crucible-core`, `PATCH /api/kiln/file`, `cru.fs.edit`, and the note index
carrying its author's frontmatter. Track C (section 11): the offline store, a
kiln kept whole, the outbox, and the Offline settings group. Track D (section
11's conflict rule, 2026-09-14): a note write carries the text it was made from,
a stale write is merged under a per-path lock, and what the merge cannot settle
waits as a conflict the user resolves region by region. The conflict copy is
retired.

**This note drifted from the code and was reconciled against it on 2026-09-12,
after a review found ~40 false claims.** Where a decision was NOT carried out,
it is marked `NOT BUILT` in place rather than deleted — a decision that was
made and then not honoured is worth more than silence. Section 14 is now a
record generated from `git diff`, not a plan.

What is NOT built, and named as such: the kiln-wide checkbox index (open
question 11), a cross-file atomic move (10), the scope vocabulary (9),
screenshot baselines for the phone (W3), and a live offline drain (W4).

**Closed since:** guarded writes now run through the daemon's `fs.write` RPC.
The web routes forward the write; the daemon owns containment, project policy,
compare, merge and the lock shared with agent note tools. Base-less legacy
writes remain explicit replacements.
Ticking a checkbox in the reading view is the anchored edit's first caller —
the "one or two lines at a time" case section 13 was written for.

## 1. What the code holds today

These facts come from the tree at `crates/crucible-web/web`.

| Fact | Evidence |
|------|----------|
| The shell has no WIDTH breakpoints. | Zero Tailwind breakpoint prefixes in `src/`. It does carry `@media(hover:none)` rules, so it is not innocent of touch — only of width. |
| The shell is a window manager. | `src/windowing/components/WindowManager.tsx` |
| Rails are fixed-width and collapsible. | `src/windowing/components/DockedBody.tsx` — 250 px default width |
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
them through `<Dynamic>` with no pane or tab context, and only `App.tsx` imports
`windowing/` (the record counted two; the second is a comment in `CenterComposer.tsx`). Every panel mounts in a second shell as it is.

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
| File tree and session as the two drawers | **Changed:** sessions and files are two tabs in the LEFT drawer; the right drawer holds Backlinks. Tabs never move | Sections 4, 7 |
| A kiln/project picker in the left drawer | **Adopted as written:** the Files tab's root picker, in the left drawer. The Sessions tab gains its own **project switcher**, so a user can leave the recency list | Section 7 |
| No vim mode | **Changed:** vim is OFF by default on a phone, with its own setting | Section 8 |
| Online only | **Kept.** Sections 11 and 13 design the `P3` entries Offline Kiln Cache and Offline Note Capture. Build the shell without them | Sections 11, 13 |

Two decisions reach past the phone, and the decision log records both:

- **Notes autosave on both shells** (section 8). The desktop editor changes too.
- **The desktop layout belongs to the desktop shell alone.** The compact shell
  never loads it, never saves it, and has no concept of it; its own tab stack
  persists in this browser (section 6). The layout stays one per daemon, shared
  by every desktop — per-machine desktop layouts would be a separate change.

(An earlier revision said the compact shell should honour the desktop's **Swap
Side Panels**. It cannot: that command mirrors the layout trees and stores
nothing else, and the result lives only in `/api/layout`, which the compact
shell never loads — see section 3.)

The record counted "all 15 panels" as mountable. There are 18 now:
`plugin-blocks` and `surfaces` arrived after it (section 10).

## 3. The switch

Add `src/stores/deviceStore.ts`.

```ts
// A phone, or a small window with a coarse pointer.
export const isCompact = () => /* matchMedia('(max-width: 767px)') */;
```

Rules for the switch:

- **Decide once, at load, and never again in that page.** Do not subscribe to
  the width. `App.tsx` loads the desktop layout once, at mount
  (`App.tsx:248-249`), so a live swap to the desktop shell would mount
  `WindowManager` with no layout at all. Rotating a tablet or resizing a window
  across the breakpoint keeps the current shell; a reload re-decides.
- Fall back to `false` where `matchMedia` is absent. Tests and embedded
  runtimes need this. `theme.ts` line 63 records the same need.
- `App.tsx` renders `<MobileShell />` or `<WindowManager />`. It renders one,
  never both. The providers stay above the choice.
- **The compact shell never loads or saves the desktop layout, and the reason is
  bigger than one device.** The layout persists to the DAEMON —
  `POST /api/layout` (`web/src/lib/api.ts:2062`) — not to a browser key. A phone
  that ran `setupLayoutAutoSave` would overwrite the layout of every desktop on
  that daemon. So `loadLayoutOnStartup` and `setupLayoutAutoSave` run only on
  the desktop branch.
- **Fix the viewport meta.** `index.html:5` lacks `viewport-fit=cover`, and
  without it `env(safe-area-inset-*)` is 0 on iOS, so section 4's inset tokens
  would do nothing. Add `interactive-widget=resizes-content` too, so the
  composer stays above the Android keyboard.

## 4. The layout

Three regions, and one content surface.

```
┌──────────────────────────────────┐
│ ☰   Session title          ⋯  ▸ │  app bar, 56 px
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

- **The app bar** holds the left-drawer button on the left. This is the
  minimum requirement. It holds the title in the middle. It holds an overflow
  menu and the right-drawer button on the right.
- **The left drawer is where a user goes: Sessions and Files, as two tabs.**
  Both pickers live together, so a user moves between a session and a note
  without crossing the screen. Section 7 describes it.
- **The right drawer is the open note's context: Backlinks.** Nothing else,
  for now; open question 14 asks whether a second pane earns a place there.
- **The tabs never move.** This is the record's "edge drawers with tabs but no
  tab MOVEMENT".
- **Every other panel opens from the overflow menu, as a content tab.**
  Surfaces, Changes, Activity, Plugin Blocks, Search, Skills and the rest are
  things a user visits, not context kept beside a note.
- **The content surface** shows exactly one registered panel.

**Render it the way `Pane` does, not by spreading metadata.** Copy
`windowing/Pane.tsx:113-130`: a memo keyed on the tab's id and content type, and
props through `reactiveMetadataProps` (`lib/panel-props.ts`). A plain spread of
`tab.metadata` remounts the editor on every `isModified` write, or loops —
`panel-props.ts:21-28` records both failures.

**The editor is the main area**, as the record says. On a cold start the surface
shows the last active tab from the saved tab stack. With no saved stack it shows
the shared `EmptyState` (2026-09-15, pick R2): the title "No note is open", one
body line, and two offers — **Open a note**, which opens the left drawer on its
Files tab, and **Start a session**. The earlier plan named the recent notes from
`/api/recents` here; the shipped state offers the two doors instead, and the
recents still reach the user through the files drawer. A chat is a tab like any
other, not the default.

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

**Android's gesture navigation claims the same edges.** On a phone set to gesture
navigation, a swipe in from either edge is the system's Back, and a web page
cannot exclude a region the way a native app can. There the edge swipe fires
`popstate`, and the back stack treats it as Back: it closes an open drawer or
does nothing harmful. The swipe still works on iOS, on Android with button
navigation, and inside an open drawer. This is why the button, below, is the
guaranteed input and the swipe is only the preferred one.

### The button

The gesture is the preferred input. The button is the guaranteed input.

The app bar always shows both drawer buttons. A user with a stylus,
a mouse or a screen reader must never depend on a swipe.

### The implementation

Use pointer events, not touch events. Pointer events cover a stylus and a
mouse with one code path.

- Set `touch-action: pan-y` on the content surface. The browser then gives the
  horizontal axis to the drawer.
- Animate `transform: translateX(...)`. Do not animate `width`. `DockedBody.tsx`
  (`src/windowing/components/`) records the same reason for the desktop rails.
- Honour `prefers-reduced-motion`. `DockedBody.tsx` shows the check.
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
  `src/windowing/model/tab-guards.ts` holds that RULE in `confirmTabClose` — but reuse the
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

### The tab host seam — built 2026-09-11

An earlier revision said three openers are welded to `windowStore`. A review
against master found more than ten sites, and one of them guards unsaved work.
All of them now go through `lib/tab-host.ts`:

| Site | What breaks on the compact shell without the seam |
|---|---|
| `FileViewerPanel.tsx:334-346` | writes `isModified` only into `windowStore`, so no dirty dot and **no close guard** — edits can be lost |
| `appWindowPolicy.onActiveTabChange` in `src/stores/windowStore.ts` | the ONLY writer of `statusBarStore.activeSessionId`; `SessionContext.tsx:259-262` follows it, so a tab switch would not change `currentSession` |
| `ChatContext.tsx:242-244` | the tab title never updates |
| `SessionContext.tsx:394-396,418-420` | deleting or archiving a session leaves its tab open |
| `draft-session.ts:37-43,90` | `findDraftTab` and `closeDraftTab` miss the compact draft; the draft stays open after send |
| `file-actions.ts:20,52,113` | open, open-at-line and `closeTabsUnder` |
| `session-actions.ts:58,76` | `openTabBesideEditor`, `openSessionInChat` |
| `panel-actions.ts:76`, `shellStore.ts:60,75` | `openPanelTab`, and the shell's go-to actions |
| `files/file-tree-a11y.ts:16` | `currentOpenFilePath`, the tree's highlight |

**One seam, not ten branches** — `src/lib/tab-host.ts`:

```ts
interface TabHost {
  find(pred: (t: Tab) => boolean): Tab | null;
  open(tab: Tab): void;           // add, or focus if present
  activate(id: string): void;
  update(id: string, patch: Partial<Tab>): void;
  remove(id: string): void;
}
```

Two implementations: `windowTabHost` wraps `windowStore` and keeps the desktop's
behaviour byte-for-byte; `stackTabHost` wraps `tabStackStore`. `tabHost()`
returns the one the shell chose at load. Both `activate` paths call
`statusBarActions.setActiveSessionId` and `syncShellSurface`, which is what
keeps `currentSession` true. Every site in the table then calls `tabHost()`.

This is runtime polymorphism with two real implementations, so an interface is
right here. The migration touched desktop code, and the desktop suite stayed
green with no test edited to fit.

Two things the migration found. `openFileAtLine` could not open anything on a
phone — its new-tab branch addressed a window-store group directly. And closing
a tab with unsaved work cannot use `confirmTabClose`, because it calls
`window.confirm`, which an installed PWA may suppress; the tab card asks in
place instead.

**Persist the tab stack in `localStorage`** (`crucible:compactTabs`), per
browser. Never call `saveLayout` or `loadLayout` from the compact shell — see
section 3.

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
history.pushState({ crucibleNav: id }, '');  // no URL argument: the URL stays
```

The state carries the depth. A `popstate` listener pops the stack.

**An earlier revision passed `location.pathname` as the third argument. That
strips the hash**, so the first tab a user opened would have erased the deep
link that brought them in. Omitting the argument keeps the whole URL, hash
included. At startup the shell reads the hash once to route a deep link, then
leaves it alone.

## 7. The two drawers

**Decided 2026-09-11:** the file picker and the session picker share the LEFT
drawer, as two tabs, and the right drawer holds Backlinks. This puts the picker
where decision row 78 put it — "a kiln/project picker in the left drawer" — and
supersedes that row's two-drawer split. The tabs stay mounted while hidden, so
the file tree keeps its expansion and the session list its scroll
(`DrawerTabs.tsx`).

### The Sessions tab (left drawer)

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
recency list.** A tap opens a bottom sheet with every project — worktrees are
NOT listed, though `buildRoster` knows them — plus
**All projects**. The list below then shows that project's sessions, most recent
first, with the rest reachable by scroll.

- It reuses `ProjectContext`'s `projects`, `currentProject` and `selectProject`.
  `selectProject` is browser-local — a signal and a remembered pin
  (`contexts/ProjectContext.tsx:123`) — so the phone's choice changes no other
  client and no daemon state.
- **All projects** shows a FLAT list across projects, each row carrying its
  project's name (`SessionsTab.tsx`). The grouped `SessionTree` the desktop
  draws was the intent and is not what was built — one project
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
- **`SessionRow` needs a compact variant.** Its height is fixed at 26 px and it
  carries no project label (`SessionTree.tsx:16-26,51`). The drawer needs 44 px
  rows, and each Inbox row must name its project.

### The Files tab (left drawer)

A root picker above a tree — the desktop `FilesPanel` shape, and the record's
"kiln/project picker".

**Mount `FilesPanel` as it is. Do not rebuild it from its parts.**
`FileTreeView` is a controlled view that needs `collection`, `loadChildren`,
`onLoadedTree` and `onContextAction` (`files/FileTreeView.tsx:32-66`), and
`RootDropdown` needs the session's own roots (`files/RootDropdown.tsx:49-60`).
The logic that feeds them — listing, fs events, reconcile, mutations, the
`treeRootStore` pin — is the 737-line `FilesPanel.tsx`. Composing the parts again
copies all of it. `FilesPanel` uses native HTML5 drag (`lib/file-dnd.ts:4-7`), so
it runs without the window manager's drag provider.

- **The picker** is `FilesPanel`'s `RootDropdown` over `buildRoster`
  (`src/lib/tree-root.ts:56`): three groups, **Projects**, **Worktrees** and
  **Kilns**. `RosterGroup.label` is that exact union (`:23`), so no group may be
  folded into another.
- **The tree** is its `FileTreeView` over the chosen root. It is the tree view
  this draft set out to provide; the picker decides which root it shows.
- **It follows the active session** unless the user pins a root, exactly as
  `treeRootStore` does on the desktop. Open a session in `crucible-docs` and the
  tree shows `crucible-docs`.
- A file tap opens the editor on the content surface, then closes the drawer.

An earlier revision put sessions, projects and kilns in one tree with no picker.
That tree was long, and it ran against the record. The picker costs one tap and
keeps each tree short.

### Density rules

- Row height 44 px for a control row. **Cite the standard correctly**: WCAG 2.1
  has no Level AA target-size criterion. 2.5.5 Target Size is 44x44 CSS px at
  Level **AAA**; WCAG 2.2's 2.5.8 Target Size (Minimum) is Level AA at
  **24x24**. `PRODUCT.md` commits to 2.1 AA, so 44 px exceeds the commitment
  rather than meeting it. Keep 44 px — it is the right number for a thumb — and
  do not justify it with a criterion that does not exist.
- **A TREE row is 36 px, not 44 px (built 2026-09-15, pick R4).** A file tree is
  a list to read as well as a list to tap, and 44 px rows put four folders on a
  phone screen. The touch metrics ride one attribute: a surface stamps
  `data-density="touch"` on the tree root, and every row, icon slot and indent
  guide below it reads the custom properties that attribute sets — 36 px rows
  (`--cru-row-md`), 14 px text, a 22 px icon slot and a 20 px indent. The
  desktop tree keeps 28 px (`--cru-row-sm`). The rules live in plain CSS in
  `styles/refine-touch.css`, not in a utility on the row, because a utility
  pins the value on one element and a nested row then keeps the desktop size.
- **Hover-only actions are already handled — do not "fix" them.**
  `SessionTree.tsx` (the row actions and the project row) carries
  `[@media(hover:none)]:opacity-100`, as does `Message.tsx`, and so do the
  tab-close and rail rules in `index.css`. A coarse pointer already reveals
  them. The assistant turn's footer no longer needs the rule at all: since
  2026-09-15 it is always visible on every pointer, and it shows the turn's
  duration beside copy and regenerate.
- A long press opens a bottom sheet with the row's actions. `FileTreeContextMenu`
  supplies the action list.
- A chevron gets the 22 px slot the touch density sets, inside a 36 px row that
  is the tap target for the whole width. A chevron tap expands. A row tap opens.

## 8. The editor

### Vim mode is a separate setting, off by default on a phone

**Built 2026-09-11; the decision log records it.** The P2 record said
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

**Show both toggles in `EditorSettingsSection`** (`settings/EditorSettings.tsx:12`,
not `settings/sections.tsx`, which only lists the sections), labelled for the shell
they govern — "Vim mode (desktop)" and "Vim mode (phone)". One unlabelled toggle
would change whichever key the current shell reads, and a user on a phone would
see a desktop-only switch that seems to do nothing.

Resolve the value where it is read. `FileViewerPanel.tsx` reads it in TWO places
— `:260`, the dependency that reconfigures an open editor, and `:488`, the prop
it passes down. Both must read the resolver, or a toggle changes the prop and
never reconfigures the open editor:

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
| `autosaveSeconds` | 2 | 2 | Notes save themselves on both shells | **No** — see below |
| `maxLineWidth` | 768 | 0 | The viewport is already narrow | **No** — derive it, do not store it |
| `showSaveButton` | true | true | unchanged | No |

**Notes autosave on both shells — decided 2026-09-11.** An earlier revision
gave the phone its own autosave key, because the desktop default was 0 and 0 on
a phone loses writing. The decision removes the reason: a note now saves 2
seconds after its last edit everywhere, so one key serves both shells.

- **Notes only.** Autosave applies to a file inside a kiln. A project file —
  code, config — still saves by hand, because a save there can fire watchers
  and builds mid-edit. `FileViewerPanel` checks the owning kiln before it arms
  the timer.
- **Existing installs are migrated once.** Every browser that ever saved a
  setting stored the old default, `autosaveSeconds: 0`, explicitly, so a new
  default alone would reach no one. `SETTINGS_VERSION` 2 turns a stored 0 into 2
  once; a user who wants it off turns it off again, and that choice is kept.
- **`Ctrl+S` and `:w` still save at once.** Autosave only removes the need.

`maxLineWidth` does not need one. A narrow viewport already clamps the column,
so the compact shell can ignore the setting rather than store a second value.
`showSaveButton` changes nothing and is listed only so nobody adds a key for it.
It also draws nothing on a phone today: only `windowing/CornerBar.tsx:36` reads it,
and the compact shell has no corner bar. `MobileEditorBar` draws the dirty dot
and the Save action itself.

### The read/write switch

The desktop editor opens the reading view with `Mod-Shift-E`. The compact editor
shows a two-item segmented control in the app bar: **Read** and **Write**.
Read renders `MarkdownPreview`. Write opens CodeMirror.

The composer toolbar sits above the keyboard. It holds the wikilink insert,
the heading level, the list toggle and the save action.

**Two changes are already on master.** Both surfaces share
`renderFrontmatterCardHtml`, so Read mode inherits them free, and Write mode is
where the second one applies:

- A note may carry `properties: expanded` to open its own Properties card
  (`web/src/lib/frontmatter.ts`).
- **The collapsed Properties row is a control, and looks like one (built
  2026-09-15, pick R5).** It carries a hairline border that darkens under a
  pointer, reads in the document's own font rather than the 11 px mono of the
  key column, and stands 44 px tall at touch density. The phone drops the right
  gutter that the desktop card reserves for its floating mode toggles, because
  the phone puts those controls in the header bar, so the card spans the whole
  content column there. The open card keeps its block and its summary carries
  no box. The grey bar that used to sit under the collapsed card was the
  active-line wash on the hidden gap line; a rule clears it for that one line.
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

Five rows — `CenterComposer`'s five context chips — each with its current value
on the right:

```
Project     crucible          ›
Workspace   the checkout      ›      where the files live (a worktree, a host)
Kiln        crucible-docs     ›
Model       default           ›
Runtime     this machine      ›      where the process runs
```

An earlier revision listed four and dropped **Workspace**, the very axis the
next paragraph says to keep apart from Runtime.

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

### How it mounts, and what it must not copy

- **The registry stays as it is**, so `chat-draft` still maps to
  `CenterComposer` (`register-panels.tsx:40`). The compact shell's renderer
  keeps a small override map — `chat-draft` → `NewSessionSheet`, `settings` → a
  full-height sheet — and consults it before the registry.
- **Extract the submit first.** The create-session call and the empty-value
  contract live inside `CenterComposer.tsx:196-225`. Move them into a shared
  function and let both surfaces call it. Two copies of "untouched is not the
  same as `host`" is two chances to containerize a session that opted out.

### What stays the same

Lazy creation stays. Nothing reaches the daemon until the first send.
`draft-session.ts` records the contract, and the compact flow must keep it.

The compact flow opens the sheet as a tab of type `chat-draft`, exactly as the
desktop does. On send, it replaces that tab with the chat tab. `draft-session.ts`
already holds the one-draft-at-a-time rule, and the compact shell keeps it.

## 10. Panels on the compact shell

| Panel | Compact shell | Reason |
|-------|---------------|--------|
| chat | content surface | A tab like any other. The editor is the main area (section 4). |
| chat-draft | full-height sheet | Section 9. |
| file | content surface | Section 8. |
| sessions | left drawer, Sessions tab | Section 7. |
| files | left drawer, Files tab | Section 7. |
| search | content surface | Results need width. |
| inbox | content surface | |
| activity | content tab, from the overflow menu | A feed a user visits. |
| backlinks | right drawer | The open note's context. |
| changes | content tab, from the overflow menu | A review queue a user visits. |
| settings | full-screen dialog, drilled into | Section 10a. NOT the registered panel: the overflow row opens the dialog, because the panel is the stacked tab. |
| skills | content surface | |
| plugins | content surface | |
| graph | content surface, read only | Pan and zoom work. Node drag does not. |
| plugin-blocks | content tab, from the overflow menu | Registered `right` (`register-panels.tsx:56`). A plugin block is working context. |
| surfaces | content tab, from the overflow menu | Registered `left` (`register-panels.tsx:60`). A surface is a list of rows with a closed set of marks; it reflows to any width without help (section 12). |
| canvas | not offered | It needs drag and a large field. |
| terminal | not offered | It needs a keyboard. |

A panel that the compact shell does not offer must say so. It must not fail
silently.

### 10a. Settings on a phone drill in, they do not tab

The desktop dialog is a section list beside a form. That list alone is 216 px
of a 412 px screen, and the first attempt flattened it to a strip of tabs
across the top — which showed two labels out of twelve and hid the rest behind
a horizontal scroll with no affordance. A strip is a bad list.

**A phone gets one list at a time.** The root names every category, grouped.
A tap opens that category as its own page, with the category named in the bar
and a back control beside it. This is the shape iOS Settings, Android Settings
and Obsidian mobile all use, and they use it for the reason it applies here:
a phone can show one level legibly, and legibility beats simultaneity.

**Depth is not capped at two.** A page can push another, and the daemon's
configuration tree does: `Configuration` lists `Chat`, `Embeddings` and the
rest as rows carrying a count, each opening its own page, and a group holding
groups drills again. Every pushed page renders at depth 0, so the nesting is
the tree's, not the renderer's. Inlining that tree gave a 412 px screen one
scroll of every leaf the daemon declares.

**Each level takes a history entry**, so the phone's own back button walks the
levels before it leaves the app (`createSettingsStack`, over `NavStack`). Two
rules follow, and both are tested:

- Closing the dialog from three levels deep gives back all three entries.
  Otherwise the next back press walks a dialog that is no longer on screen.
- Escape means "up one level", and only closes at the root. The dialog does
  not install its own Escape handler while a phone is showing, or the first
  keystroke would close it from any depth.

**A section does not know which shell it is in.** `useSettingsStack()` answers
null on the desktop, and a section that would push a sub-page renders it
inline there instead. Null is the ordinary case, not an error.

**A control sits to the RIGHT of the text it labels**, not under it — the
placement iOS, Android and Obsidian all use for a switch, a value and a
picker alike. The settings form is a two-column table built for the desktop
dialog, so one media block in `index.css` turns each row into a flex row: the
label takes what is left and wraps inside it, the control keeps a fixed column
and a thumb-sized height.

The trap in that block is `:only-child`. A status, an error and a section
header are ONE cell spanning both columns, and that cell is both the first and
the last child — so a rule written as `td:last-child` right-aligns every
message in the dialog. Both cases are selected explicitly.

## 11. An offline kiln, and the sync

A phone loses the network. The shell must still open a kiln and edit a note.

### One kiln, whole, when the user chooses it — decided 2026-09-11

An earlier revision mirrored a working set: pinned notes, the last twenty
opened, and anything queued. That is a heuristic, and a heuristic is the wrong
shape here — a user who wants a kiln on a plane wants the kiln, not the part of
it a cache guessed at.

**So: a kiln is either kept offline or it is not, and the user says which.**

- **Whole.** Every note in the chosen kiln, bodies and all. No cap, no
  eviction, no recency seed. `listNotes` gives the set; each body is fetched
  once and refreshed on change.
- **Per kiln.** A kiln is the unit because a kiln is what a user thinks in.
  Projects are not offered: they hold code, which the phone does not edit.
- **Until unchosen.** That answers the record's "for how long". Nothing expires
  on a timer; a user who wants the space back turns the kiln off, and the app
  says how much it will free.
- **Two ways in, one source of truth.** A settings group, **Offline**, lists
  every kiln with a toggle and its size on disk — discoverable, reversible, and
  the place a user goes to reclaim space. The files drawer's root picker adds a
  long-press shortcut, **Keep offline**, so the choice is where the kiln is.
  The settings group is the source of truth; the shortcut writes the same flag.
- **Say what is happening.** A kiln being fetched shows progress, and a kiln
  that is kept shows it. A silent cache is one a user cannot trust or clear.

### Attachments: two modes, the user picks per kiln

A kiln holds more than notes. Notes are kilobytes; images and PDFs are the
whole budget, so they get their own choice — **per kiln, beside the toggle that
keeps it**:

| Mode | Notes | Attachments |
|---|---|---|
| **Notes only** (default) | every one, at once | fetched when first opened, then kept |
| **Everything** | every one, at once | every one, at once |

**Everything** is the mode for a user who wants a kiln on a plane and expects
its images to be there — the reason the choice exists. **Notes only** is the
default because a kiln's attachments can be hundreds of megabytes and a phone
should not spend them without being asked.

Three rules that follow from where the bytes come from:

- **A binary is not a note.** Text comes from `GET /api/kiln/file`, which
  refuses non-UTF-8 with a 415 and says to use `/api/file/raw`. Attachments come
  from that raw route as bytes, and are stored as Blobs, not strings.
- **A cached binary renders only through `<img>`.** `/api/file/raw` strips power
  from those bytes deliberately — `nosniff`, `Content-Disposition`,
  `application/octet-stream`, and for SVG a `sandbox` CSP that gives it an
  opaque origin (`routes/kiln.rs:209`). A `blob:` URL carries none of that and
  inherits THIS origin. An `<img>` cannot run script even for an SVG; an
  `<iframe>` or `<object>` can, so anything needing a document context — a PDF
  card — re-fetches from the network when online rather than being handed a blob.
- **Attachments are read-only.** The phone edits notes; no attachment ever
  enters the outbox, so none of the conflict rules apply to one.

Nothing else changes: the index still holds every note's name and path, the
outbox still holds what the user wrote, and a note outside a kept kiln is still
read from the network.

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

**1. What may be cached — answered.** Every note of a kiln the user chose to
keep offline, and the note index of every kiln they open. Nothing else: a
project's files are never cached.

**2. For how long — answered.** Until the user turns that kiln off. Nothing
expires on a timer; an expiry would empty the cache exactly when the phone is
offline and cannot refill it.

**3. What happens on sign-out — answered, 2026-09-11: nothing is wiped, ever,
by the app.**

The cached notes stay until the user removes them. A lapsed or rotated key
stops sync; it does not touch what is already on the device. When a new key is
entered and the phone reconnects, the drain resumes.

**Revocation cannot do what the question implied, and that is why the answer is
simple.** A web app cannot erase a cache remotely: the bytes are on the phone,
and rotating `api_key` only stops the phone from talking to the daemon. The
real choices were "do not cache" or "accept that the device holds a copy", and
the second is the bargain every offline notes app makes. The cache is as safe
as the device, and the design must not imply otherwise.

**One hazard is revocation-adjacent, and the rule above does not cover it: the
outbox draining into the WRONG daemon.** A key identifies a daemon, not a
person. A phone that reconnects to a different daemon — another machine, a
restored backup, a different instance answering the same address — would replay
the user's queued edits into that daemon's kiln.

> **So bind the mirror and the outbox to the daemon they came from, and refuse
> to drain into another.** There is no daemon id on the wire today; the
> practical identity is the origin plus `config_root` (`GET /api/config`), and a
> mismatch must stop the drain and say so rather than write.

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

**Facts 1 to 3 are closed; fact 4 still holds.** `GET /api/kiln/file` answers a
`content_hash` taken from the disk bytes it just read, not from the index, and
`PUT`/`PATCH` hash the disk inside the read-modify-write, so a base names what
the writer actually read. Since 2026-09-14 the write path also holds a per-path
lock and merges a stale write that carries its base text — the conflict rule
below. Fact 4 is untouched and still decides the mechanism.

**The offline store is app-level, not a service worker cache.** The worker keeps
precaching the shell and nothing else.

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

> **BUILT, 2026-09-12, after shipping the thing this forbids.** `PUT
> /api/kiln/file` still takes `{path, content}` and writes blind
> (`routes/kiln.rs`); none of the three routes gained `base_hash`. So
> `networkSink.write` reads the note, compares the hash IN THE BROWSER, and
> only then writes — exactly the second copy of the rule this section rules
> out, on exactly the machine with the stale view.
>
> It is there because the alternative was worse: with no daemon-side check and
> no client-side one, every drain silently overwrote whatever another writer
> had done. The compare narrows the window; it does not close it. Two writers
> can still interleave between the read and the PUT.
>
> Closing it properly means `base_hash` on all three routes, with the compare
> inside the daemon's read-modify-write. `PATCH /api/kiln/file` already does
> this correctly for anchored edits and is the model to copy.
>
> **Closed, 2026-09-13.** The routes take `base_hash`, the browser compare is
> gone, and every note write from the browser goes through one door,
> `lib/offline/sync.ts`: `writeNote` for a whole write and `editNote` for an
> anchored edit. Both send the base, and both routes gate on it: a PUT or a
> PATCH with a base is refused when the note moved on, even when every anchor
> applies; the outbox replay sends no base. Both queue in the outbox only when
> the daemon never answered.

### The stores

Five tables, not three: `mirror`, `outbox`, `index`, `blobs`, `meta`
(`lib/offline/store.ts`). `index` and `meta` were written before they had
readers — `meta` now holds the daemon identity, and `index` still has none, so
an offline user cannot browse a kept kiln's note list.

### The three stores (as first drafted)

```
mirror   path → { body, baseHash, mirroredAt }        every note of a kept kiln
outbox   path → { body, baseHash, queuedAt, daemon }  what this device wrote
index    kiln → { NoteEntry[], indexedAt, kept }      every kiln's note list
```

### The conflict rule — rewritten 2026-09-14

The daemon refuses a stale base with a 409. What happens next depends on what the
write carried, not on who is present.

**A write carries the text it was made from.** `PUT /api/kiln/file` takes a
`base_text` beside the `base_hash`. With both, a stale write is **merged** rather
than refused: a three-way line merge (`crucible_core::note_merge`) over the base
text, the writer's text and the disk. The route holds a **per-path lock** across
the read, the compare, the merge and the write, so two writers on one note are
ordered rather than raced. A `base_text` that does not hash to its `base_hash` is
a 422 — the pair is one fact.

- **A clean merge writes**, and the answer carries the merged text (`merged:
  true`), which the caller takes into its buffer. Both writers' changes are on
  disk.
- **A merge that leaves regions writes nothing.** The answer is a 409 carrying
  the current text, the merged text and the **regions** — one span per place the
  two writers changed the same lines differently, each with its base, ours and
  theirs.
- **A write with no base text is still refused**, as before. It cannot be merged,
  because a merge needs three texts.

**A conflict is an entry, not a copy.** The refused write stays in the outbox in
the `conflicted` state, holding the merged text and its regions. It is counted
apart from the unsent edits, because sending again can never settle it — only a
person can. Nothing is written beside the note.

**Three doors, one surface.** The offline badge (its tap opens the conflict when
one waits, instead of sending), the phone's More sheet and the Changes panel all
open the Conflicts panel. It lists what waits and draws the chosen one in a
CodeMirror editor over the merged text, with a block widget at each region:
**Keep mine**, **Keep theirs**, **Keep both**, and the words that differ marked
on each side. Save is disabled until every region is settled. It then writes the
settled text against the hash the daemon answered with; a note that moved again
keeps the conflict open.

**An open buffer is told.** A whole write the drain turned into a conflict reaches
the buffer it was made from: the buffer goes dirty, keeps its text, and the notice
points at where the conflict waits.

**An anchored edit has no base text of its own.** The drain turns a stale anchored
entry into a whole write by applying its anchors to its base text, then merges it
like any other. An entry stored before base texts were kept is a conflict with one
region over the whole note — never a merge, and never a silent overwrite.

**One queued write per note.** The first queued base is what lets the drain see a
remote change, so a second write to a queued note folds into the entry instead of
taking a second one: a tick folds into a queued whole write's text, two ticks
compose into one edit set, and a whole write replaces a queued tick. A fold keeps
the base AND the base text of the first write, because a base that names a text it
was not made from is the route's 422. A write arriving for a *conflicted* entry
replaces it with its own base and base text rather than folding — the conflict's
base is the one the daemon already refused. A write that lands from the outbox
moves the open buffer's base, so the user's next save is not refused for their own
queued write.

**Clear the outbox entry only when its writing is safe.** An entry clears when the
daemon took it — plainly, or as a merge — or after a person settles its conflict.
Until then it is the only copy of that writing, and the rule this section exists to
serve is that the writing is never lost.

**No CRDT.** State the reason plainly: the truth is markdown bytes on disk, which
the agent, the TUI, the CLI and any editor may rewrite at any moment. A CRDT needs
every writer to speak it. Most writers here never will. A three-way merge needs no
writer to co-operate, and it loses nothing: what it cannot merge it hands to the
user as named regions rather than guessing.

**What this replaced, and why.** Until 2026-09-14 a stale write became a **conflict
copy** — a second note beside the original under a dated name
(`Release Notes (conflict, phone, 2026-09-09).md`), which the user was expected to
merge by hand, the way Obsidian Sync resolves one. It lost no bytes, and that was
its whole defence. Its defect is that nothing listed it: a user met the copy by
accident, weeks later, or never, and two notes with the same name are worse than one
note with a question in it. A merge settles the ordinary case with no question at
all, and a conflict is now something the app counts, lists and opens.

### What offline does NOT do

An offline kiln is a note store. It is not a Crucible.

| Offline | Reason |
|---------|--------|
| Read a note of a kept kiln | The mirror holds its body. |
| Edit a note of a kept kiln | The outbox holds the write, one entry per note. A later write folds into it. |
| Browse the WHOLE kiln tree | The index holds every note, body or not. |
| **No note of a kiln that is not kept** | Its body was never fetched. The tree says so. |
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
is gone and `web/src/components/blocks/` is in its place. The design and the settled API
decisions are recorded in working notes outside this kiln.

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
- The plugin API design: until blocks are isolated, a header is
  forgeable by same-origin script. Attribution is not a security boundary.
- The old plugin `Capability` enum is gone. Operator-installed Lua is trusted
  code; the remaining plugin tool-interception declaration does not isolate
  browser script.

The three describe one gap. **Nothing separates plugin code from app code on
this origin**, and every proposed control assumes an isolation that does not
exist yet. The plugin web delivery design selects an opaque-origin
sandboxed iframe and MessageChannel bridge; implementation waits for a
third-party web-asset consumer. The existing exemplars are not the same strength:

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
  it, a second device syncing the same change reports a spurious conflict
  every time. This is the second half of the original-text rule above; it has no
  independent existence.
- **`replace` may carry newlines.** One edit turns one line into two, which is
  how a recurring task writes its next occurrence.

**`base_hash` reports; it does not gate.** Since conflict control has to hash
the disk bytes inside the read-modify-write anyway, the anchors already do
detection. What the hash adds is a better message: it separates "the file moved
on" from "your anchor moved on", which is the difference between showing a user
a conflict over the whole note and showing them one failed edit. Keep it for
the message. Do not build the gate on it.

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

**This invariant is not in the plugin design today.** It was agreed in
correspondence and may land there; until it does, treat it as this draft's
assumption about another design, not as a citation.

It constrains future plugins rather than this one. It also constrains offline directly: an offline note edit
reaches a plugin's files with the plugin not running, so a plugin that fails the
invariant fails hardest exactly here. The honest test for such a plugin is
whether it survives its own user's text editor.

### The primitive belongs in `cru.fs`, not only in HTTP

**Asked for in correspondence with the plugin work, not in its published docs.**
Those ask for the more general thing: a scoped read/write of any shape
and "a refusal a UI can act on". This design is one answer to both, and it is
not yet written into either document. Do not read it as a settled cross-branch dependency.

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

**This is a RECORD, not a plan.** It was a plan, and it drifted: it named five
files that were never written (`offline/db.ts`, `conflict.ts`,
`working-set.ts`, `query.ts`, `line-edit.ts`), omitted fourteen that were, and
put the anchored-edit operation in `crucible-daemon` when it landed in
`crucible-core`. Regenerated from `git diff master..HEAD`.

New:

```
core/src/note_edit.rs
web/src/components/AppShell.tsx
web/src/components/mobile/BottomSheet.tsx
web/src/components/mobile/ContentSurface.tsx
web/src/components/mobile/Drawer.tsx
web/src/components/mobile/DrawerTabs.tsx
web/src/components/mobile/MobileEditorBar.tsx
web/src/components/mobile/MobileShell.tsx
web/src/components/mobile/NavStack.ts
web/src/components/mobile/NewSessionSheet.tsx
web/src/components/mobile/OfflineBadge.tsx
web/src/components/mobile/SessionsTab.tsx
web/src/components/mobile/TabOverview.tsx
web/src/components/mobile/drawer-gesture.ts
web/src/components/mobile/edge-swipe.ts
web/src/components/settings/MobileSettings.tsx
web/src/components/settings/OfflineSettings.tsx
web/src/components/settings/settings-nav.tsx
web/src/lib/offline/identity.ts
web/src/lib/offline/images.ts
web/src/lib/offline/kept.ts
web/src/lib/offline/mirror.ts
web/src/lib/offline/outbox.ts
web/src/lib/offline/store.ts
web/src/lib/offline/sync.ts
web/src/lib/session-draft.ts
web/src/lib/session-inbox.ts
web/src/lib/shell-boot.ts
web/src/lib/tab-host.ts
web/src/stores/deviceStore.ts
web/src/stores/editorModeStore.ts
web/src/stores/tabStackStore.ts
```

Changed:

```
core/src/lib.rs
core/src/storage/note_store.rs
core/src/traits/knowledge.rs
daemon/src/rpc_client/client/mod.rs
daemon/src/rpc_client/client/storage.rs
daemon/src/rpc_client/mod.rs
daemon/src/rpc_client/storage.rs
daemon/src/server/kiln.rs
daemon/src/skills/discovery.rs
daemon/src/storage/sqlite/repository.rs
lua/src/fs.rs
web-rs/src/routes/helpers.rs
web-rs/src/routes/kiln.rs
web-rs/src/services/daemon.rs
web/e2e/live/kiln-truth.live.spec.ts
web/index.html
web/src/App.tsx
web/src/components/CenterComposer.tsx
web/src/components/FileViewerPanel.tsx
web/src/components/SessionsPanel.tsx
web/src/components/settings/EditorSettings.tsx
web/src/components/__tests__/FileViewerPanel.refcount.test.tsx
web/src/components/__tests__/FileViewerPanel.test.tsx
web/src/components/editor/EditorWithPreview.tsx
web/src/components/editor/MarkdownPreview.tsx
web/src/components/files/file-tree-a11y.ts
web/src/components/settings/AppConfigSettings.tsx
web/src/components/settings/SettingsModal.tsx
web/src/components/settings/__tests__/SettingsModal.test.tsx
web/src/components/settings/sections.tsx
web/src/contexts/ChatContext.tsx
web/src/contexts/EditorContext.tsx
web/src/contexts/SessionContext.tsx
web/src/contexts/SettingsContext.tsx
web/src/contexts/__tests__/EditorContext.retry.test.tsx
web/src/contexts/__tests__/EditorContext.test.tsx
web/src/index.css
web/src/lib/api.ts
web/src/lib/draft-session.ts
web/src/lib/file-actions.ts
web/src/lib/icons.ts
web/src/lib/panel-actions.ts
web/src/lib/session-actions.ts
web/src/lib/settings.test.ts
web/src/lib/settings.ts
web/src/lib/tab-icons.ts
web/src/lib/types.ts
web/src/stores/shellStore.ts
```

Tests added:

```
core/tests/note_edit.rs
core/tests/public_properties.rs
web/e2e/stories/mobile-shell.story.spec.ts
web/src/components/mobile/__tests__/AppShell.test.tsx
web/src/components/mobile/__tests__/ContentSurface.test.tsx
web/src/components/mobile/__tests__/Drawer.test.tsx
web/src/components/mobile/__tests__/MobileEditorBar.test.tsx
web/src/components/mobile/__tests__/MobileShell.test.tsx
web/src/components/mobile/__tests__/NavStack.test.ts
web/src/components/mobile/__tests__/NewSessionSheet.test.tsx
web/src/components/mobile/__tests__/OfflineBadge.test.tsx
web/src/components/mobile/__tests__/SessionsTab.test.tsx
web/src/components/mobile/__tests__/TabOverview.test.tsx
web/src/components/mobile/__tests__/drawer-gesture.test.ts
web/src/components/mobile/__tests__/edge-swipe.test.ts
web/src/components/settings/__tests__/OfflineSettings.test.tsx
web/src/components/settings/__tests__/SettingsModal.compact.test.tsx
web/src/components/settings/__tests__/settings-nav.test.ts
web/src/lib/__tests__/session-draft.test.ts
web/src/lib/__tests__/session-inbox.test.ts
web/src/lib/__tests__/shell-boot.test.ts
web/src/lib/__tests__/tab-host-routing.test.ts
web/src/lib/__tests__/tab-host.test.ts
web/src/lib/offline/__tests__/idb-store.test.ts
web/src/lib/offline/__tests__/images.test.ts
web/src/lib/offline/__tests__/kept.test.ts
web/src/lib/offline/__tests__/mirror.test.ts
web/src/lib/offline/__tests__/outbox.test.ts
web/src/lib/offline/__tests__/store.test.ts
web/src/lib/offline/__tests__/sync.test.ts
web/src/stores/__tests__/deviceStore.test.ts
web/src/stores/__tests__/tabStackStore.test.ts
```

**The core operation lives in `crucible-core`, not in an Axum route and not in
the daemon.** Section 13 asks for two callers — `PATCH /api/kiln/file` and
`cru.fs.edit` — and `crucible-lua` depends on `crucible-core`, not on
`crucible-daemon`. Both routes are thin adapters over `apply_anchored_edits`.

`cru.fs.edit` is not free on the Lua side: `crucible-lua/src/host_api.rs`
holds `UNSIGNED` behind a ratchet test
(`crucible-daemon/tests/plugin_stubs_contract.rs`), so a VM function that is
neither declared nor listed **fails the build**.

`docs/Help/Lua/Language Basics.md` documents `cru.fs.edit` beside
`cru.fs.read` and `cru.fs.write`.

## 15. Tests

Follow the tiers in [[Web User Stories]].

- **W1** — unit tests with a stubbed `matchMedia`. `theme-preference.test.ts`
  already stubs it, so copy that helper. Cover the drawer state machine, the
  tab stack and the vim resolver. Cover the outbox drain and the 409 path with
  a fake IndexedDB.
- **W2** — Playwright with a phone device profile. Cover the swipe, the drawer
  button, a tree tap, and the three-step session flow.
- **W3** — screenshot baselines for the drawer open, the drawer closed and each
  step of the session sheet. **NOT BUILT.** `story.step()` attaches a frame and
  asserts nothing, and `e2e/__screenshots__/stories/` holds no baseline for
  `mobile-shell.story.spec.ts`. Every layout defect on the phone so far was
  found by a human reading a screenshot, which is the gap this names.
- **W4** — the offline sync needs a live tier, and it is the one part that
  does. The conflict rule is only real against a real file on disk. Cover
  three cases: a clean drain, a 409 the merge settles or hands back as
  regions, and a drain that survives a reload. A mocked tier cannot prove any of
  them.
  **PARTLY BUILT (2026-09-14).** The 409 case is covered live:
  `e2e/live/kiln-truth.live.spec.ts` proves the route's two answers against a
  real daemon — a stale write merged when it carries its base text, and the
  region answer when both writers changed one line — and
  `e2e/live/conflict.live.spec.ts` drives the whole journey through the app, at
  a phone's viewport as well as a desktop's: two writers on one line, the
  banner, the merge, the region settled with **Keep theirs**, and the settled
  text on disk. The **clean drain** and the **drain that survives a reload** are
  still NOT BUILT — no live leg queues a write offline and drains it. Both
  defects that made Track C useless — a kiln-relative path never joined, and an
  identity fetched at the one moment it cannot be — would have been caught by
  the clean-drain leg alone.

Section 13 adds its own gates:

- A Rust test that `PATCH /api/kiln/file` changes only the anchored lines and
  leaves the rest byte-identical. Built as a LIVE leg
  (`kiln-truth.live.spec.ts`), not a Rust route test — the route's guards run
  through the same helpers the PUT route's tests already cover, but the route
  itself has no `#[cfg(test)]` coverage of its own. The kanban comment records the regression it
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

- ~~A Rust unit test for the hash comparison in `kiln.rs`.~~ **Unimplementable
  as written**: there is no hash comparison in `kiln.rs`. `base_hash` only sets
  `stale_base` on a PATCH refusal; the PUT routes still write blind. The gate
  this asks for cannot exist until those routes gain the check.
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
   setting too. Only vim mode gets a split key in this draft; autosave no
   longer needs one (section 8). Fonts and the terminal font do not.
5. **The browser's own eviction.** IndexedDB has no fixed quota, and a browser
   may drop a whole origin under storage pressure — which, with a whole kiln
   kept, is the thing that would empty it. Call `navigator.storage.persist()`,
   and decide what the app says when the browser refuses. An outbox a browser
   evicts loses a user's writing.
6. **A kiln too big for the device.** "Whole kiln" has no cap by decision. The
   app should say what a kiln will cost before it fetches it, and say what
   happened if the device runs out part way.
7. **A read-only session transcript offline.** A session history is markdown
   too. It could mirror on the same mechanism. It is not in this draft.
8. **A file change does not republish.** Section 13's invariant makes a
   plugin's state *recoverable* from its files, not *refreshed*. A user-authored
   note write triggers no republish, and no file-change-to-republish trigger
   exists in either design — so a board stays stale until something re-invokes
   the plugin. Offline makes it worse: the edit lands with the plugin not
   running at all. Owner: the plugin design, but this draft is where it
   bites.
9. **Who owns a saved query — and it is one question with scoped publications.**
   Section 13 gives the client a query over the index, and a user will want to
   name one and keep it. The plugin work has the same shape from the other
   side: a publication has exactly one scope today, global per plugin, while a
   review index wants a *session* scope and tree expansion wants a *viewer*
   scope. A saved query is a third case. **Design the scope vocabulary once,
   jointly, rather than growing one on each side.** Owner: this draft and
   the plugin API design together.
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
13. **A lost phone — closed, 2026-09-11.** Nothing is wiped. A web app cannot
    erase a cache remotely, so the only real choices were "do not cache" and
    "the device holds a copy"; the second is the bargain every offline notes app
    makes. What replaces it is the daemon-binding rule in section 11: an outbox
    must refuse to drain into a daemon it did not come from.
14. **A second pane in the right drawer.** Backlinks is the only one today.
    Obsidian's mobile right sidebar adds an outline and tags; Crucible has
    neither panel. Of the panels that exist, Changes is the likeliest — reviewing
    an agent's edits beside the note — but it is a queue a user visits, not
    context, so it is not added.
