---
title: Web Windowing
description: The domainless windowing core under src/windowing, its WindowPolicy seam, its edge modes, and its own harness and specs.
tags: [meta, architecture, web, windowing]
status: as-built
as_of: 582c5e6c1
---

# Web Windowing

The web frontend's window manager — edge panels, split panes, tabs, floating
windows — lives in `crates/crucible-web/web/src/windowing/`. It knows no
product concept: no session, no file, no terminal. The app configures it once
with one `WindowPolicy` object, from `stores/windowStore.ts`. Paths below are
relative to `crates/crucible-web/web/`. See [[Crate Map]] for this crate's
place among the others. See [[Web Server]] for the backend that serves this
frontend. That backend also stores the saved layout.

## The folder

| Path | Contents |
|---|---|
| `model/` | `WindowState` and the node types (`types.ts`), the tree helpers (`tree.ts`), the pane and collision helpers (`pane-content.ts`, `pane-collapse.ts`, `pane-boundaries.ts`, `collision-detector.ts`, `layout-restore.ts`, `tab-guards.ts`), and the v10 layout serializer (`serializer.ts`) |
| `store/` | The Solid store, the `WindowPolicy` type (`policy.ts`), and the tab, layout and floating-window actions |
| `components/` | `WindowManager`, `EdgeHost`, `Ribbon`, `DockedBody`, `Pane`, `SplitPane`, `CenterTiling`, `TabBar`, `FloatingWindow`, `WindowControls`, `EmptyPane`, `MinimizedBar`, `RibbonPaneStrip`, plus `split-drag.ts` and `tab-placement.ts`. `context.tsx` holds the `WindowingSlots` type, the provider and `DROP_OVER_ATTR`. `icons.tsx` names the icons that the chrome draws. `RibbonButton.tsx` holds `ribbonBtn` and `RibbonCommand` |
| `reveal/` | `RevealController` and `flyoutRect`, the two pure/reactive rules a hover reveal needs |
| `context-menu.ts` | The native-fallthrough rule for a custom right-click menu (Shift+right-click, images, links) |
| `shortcuts.ts` | `ShortcutAction`, `matchShortcut`, `LAYOUT_SHORTCUTS` — the chords the layout owns |
| `index.ts` | The public entry. It re-exports the names that app code uses. See "The public entry" below |
| `theme.css` | The default theme: the look of every `wm-*` part, in `@layer wm-theme`. See "Styling" below |
| `testing/` | `neutralPolicy` — a policy with no product knowledge, shared by the core's own unit tests and the harness page — and `stackRightRail`, which gives a test a column in the right rail |
| `__tests__/` | The core's unit tests, including `boundary.test.ts` |

## The boundary rule

A file under `src/windowing/` may import only: another module under
`@/windowing/`, `@/lib/cn`, `@/lib/icons`, anything under `@/components/ui/`,
or a package (`solid-js` and the like). It may never import from `@/stores/`,
`@/components/` outside `ui/`, `@/lib/` outside `cn`/`icons`, or `@/contexts/`.

`src/windowing/__tests__/boundary.test.ts` enforces this by reading every
`.ts`/`.tsx` file in the folder and checking every static, dynamic and
side-effect import specifier against that list. It is not a grep a developer
can forget to run: it is a Vitest test, so `bun run test` and CI fail the
build the moment a file under `src/windowing/` reaches into the app.

The design tokens in `src/index.css` are a shared layer that the core uses,
as it uses `components/ui/`. The components do not name those tokens. The
default theme, `src/windowing/theme.css`, reads them. See "Styling" below.
The harness page imports both stylesheets, so the core draws the same way
there as in the app.

## The public entry

App code imports the core from `@/windowing`, which is
`src/windowing/index.ts`. That module holds re-exports only, and each name in
it has an app caller. It exports the store and its actions, `WindowManager`,
the slot type, `DROP_OVER_ATTR`, the model types and tree queries that the app
uses, the stored layout types, the chords and the context menu rule.

`WindowControls`, `useFloatingWindow` and the `FloatingWindowHandle` type
are public, because a tabless content can hold the controls of its window.
See "Floating chrome" below.

`ribbonBtn` and `RibbonCommand` are public on purpose. The app draws its own
rail buttons in `components/shell/RailChrome.tsx`, and those buttons must
match the core buttons.

The default theme, `src/windowing/theme.css`, is the second public entry. It
holds CSS only. The app entry (`src/index.tsx`) imports it once.

`boundary.test.ts` also enforces this rule. It reads every file under `src/`
outside the core and fails when one imports a `@/windowing/<path>` specifier,
or a relative path into the core, other than the index or the theme. The test skips
`src/test-harness/`, every `__tests__` folder and every `*.test.*` file,
because a test and a harness page may import a core module directly.

## The policy

The core takes one `WindowPolicy` object through `configureWindowing(policy)`
(`store/index.ts`). Every member is required — TypeScript refuses a policy
that forgets one. The app's policy is `appWindowPolicy`
(`stores/windowStore.ts`), configured once at module load:

