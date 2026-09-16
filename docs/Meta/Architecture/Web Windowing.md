---
title: Web Windowing
description: The domainless windowing core under src/windowing, its WindowPolicy seam, its edge modes, and its own harness and specs.
tags: [meta, architecture, web, windowing]
status: as-built
---

# Web Windowing

The web frontend's window manager — edge panels, split panes, tabs, floating
windows — lives in `crates/crucible-web/web/src/windowing/`. It knows no
product concept: no session, no file, no terminal. The app configures it once
with one `WindowPolicy` object, from `stores/windowStore.ts`. Paths below are
relative to `crates/crucible-web/web/`.

## The folder

| Path | Contents |
|---|---|
| `model/` | `WindowState` and the node types (`types.ts`), the tree helpers (`tree.ts`), the pane and collision helpers (`pane-content.ts`, `pane-collapse.ts`, `pane-boundaries.ts`, `collision-detector.ts`, `layout-restore.ts`, `tab-guards.ts`), and the v10 layout serializer (`serializer.ts`) |
| `store/` | The Solid store, the `WindowPolicy` type (`policy.ts`), and the tab, layout and floating-window actions |
| `components/` | `WindowManager`, `EdgeHost`, `Ribbon`, `DockedBody`, `Pane`, `SplitPane`, `CenterTiling`, `TabBar`, `FloatingWindow`, `EmptyPane`, `MinimizedBar`, `RibbonPaneStrip`, plus `split-drag.ts` and `tab-placement.ts` |
| `reveal/` | `RevealController` and `flyoutRect`, the two pure/reactive rules a hover reveal needs |
| `context-menu.ts` | The native-fallthrough rule for a custom right-click menu (Shift+right-click, images, links) |
| `shortcuts.ts` | `ShortcutAction`, `matchShortcut`, `LAYOUT_SHORTCUTS` — the chords the layout owns |
| `testing/` | `neutralPolicy` — a policy with no product knowledge, shared by the core's own unit tests and the harness page |
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
| `iconFor(contentType)` | `iconForContentType` in the layout actions | `iconForContentType` (`lib/tab-icons.ts`) |
| `unavailableReason(tab)` | The terminal-availability check and its rail tooltip | An inline closure that checks `terminalAllowed()` (`lib/terminal-availability.ts`) |
| `layoutHooks` | The legacy migrations to v9, the registry prune, the icon restore | `appLayoutHooks` (`stores/layoutMigrations.ts`) |
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
(`windowing/components/context.tsx`), which every member is optional:

- `railHead(position)` — above the tab icons on a rail (the layout menu, left rail only).
- `railTail(position)` — pinned to the far end of a rail (the offline badge, the theme and settings buttons, the notification bell).
- `corner()` — the floating cluster at the bottom-right of the centre (`CornerBar`).
- `emptyPaneHints()` — the rows an empty pane prints under its label.
- `attachDropTarget(el, groupId)` — attaches a native HTML5 drop target to a pane body or ribbon, for the group `groupId` names; returns the cleanup.

The app builds all five in `src/components/shell/windowSlots.tsx`
(`appWindowSlots`), backed by `RailChrome.tsx`, `CornerBar.tsx`,
`lib/keyboard-shortcuts.ts` and `lib/file-dnd.ts`, and hands them to
`WindowManager` in `AppShell.tsx`.

## Edge modes

Each edge panel (`left`, `right`) carries an `EdgeMode`:

| Mode | Ribbon | Body |
|---|---|---|
| `docked` | shown | in the flow, sized by the layout |
| `strip` | shown | collapsed out of the flow |
| `flyout` | shown | floats over the centre until its button is clicked again |
| `hidden` | hidden | hidden; a hot zone on the screen edge reveals the host as a flyout |

`EdgeHost` (`windowing/components/EdgeHost.tsx`) reads `mode` and sets
`data-edge-mode` on its root. `docked` and `strip` share one tree: `DockedBody`
stays mounted in every mode, clipped to zero width in `strip`, so a toggle
slides it and never remounts it — a terminal keeps its scrollback and a file
tree keeps its scroll position across a collapse. `flyout` and `hidden`
currently render the same as `strip`; `EdgeHost` already branches on all four
modes, so the next session's presentation work grows from a proven seam
rather than adding one.

`flyout` and `hidden` are typed, stored and placed, but not yet presented:

- **Types and state.** `EdgeMode` and `EdgeCue` are closed unions
  (`model/types.ts`), checked exhaustively by `types.test.ts` against
  `EDGE_MODES` and `EDGE_CUES`. `windowActions.setEdgeMode(position, mode, opts?)`
  writes the mode and, for `hidden`, the cue; `toggleEdgePanel` still moves only
  between `docked` and `strip`.
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

`PaneNode` also gained `reveal?: 'click' | 'hover'` (`model/types.ts`), for a
pane collapsed to its band inside a rail column; only the type and the field
exist so far, not the hover behavior.

## The saved layout

The core owns the **current** format and the one step into it. The app owns
the **history** before that step, because each earlier version names content
types that only existed at the time.

- `windowing/model/serializer.ts`: `LAYOUT_VERSION = 10`, `serializeLayout`,
  and `deserializeLayout(json, hooks)`. `deserializeLayout` runs, in order: the
  app's legacy upgrade (only when the stored version is below 9), the v9-to-v10
  step (`isCollapsed` becomes `mode: 'strip' | 'docked'`, and the v10 writer
  never stores `isCollapsed`), a rebuild that gives every rail position a valid
  panel even from a partial or hand-edited payload, the app's `prune`, and
  last the app's `iconFor`. A version outside 1–10 throws; an upgrade that does
  not return a v9 payload throws.
- `stores/layoutMigrations.ts`: the v1-to-v9 migration chain, the always-on
  prune of unregistered content types (`pruneRestored` — this is what drops a
  persisted tab for a panel the registry no longer has, such as the retired
  Explorer/Search/Source-Control placeholders), the WS-220 chat-docking move,
  and `appLayoutHooks`, the `LayoutCodecHooks` implementation
  (`upgradeLegacy`, `prune`, `iconFor`) that the app hands the core through
  `WindowPolicy.layoutHooks`.

`layoutActions.importLayout` is the only production caller of the reader; it
passes `appLayoutHooks`.

## How to add a windowing feature

1. Decide whether the change is a layout mechanic (belongs in
   `src/windowing/`) or a product decision (belongs in the app — a new
   `WindowPolicy` member's implementation, a new slot, a new content type).
2. If it is a mechanic, write it under `src/windowing/`, run
   `bunx vitest run src/windowing/__tests__/boundary.test.ts` to confirm it
   imports nothing from the app, and add its unit test under
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