| `WindowPolicy` member | What it replaces | App code that fills it |
|---|---|---|
| `seed()` | The Sessions, Files and Terminal seed | `defaultLayout` (`stores/defaultLayout.ts`) |
| `mayCloseTab(state, groupId, tabId)` | `isLastFixedRailTab` inside `removeTab` | `isLastFixedRailTab` (`stores/fixedRails.ts`) |
| `repairLayout(draft)` | `ensureFixedRails` on restore and reset | `ensureFixedRails` (`stores/fixedRails.ts`) |
| `onActiveTabChange(tab)` | The status bar and shell-surface calls | An inline closure in `stores/windowStore.ts` that sets the active session id and calls `syncShellSurface` |
| `iconFor(contentType)` | The `iconForContentType` call on each tab that a restored layout brings back. `importLayout` gives this member to `deserializeLayout` | `iconForContentType` (`lib/tab-icons.ts`) |
| `unavailableReason(tab)` | The terminal-availability check and its rail tooltip | An inline closure that checks `terminalAllowed()` (`lib/terminal-availability.ts`) |
| `layoutHooks` | The legacy migrations to v9 and the registry prune | `appLayoutHooks` (`stores/layoutMigrations.ts`) |
| `shortcuts` | The chord table the keyboard loop matches | `DEFAULT_SHORTCUTS` (`lib/keyboard-shortcuts.ts`; `LAYOUT_SHORTCUTS` first, the app's chords after) |
| `onShortcut(action, e)` | The app branches inside the old keyboard loop | An inline `switch` in `stores/windowStore.ts` (`focusChatInput`, `newSession`, `clearChat`, `toggleThinking`) |

A policy is content-type generic (`WindowPolicy<C extends string>`). The core
stores one narrowed to plain `string`; `stores/windowStore.ts` re-exports the
store and actions cast to the app's `TabContentType` — the one cast the
windowing plan allows, and it lives nowhere else.

Every action the core exports is wrapped to throw before `configureWindowing`
ran (`requirePolicy` in `store/index.ts`), so a call that reaches the store
before configuration cannot silently mutate state that the seed then
overwrites.

## The slots

`WindowManager` takes `renderContent(tab)` — the app's panel registry lookup
(`lib/render-panel.tsx`) — and a `WindowingSlots` object
(`windowing/components/context.tsx`), and every member of it is optional:

- `railHead(position)` — above the tab icons on a rail (the layout menu, left rail only).
- `railTail(position)` — replaces the default side-swap control at the far end of a rail (the offline badge, the theme and settings buttons, the notification bell).
- `corner()` — the floating cluster at the bottom-right of the centre (`CornerBar`).
- `emptyPaneHints()` — the rows an empty pane prints under its label.
- `attachDropTarget(el, groupId)` — attaches a native HTML5 drop target to a pane body or ribbon, for the group `groupId` names; returns the cleanup.

The app builds all five in `src/components/shell/windowSlots.tsx`
(`appWindowSlots`), backed by `RailChrome.tsx`, `CornerBar.tsx`,
`lib/keyboard-shortcuts.ts` and `lib/file-dnd.ts`, and hands them to
`WindowManager` in `AppShell.tsx`.

## Edge modes

Each edge panel (`left`, `right`) carries an `EdgeMode`. The table names the
mode each state stands for; only `docked` differs in rendering today, as the
paragraph after it explains:

| Mode | Ribbon | Body |
|---|---|---|
| `docked` | shown | in the flow, sized by the layout |
| `strip` | shown | collapsed out of the flow |
| `flyout` | shown | planned: floats over the centre until its button is clicked again |
| `hidden` | planned: hidden | planned: hidden; a hot zone on the screen edge reveals the host as a flyout |

`EdgeHost` (`windowing/components/EdgeHost.tsx`) reads `mode` and sets
`data-edge-mode` on its root, but always renders the ribbon and `DockedBody`;
no current selector or component branch reads that attribute. `docked` and
every other mode share one tree: `DockedBody` stays mounted in every mode,
clipped to zero width once `isEdgeCollapsed` is true (every mode but
`docked`), so a toggle slides it and never remounts it — a terminal keeps its
scrollback and a file tree keeps its scroll position across a collapse.
`flyout` and `hidden` currently render exactly the same as `strip`: only the
`data-edge-mode` value on the DOM tells them apart. That attribute is the seam
the next session's presentation work attaches to, not a rendering branch that
already exists.

`flyout` and `hidden` are typed, stored and placed, but not yet presented:

- **Types and state.** `EdgeMode` and `EdgeCue` are closed unions
  (`model/types.ts`), checked exhaustively by `types.test.ts` against
  `EDGE_MODES` and `EDGE_CUES`. `windowActions.setEdgeMode(position, mode, opts?)`
  writes the mode and, for `hidden`, the cue. `toggleEdgePanel` knows two
  modes only. It sends every mode that is not `docked` to `docked`, and it
  sends `docked` to `strip`. The toggle thus never enters `flyout` or
  `hidden`, and it always brings a hidden rail back.
- **A saved format.** Layout version 10 stores `mode` and an optional `cue`
  (`grip` or `none`, default `grip`) on each `EdgePanel`. See "The saved
  layout" below.
- **A placement rule.** `reveal/flyoutRect.ts` is a pure function: given the
  ribbon button's anchor rect, the viewport size and the rail's stored width,
  it returns where a flyout goes. Width is the rail's stored width, height is
  half the viewport height; both are clamped between a minimum (`FLYOUT_MIN`,
  100px) and the room the viewport leaves after an 8px margin
  (`FLYOUT_MARGIN`) on each side, so a flyout never leaves the screen even on
  a narrow viewport or a wide rail.
- **A reveal controller.** `reveal/RevealController.ts` is the one timing rule
  for every hover reveal: an enter arms a delay before it opens, a leave
  before that timer fires cancels it, a leave after it opens arms a delay
  before it closes, a pin holds it open through a leave, and a tap toggles the
  pin for touch, where hover does not exist. It will drive both the rail hot
  zone and a pane's collapsed band.

`PaneNode` also gained `reveal?: 'click' | 'hover'` (the `PaneReveal` type,
`model/types.ts`), for a pane collapsed to its band inside a rail column.
`layoutActions.setPaneReveal(paneId, reveal)` writes the field on a pane in
the centre or in a rail. No app code calls it yet. No component reads
`reveal` to change how a collapsed pane opens. The write path and the field
exist; the hover behavior does not.

## Expand

A rail can cover the centre. `WindowState.expandedEdge` names the rail that
covers it, or holds `null`. The actions are `expandEdge`, `collapseExpandedEdge`,
`toggleEdgeExpanded` and `setExpandExit`, and the chord `Shift+Escape`
(`toggleExpandFocusedEdge`) toggles the focused rail. The chord is Zed's zoom
chord, and a policy can bind another one.

- **The centre stays mounted.** `CentreColumn` sets `hidden` while a rail
  covers it, so an editor keeps its buffer, scroll and undo history. The rail's
  `DockedBody` fills the row and draws no resize handle.
- **What ends it.** `WindowState.expandExit` is a setting, not a policy
  decision: `toggle` ends an expand on its toggle only, and `centre-focus` also
  ends it when a tab or a pane of the centre TILING takes focus. A floating
  window does not count, although the store files its focus under `center`: a
  peek that floats over the expanded rail must not end the expand.
  `releaseExpandOnCentreFocus` (`model/tree.ts`) holds that rule for both
  `setActiveTab` and `setActivePane`.
- **What else ends it.** A rail that leaves `docked` (toggle, collapse or
  `setEdgeMode`) cannot cover the centre, so it ends the expand. Closing, moving
  or popping out the last tab of that rail also clears the expand. A reset and a
  restore end it and keep `expandExit`. A swap carries it to the rail's new side.
- **Not stored.** The serializer does not write `expandedEdge`, so a reload
  opens the plain layout.

## Swap the centre with a rail

`swapCentreWithEdge(position)` exchanges the centre's layout with one rail's
layout. An app uses it to put sessions in the centre and documents in a rail
with one press. `swapSidePanels` is a different action: it mirrors the two rails.

- **The whole layout moves.** Every pane, split, ratio and collapsed pane goes
  across. Each pane keeps its id, so its tabs and its state stay.
- **The column stays.** The rail keeps its width, its cue and its id, because
  they describe the column and not the panes.
- **Both halves show.** The rail docks, and an expand ends: an expanded rail
  would cover the centre that it just filled.
- **Focus stays on its pane.** `focusedRegion` names the pane's new side.
- **A second call restores the layouts.**

## Ribbon placement

`WindowState.ribbonPlacement` sets where the rail ribbons sit. The action is
`setRibbonPlacement`.

- **`edge`** (the default): the ribbon sits at the window edge and is always
  in view. The body grows out of it, as in Obsidian.
- **`panel`**: the ribbon sits inside the rail, on its inside edge next to
  the centre, in the `[data-edge-card]` element with the body and the resize
  handle. The whole card slides. A closed rail keeps `RIBBON_WIDTH_PX` in
  view, so its ribbon stays at the window edge and the body hides itself.
- **For themes.** `EdgeHost` writes `data-ribbon-placement`. With `edge`,
  there is no card element.
- **Not stored.** It is the user's setting: a reset and a restore keep it, and
  the serializer does not write it.

## Floating chrome

`WindowState.floatingChrome` sets where a floating window puts its controls:
the pin of a transient window, the tab bar toggle, dock, roll up, maximize or
restore, and close. The action is `setFloatingChrome`. `WindowControls.tsx`
holds the controls, so each control has one implementation in every place.

- **`titlebar`** (the default): the window draws a title bar. The title bar
  holds the controls, and it is the drag handle.
- **`merged`**: the window draws no title bar. The controls act on the whole
  window, so they sit in `wm-tabbar-actions` of the window's own tab bar.
  The empty part of that tab bar is the drag handle.
- **A window without a tab bar.** A peek or a hover editor that shows one
  document sets `showTabBar: false`. Such a window has no tab bar to hold the
  controls, so the core gives them to the content. The content calls
  `useFloatingWindow()`, which returns null for docked content. Inside a
  floating window it returns `{ id, controls, chrome, hasTabBar }`. The
  content renders `<fw.controls />` in its own nav bar when `fw.chrome()` is
  `merged` and `fw.hasTabBar()` is false.
- **Compact controls.** A nav bar has little room. To omit roll up and
  maximize there, the content renders `<fw.controls compact />`.
- **The fallback.** A tabless `merged` window whose content does not mount
  `controls` draws its title bar. Without it, the window has no close control.
- **Drag handles.** A press in an element that carries `data-wm-drag-handle`
  moves its floating window. The title bar carries it. With `merged`, the
  window's tab bar carries it. A content marks its own nav bar with it. A
  press on a button, a field, a link, a `[role=button]` element or a tab does
  not start a drag. A drag does not move a maximized window, and it pins a
  transient window.
- **Not stored.** It is the user's setting: a reset and a restore keep it, and
  the serializer does not write it.


A content that shows only some controls of its own (for example a hover
popup with only pin and close) calls `fw.claim()` in its setup instead of
mounting `fw.controls`: the window then draws no fallback title bar while
that component is mounted. `fw.close()` closes the window with the same
unsaved-changes check as its close button.

## Pop out and dock

The right-click menu of a tab moves the tab between the layout and a floating
window, as the panel menu of an Adobe app does. `TabContextMenu` in
`TabBar.tsx` holds the menu. The menu also has the three close rows.

- **Pop out.** A tab in a docked pane (the centre or a rail) shows this row.
  The row calls `popOutPane(paneId, tabId)`. The pop-out button of the tab
  bar calls the same action for the active tab, and it hides when the policy
  keeps that tab. With a tab id, the action moves that tab only into
  a new floating window, and the pane keeps its other tabs. When the tab is
  the only tab of the pane, the whole group moves, as without a tab id.
- **Dock.** A tab in a floating window shows this row. The row calls
  `dockFloatingWindow(windowId, tabId)`, the same action as the dock control
  of the window. With a tab id, the action moves that tab only into the
  centre tiling, and the window keeps its other tabs. The last tab takes the
  window with it. A centre whose root is a split takes the tab beside its
  first pane.
- **The policy.** `canPopOutTab(groupId, tabId)` is false for a tab that the
  policy keeps (`mayCloseTab`) or calls unavailable (`unavailableReason`).
  A closed floating window closes its tabs without a policy check, so a kept
  tab must stay docked. The menu then shows no Pop out row, and the action
  refuses the tab.
- **The ribbon.** Each tab icon of a rail ribbon has the same menu. A theme
  can hide the tab bars of a rail, and then the icon is the only handle of
  the tab.
- **The keyboard.** The menu opens on the `contextmenu` event, so the menu
  key and `Shift+F10` open it on a focused ribbon icon. The arrow keys move
  through the rows, and `Enter` chooses a row.


The tab menu shows "Close Others" and "Close to the Right" only when the
row closes at least one tab. On a group of one kept tab, the menu shows no
close row.
## Layout queries the app uses

`model/tree.ts` holds the pure queries over a layout. Two of them moved here
from `lib/panel-actions.ts`, because they know no product:

- `edgeLeaf(layout, side)` — the leftmost or rightmost leaf. Both halves of a
  stacked split count as the same edge, and the top one wins.
- `firstLeafGroupId(layout)` — the tab group of the first leaf that has one.

## The saved layout

The core owns the **current** format and the one step into it. The app owns
the **history** before that step, because each earlier version names content
types that only existed at the time.

- `windowing/model/serializer.ts`: `LAYOUT_VERSION = 10`, `serializeLayout`,
  and `deserializeLayout(json, hooks, iconFor)`. `deserializeLayout` runs, in order: the
  app's legacy upgrade (only when the stored version is below 9), the v9-to-v10
  step (`isCollapsed` becomes `mode: 'strip' | 'docked'`, and the v10 writer
  never stores `isCollapsed`), a rebuild that gives every rail position a valid
  panel even from a partial or hand-edited payload, the app's `prune`, and
  last the `iconFor` function. A version outside 1–10 throws; an upgrade that does
  not return a v9 payload throws.
- `stores/layoutMigrations.ts`: the v1-to-v9 migration chain, the always-on
  prune of unregistered content types and legacy session-less chat tabs
  (`pruneRestored` — this is what drops a persisted tab for a panel the
  registry no longer has, such as a panel removed after a layout was saved),
  and `appLayoutHooks`, the `LayoutCodecHooks` implementation (`upgradeLegacy`,
  `prune`) that the app hands the core through `WindowPolicy.layoutHooks`.

`layoutActions.importLayout` is the only production caller of the reader. It
passes `policy().layoutHooks`, and a function that calls `policy().iconFor`
for each restored tab.

The app persists the exported layout on the server, not in the browser.
`lib/query/layout.ts`'s `setupLayoutAutoSave` reads
`windowActions.exportLayout()` inside a reactive effect. It saves the
result through `lib/api.ts`'s `saveLayout`, 500ms after the last change.
`crates/crucible-web/src/routes/layout.rs` stores that JSON blob as-is and
returns it unread on `GET /api/layout`. A pane type the server has never
seen still survives the round trip. See [[Web Server]] for the route.
`loadLayoutOnStartup`, in the same module, calls `windowActions.importLayout`
with the loaded blob once, at boot, before auto-save arms.

## Styling

The components render structure and state only. The default theme,
`src/windowing/theme.css`, gives them their look. The app must look the same
with the theme as it looked with the old utility classes.

- **The components keep layout and behaviour.** This is display, flex,
  position, z-index, overflow, cursor, pointer events, selection, the sizes
  that a script reads, and the inline styles of the slide tween and the
  resize code.
- **The theme holds the look.** This is every colour, border colour, radius,
  shadow, outline, font size and weight, opacity, transition and animation,
  and the paddings, gaps and heights that no script reads.
- **The theme is in one layer.** Every rule is in `@layer wm-theme`.
  `src/index.css` puts that layer after `components` and before `utilities`.
  Thus a utility class on a part wins over the default look. An unlayered
  stylesheet, for example a plugin theme, wins over every layer.
- **The theme reads tokens.** Colours read the `--color-*` aliases, so an
  element that sets `--color-shell-bg` recolours itself. Type, radii and
  shadows read the `--cru-*` contract directly, because Tailwind writes an
  alias only when a utility uses it.
- **Where it loads.** `src/index.tsx`, `windowing-harness.tsx`,
  `editor-harness.tsx` import the theme after
  `index.css`.

### Parts

Each element that the theme styles carries one `wm-<part>` class.

| Part | Element |
|---|---|
| `wm-root` | The window manager root |
| `wm-drag-overlay`, `wm-drag-overlay-title` | The label that follows a tab drag |
| `wm-edge-host` | One rail |
| `wm-edge-card` | The card that holds the body, the handle and the ribbon, with `ribbonPlacement: 'panel'`. The default theme gives it no look |
| `wm-edge-body` | The body of a rail. The default theme gives it no look |
| `wm-edge-handle` | The line that resizes a rail |
| `wm-ribbon` | The ribbon |
| `wm-ribbon-btn` | Every ribbon button. `ribbonBtn` carries it, so the app's rail chrome has it too |
| `wm-ribbon-toggle` | The button that opens and closes the rail |
| `wm-ribbon-cmd` | A command button (`RibbonCommand`) |
| `wm-ribbon-tab` | The icon of one rail tab |
| `wm-ribbon-tab-slot`, `wm-ribbon-tab-close` | The box of a rail icon, and its close control. The default theme shows the control on hover and on focus. A tab that the policy keeps has no control |
| `wm-ribbon-leading` | The tab icons of the leading branch of the rail, under the toggle. The default theme gives it no look |
| `wm-ribbon-trailing` | The tab icons of the trailing branch (the `second` half of the root split), at the far end of the ribbon. The default theme gives it no look |
| `wm-ribbon-tail` | The pinned cluster at the far end of the ribbon |
| `wm-icon-letter` | The first letter of a title, when a tab has no icon |
| `wm-pane-marker` | The marker of one pane in a rail column |
| `wm-pane-boundary` | The ribbon line that drags the boundary between two panes |
| `wm-pane` | A pane |
| `wm-drop-veil` | The veil over a pane during a tab drag |
| `wm-drop-zone` | An edge zone of a pane. A drop there splits the pane |
| `wm-no-tab`, `wm-no-tab-label` | A pane whose group has no active tab |
| `wm-splitter` | The line between the two halves of a split |
| `wm-empty-pane`, `wm-empty-card`, `wm-empty-title`, `wm-empty-hints`, `wm-empty-hint`, `wm-empty-kbd` | The empty pane and its hints |
| `wm-tabbar`, `wm-tabstrip`, `wm-tabbar-actions` | The tab bar, its scroll row and its button group |
| `wm-tabbar-btn` | The pop-out button and the button that lists every tab |
| `wm-tabbar-drop-line` | The line under a tab bar during a tab drag |
| `wm-tab-insert` | The mark that shows where a moved tab goes |
| `wm-tab` | A tab |
| `wm-tab-lead` | The box at the start of a tab that holds the icon and the grip |
| `wm-tab-icon`, `wm-tab-grip` | The icon of a tab, and the grip that replaces it on hover. The whole tab is the drag handle, so a theme can hide the grip |
| `wm-tab-title` | The title of a tab |
| `wm-tab-dot` | The mark of unsaved work |
| `wm-tab-close` | The close button of a tab |
| `wm-tab-menu`, `wm-tab-menu-item`, `wm-tab-menu-dot` | The list of every tab |
| `wm-floating`, `wm-floating-titlebar`, `wm-floating-title`, `wm-floating-actions`, `wm-floating-btn`, `wm-floating-body` | A floating window. `wm-floating-btn` is each control of the window, in every place |
| `wm-window-controls` | The controls of a floating window outside its title bar: in its tab bar, or in the nav bar of a tabless content (`floatingChrome: 'merged'`) |
| `wm-minimized-bar`, `wm-minimized-btn` | The bar of rolled-up floating windows |

The right-click menu of a tab uses the shared `menuContent` and `menuItem`
styles of `components/ui/`, as every menu in the app does. The theme does not
style it.

### States

A state is a data attribute on its part, never a colour class. The component
writes the raw state. The theme decides which state wins.

| Attribute | Parts | Meaning |
|---|---|---|
| `data-active` | `wm-tab`, `wm-tab-menu-item` | The active tab of its group |
| `data-overflow-start`, `data-overflow-end` | `wm-tabstrip` | Tabs extend beyond that scroll edge; the theme fades only clipped ends |
| `data-focused` | `wm-tab` | The pane of the tab has focus |
| `data-modified` | `wm-tab` | The tab holds unsaved work |
| `data-dragging` | `wm-tab`, `wm-ribbon-tab`, `wm-splitter`, `wm-pane-boundary` | The element moves now |
| `data-overflows` | `wm-tab-title` | The box cuts the title |
| `data-highlighted` | `wm-ribbon-tab` | The tab is active, its pane is not folded, and its rail is shown (see `data-edge-shown`) |
| `data-edge-shown` | `wm-edge-host` | Some of the rail's body is on screen: the rail is open, or it still slides shut. It comes on at the start of an opening slide and goes off at the end of a closing slide, so a theme's rail colours grow out of the icon and leave with the body (`components/rail-shown.ts`) |
| `data-unavailable` | `wm-ribbon-tab` | The policy gives a reason that the tab cannot open |
| `data-content-type` | `wm-tab`, `wm-ribbon-tab` | The app's content type of the tab, for a theme that styles one kind of tab |
| `data-orientation` | `wm-ribbon-tab` | `vertical` or `horizontal` |
| `data-collapsed` | `wm-pane-marker` | `true` or `false`: the pane shows its tab bar only |
| `data-locked` | `wm-splitter`, `wm-pane-boundary` | A collapsed or empty side holds the split |
| `data-drop-active` | `wm-pane`, `wm-drop-zone`, `wm-tabbar`, `wm-ribbon` | A tab drag is over the element |
| `data-drop-over` | `wm-pane`, `wm-ribbon` | A native drag is over the element. The app sets it (`DROP_OVER_ATTR`) |
| `data-ribbon-floor` | `wm-ribbon-tail` | The tail claims the far end of the ribbon |

The default theme gives no tint to `data-drop-active` on `wm-tabbar` and
`wm-ribbon`. The old classes gave none either, because the background colour
of the bar won over the tint. A theme may add a tint.

### Layout contracts

The components set these sizes. A script reads them, so a theme must not
change them.

- **The ribbon width.** The ribbon column is 40px wide (`w-10`) and its
  border is 1px wide (`border-r` or `border-l`). `RIBBON_WIDTH_PX` (41) in
  `Ribbon.tsx` holds the sum. A closed rail with `ribbonPlacement: 'panel'`
  keeps this width. A theme may change the border colour only.
- **The tab bar height.** The tab bar is 36px high (`h-9`), and the border is
  inside that height. `COLLAPSED_PANE_PX` (`model/pane-collapse.ts`) gives a
  collapsed pane this height. `MARKER_PX` (`RibbonPaneStrip.tsx`) gives a
  pane marker this height. The rail toggle has the same height, so it lines
  up with the topmost tab bar.
- **The tab trailing slot.** A tab keeps `pr-5`. That padding is the room for
  the close slot (`right-1 w-4`).

A script that measures an element measures the DOM, so a theme may change the
other sizes. For example, it may change a tab padding or a button height.

### Published geometry

The ribbon measures the rail and writes the result as custom properties on
the `wm-ribbon` element. Each value is in px, from the top of the ribbon. A
theme reads them to place a part. The default theme reads none of them.

| Property | Value |
|---|---|
| `--wm-trailing-pane-top` | The top edge of the first pane of the trailing branch. It follows the split during a drag and on a resize. It is absent for a rail of one pane |
| `--wm-ribbon-ceiling` | The bottom of the leading run: the toggle, the leading tab icons and the `railHead` slot |
| `--wm-ribbon-floor` | The top of the pinned tail (`wm-ribbon-tail`) |
| `--wm-ribbon-trailing-height` | The height of `wm-ribbon-trailing` |
| `--wm-edge-progress` | On `wm-edge-host`, not on the ribbon: how far the rail is open, from 0 (shut) to 1 (open), on every frame of its slide. A theme fades or mixes a rail colour with it, so the colour moves with the body |

`rail-geometry.ts` holds the measure loop, which `RibbonPaneStrip` also uses.
A ResizeObserver watches the body, the panes and the clusters, and an effect
measures again when the tree changes its shape.

By default, `wm-ribbon-trailing` sits at the far end of the ribbon. To put the
trailing tab icons at the top edge of their pane, a theme adds this rule:

```css
.wm-ribbon-trailing { position: absolute; inset-inline: 0; top: var(--wm-trailing-pane-top); }
.wm-ribbon-tail { margin-top: auto; }
```

The second line keeps the tail at the far end, because the cluster left the
flow. To keep the cluster clear of the leading run and of the tail, write the
top as `clamp(var(--wm-ribbon-ceiling), var(--wm-trailing-pane-top),
calc(var(--wm-ribbon-floor) - var(--wm-ribbon-trailing-height)))`. The pane
marker of that pane sits at the same top, so a theme can add `36px`
(`MARKER_PX`) to put the icons under the marker. When the cluster leaves the
flow, the pane markers stop at the tail and not at the cluster.

### The gate

`components/__tests__/theme-gate.test.tsx` renders the window manager with
open rails, a split centre, tabs, a modified tab, a floating window and a
rolled-up window. A second case renders the `merged` window controls. It
walks every element in the DOM. It fails when an element
carries a class that sets a look: a colour, a border colour, a radius, a
shadow, a ring, an outline, a font size or weight, an opacity, a transition,
or a variant of one of these. It also fails when the walk does not find one
of the parts, so the check cannot pass on an empty tree.

## How to add a windowing feature

1. Decide whether the change is a layout mechanic (belongs in
   `src/windowing/`) or a product decision (belongs in the app — a new
   `WindowPolicy` member's implementation, a new slot, a new content type).
2. If it is a mechanic, write it under `src/windowing/`. Give each new
   element that has a look a `wm-<part>` class and its state as data
   attributes, and put its look in `theme.css`. Run
   `bunx vitest run src/windowing/__tests__/boundary.test.ts` to confirm it
   imports nothing from the app, export a new name from `index.ts` only when
   app code needs it, and add its unit test under
   `src/windowing/__tests__/` (or `components/__tests__/`).
3. Prove the mechanic against `/windowing-harness.html`
   (`src/test-harness/windowing-harness.tsx`, dev-served only, never built
   into `dist/`), which mounts the core with `windowing/testing/neutralPolicy`
   — three generic content types, no registry, no rails rule, no server. Add
   or extend a spec under `e2e/windowing/`.
4. If it is a product decision, change the app's `WindowPolicy`
   (`stores/windowStore.ts`), its slots (`src/components/shell/windowSlots.tsx`)
   or its content types, and prove it against the real app in `e2e/` (for
   example `e2e/fixed-rails.spec.ts` for the fixed-rail rule).
5. Run `bun run typecheck && bunx vitest run`, then
   `bunx playwright test --project=ui --reporter=line`.

## See Also

- [[Meta/Web User Stories#WS-325]] — the story this folder shape and harness satisfy.
- [[Meta/Web User Stories#WS-324]] — the fixed-rail policy the app layers on top.


## Shell presentation and review

### Production integration

`src/shell-theme.css` owns the production palette and chrome. It extends the core theme without putting app decisions in the core.
`components/DiffRows.tsx` owns the numbered, syntax-highlighted permission
preview rows and inline changes.
The review editor continues to use `lib/merge-view.ts` and the existing comment
owner. Its file Comment action sends a whole-file range through the same
mutation, with root, source, side and path preserved. Whole-file comments render
below the file so collapsed context cannot hide them.

The default layout stacks Sessions above Files on the left. Conversations open
on the opposite rail above the folded terminal, leaving documents in the centre.
`lib/session-actions.ts` chooses that rail by role, including after a side swap.
Reopening an existing conversation activates it where the user placed it.
The session header can expand its rail; the centre stays mounted throughout.
`stores/fixedRails.ts` migrates the former two-pane Files/Terminal skeleton,
keeping tab identities and centre content; other saved arrangements remain.
Missing navigation panels are restored together on the navigation rail.
App tab activation also reveals a folded owning pane, including when selecting
an existing or new session from navigation.
Selecting a ribbon tab unfolds both its rail and its pane in one click,
including the default folded terminal. `TabContextMenu` also exposes Fold pane
and Unfold pane for rail tabs, so pane folding stays accessible when the theme
hides pane markers. `layoutActions.canCollapsePane` shares the last-expanded-pane
guard with `setPaneCollapsed`; folding never closes tabs or stows the rail.

Appearance changes update CSS variables, then notify the theme owner's palette
revision so canvas terminals and graphs repaint. Editors keep their buffers and
undo history. Dark/light continues through `lib/theme.ts`, including OS choice
and the existing rail toggle. This is browser presentation; the TUI and daemon
continue to use the same session and transcript protocols.

The agreed design is integrated into the production components. The selected
adjustable controls live in the existing
`AppearanceSettingsSection` (`components/settings/AppearanceSettings.tsx`),
rendered by `SettingsModal`, with persistence through `SettingsContext` and
`lib/settings.ts`. The integration adds no Look panel or parallel settings store.

The chosen fixed style uses leaf tabs and a right-hand card, with no optional
edges, inner lines or shadows. Theme, true black, contrast, navigation tint,
an RGB accent, pane gap and radius, note text size and file-label presentation
remain configurable. Production preferences persist through the existing browser settings. The
temporary shell mockup, its fixture store and experimental Look toolbox are
retired. Optional visual comparisons use a frozen reference in a separate
worktree; production harnesses mount production components.

The full existing editor remains in place. Source mode shows line numbers;
live preview hides them and retains paragraph joining, readable-width wrapping
and cursor motion. Integration preserves the existing query owners for daemon
entities, the transcript store for daemon-folded transcript operations, and
the editor's buffers, saves, selection and undo history. The app reads those owners directly.

Production review and permission previews share numbered monospace rows with
addition/removal markers and word-change emphasis. Syntax colors remain visible
for code. The review editor owns stale proposals, conflict settlement and
line-range comments. Its file comment action preserves root, source, side and
path through the existing mutation.

Permission prompts use a raised surface with an inset base-surface diff.
Transcript copy and regenerate controls appear once at the end of a completed
assistant turn. Interim text before a tool or permission request reserves no
action row. Copy collects assistant text up to the user-message boundary.

The dev-only `/review-harness.html` mounts the production `DiffPanel` with
local RPC replies. It shares a two-hunk Rust fixture with the browser tests.
The production merge theme uses stronger row tints, caret headers without
`@@` delimiters, and borderless line comments with a soft textarea background.
The preview supports unified/split layouts, folding, and local comments; it
does not send requests to the daemon.

The tab strip reveals the active tab after its own width changes as well as
after selection changes. It measures after the overflow controls lay out and
scrolls only the strip, keeping the selected tab clear of those controls.
The production theme aligns file and hunk carets on one axis.

The tab strip measures its clipped edges on scrolling, resizing, tab changes,
and font loading. Overflow data attributes drive the theme's gradient mask,
which fades tabs without covering neighboring controls and disappears when
the tabs fit.

The desktop note toolbar uses `components/editor/NoteViewSwitch.tsx` to drive the existing `EditorWithPreview` mode owner.
Controls occupy a separate row above the document. Controlled compact editors
use their app-bar Read/Write controls instead.

Document history lives in file-tab metadata and is changed by `lib/file-actions.ts`
through the tab host. `EditorContext` retains buffers owned by history and tracks
their dirty state for tab-close confirmation; CodeMirror retains per-file editor
state so undo cannot cross files. `HistoryNav`, `Breadcrumb` and `NoteViewSwitch`
are production-owned views. File and wikilink gestures use
one file-open intent mapping for in-place, new-tab and split navigation.

Files uses one lazy filesystem tree and the existing `useListDir` cache for all
admitted root kinds. Empty directories and non-note assets are filesystem entries,
not synthetic note-index rows. The shared filesystem event route invalidates
listings once; each pane rebuilds its expanded children from that cache without
invalidating again. Explicit Refresh covers external files outside the kiln
watcher's indexable-format filter. Revealing a nested file awaits each ancestor
listing.
Centre/rail swapping updates placement through the existing editor and session
pane selectors, so subsequent opens follow the swapped regions.

Queued prompt actions extend the browser's existing shared transcript queue.
Removing an unsent prompt removes its optimistic turn; prioritizing it requests
cancellation and lets the normal idle drain claim it once across mounted panes.
Session menu actions use the session context; note/tool links use canonical
resolution/file actions; permission scopes use the interaction response owner.

Navigation controls use the shared configurable control radius,
and menus use the pane radius. `menu-style.ts` and `ChipSelect` share the
`shell-popup` tonal surface without borders or shadows; menu groups use spacing.
Files and Sessions rows use these same corners in desktop rails and phone drawers.

The app exposes one swap button: conversation and editor exchange through
`swapConversationAndEditor`, which selects the right-rail subtree without Terminal.
The core `swapCentreWithEdge` accepts that optional subtree id without knowing content types.
Terminal and the left navigation rail stay in place. The generic core side-swap button remains the fallback
when no application `railTail` slot is provided.

Ribbon tab sorting uses the same insertion-index and pending-drop owner as
horizontal tab bars, with a vertical axis and per-group rows. The ribbon paints
a horizontal insertion marker at the committed drop position. Each drag surface
clears only its own pending placement, so a hidden bar cannot cancel a ribbon
drop. The shell centers rail resize handles in the existing pane gutter.

Session selection reuses a conversation tab group even when the user groups
Backlinks, Activity or other supporting tabs with it. It activates the selected
conversation without adding a split, preferring an existing conversation over
an empty group. Navigation groups containing Sessions or Files remain excluded.


### Supporting panels and transcript controls

`PanelShell` and `PanelHeader` share the flat surface and header rules in
`styles/shell/content.css`; supporting panels reuse these rules instead of
maintaining separate boxed chrome. `ToolCallRow` presents the daemon's canonical
tool kind as a compact action/target row. `ToolCard` retains expansion, results,
permissions and diff navigation. `ThinkingBlock` uses the same row geometry and
adjacent left-aligned caret, with a brain icon. Assistant parts share one spacing token.

`RibbonTabButton` folds its pane when a sibling remains expanded; the final
expanded pane toggles the rail. `closedPanels` includes individually folded panes,
so the Layout menu restores their existing tab through `tabHost.activate`.

`markdown-click.ts` routes absolute file links through `file-actions.ts`, splitting
an optional `:line` suffix into the file-open intent. `openFileAtLine` delegates to
that same owner. Relative note links retain kiln lookup. `lib/clipboard.ts` owns
the Clipboard API and HTTP-compatible selection fallback, restoring focus and
selection after copying; transcript actions report failures rather than silently
claiming success.

The model query refreshes session records after the ACP handshake has populated
the selected model. Manual refresh invalidates the requested session's key,
rather than refetching an observer that may still hold the previous selection.
The session event route invalidates model and session queries on `model_switched`.

File-tab context actions come from the optional `tabMenuActions` app slot.
`treeRootStore` holds the reveal intent until FilesPanel can select its root,
load ancestors and focus the file. Tree selection uses the default file-open intent
(new/existing editor tab), while note links explicitly request in-place navigation.
Tree guides use a navigation ink/surface blend independently of decorative hairlines.

Rail geometry publishes actual active-tab/pane edge alignment. The theme applies
top/bottom join exceptions only at those measured edges; middle tabs retain both
curves, independently of order and which split subtree owns them.

After swapping, a right rail containing only an empty editor and folded Terminal
collapses automatically. An expanded Terminal keeps it open; opening a file
reveals the editor rail again.

Backlinks and Activity are absent from the default layout. Open them through
**Layout → Re-add pane** when needed; saved user layouts retain their chosen tabs.

The model chip renders the authoritative session model. Native catalogue selection
uses the configured provider key from the shared session detail query; ACP model
identifiers remain opaque. This preserves the dropdown checkmark after refresh.

Centre splitter gutters use the shared shell gap in both axes; the resize mark
is centred in that gutter. Tool filenames use the existing note resolver for
kiln-relative paths and file actions for absolute paths, preserving transcript
kiln ownership and the standard tab/split gesture mapping.
