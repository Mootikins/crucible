---
tags: [spec, web, flexlayout]
---

# FlexLayout Behavioral Spec

Source of truth. Tests verify these behaviors. Code implements them.

---

## Tabs

- Clicking a tab selects it and shows its content.
- Only one tab is selected per tabset at a time.
- Newly added tabs auto-select (unless `autoSelectTab: false`).
- Double-clicking a tab enters rename mode. Enter confirms, Escape cancels.
- Tabs have a close button. Close type controls visibility: always, when selected, or on hover.
- Tabs render on demand — unselected tabs are not mounted (unless `enableRenderOnDemand: false`).
- Scroll position within a tab persists across select/deselect cycles.

## TabSets

- A tabset is a container of tabs with a tab bar and content area.
- Tab bar appears at top (default) or bottom.
- When tabs overflow the tab bar, an overflow button appears with a count. Clicking opens a menu listing hidden tabs. Arrow keys navigate, Escape closes.
- Only one tabset is "active" at a time (visual highlight). Selecting any tab makes its tabset active.
- Maximize toggle: one tabset fills the entire layout; all others collapse to zero. Toggle again to restore.
- When the last tab is closed, the tabset is removed (unless `enableDeleteWhenEmpty: false`).
- Single tab stretch: when enabled and only one tab exists, the tab button stretches to fill the bar and hides the close button.

## Borders

- Four dockable borders: top, right, bottom, left.
- Each border has a tab bar (strip) and an optional content panel.

### Dock States

Borders cycle through three states via the dock button:

```
expanded → collapsed → hidden → expanded
```

- **Expanded**: Tab bar visible. If a tab is selected, content panel is open.
- **Collapsed**: Tab bar shows rotated labels only. No content panel.
- **Hidden**: Nothing visible. If tabs exist, a small FAB arrow appears at the edge. Clicking the FAB expands the border.

### Strip Sizing

- Expanded with a selected tab: strip is 0px (content panel takes over).
- Expanded without selection: strip is 38px (tab buttons visible).
- Collapsed with tabs: strip is 38px (rotated labels).
- Collapsed or hidden with no tabs: strip is 0px (no wasted space).
- Hidden with tabs: strip is 38px (dock button accessible).

### Tab Selection in Borders

- Clicking an unselected border tab selects it and opens its content.
- Clicking an already-selected border tab does nothing (no toggle).
- Dragging an unselected border tab auto-selects it at drag start.
- Dragging the active tab out: the next tab auto-selects.
- Dragging an inactive tab out: current selection is preserved, indices adjust.
- Dragging the last tab out: selection becomes -1 (no selection).

### Auto-Hide

- When enabled: border disappears from the layout when it has zero tabs.
- Dragging the last tab out of a border sets it to hidden.
- Dropping a tab into a hidden or collapsed border auto-expands it.

### Tiling (Split View)

- Borders can show multiple tabs simultaneously, split with splitters.
- Right-click a border tab → "Split with [TabName]" to tile two tabs side by side.
- Right-click a tiled tab → "Untile" to return to single-tab mode.
- N tiled tabs produce N-1 splitters with equal weight.
- Dropping a tab to the left/top of tiled content inserts it first; right/bottom inserts it last.

### Priority & Nesting

- Borders nest around the main content. Higher priority = outermost.
- Default: left/right priority 1 (outer), top/bottom priority 0 (inner).
- Equal priority tie-breaks by location order: top, right, bottom, left.

### Collapsed Labels

- In collapsed state, border tabs show as rotated text labels.
- Labels truncate with ellipsis when too long.
- Bar width (38px) provides enough room for readable text.

## Drag & Drop

### Sources

- Tab buttons (in tabsets and borders).
- TabSet headers (drags entire tabset).
- Overflow menu items.
- External drag (consumer provides tab JSON).

### Drop Targets

- **Center** of a tabset: adds tab to that tabset.
- **Edge** (top/bottom/left/right 25%) of a tabset: splits the tabset, creating a new one at that edge.
- **Border strip**: adds tab to that border.
- **Border content area** (when open): horizontal borders split left/right, vertical borders split top/bottom.
- **Layout edge** (within 10px of outer edge): creates edge dock or restructures layout.

### Drag Behavior

- A drop preview outline animates to show where the tab will land.
- A drag image shows the tab's name.
- Escape cancels any in-progress drag.
- Edge dock zones (small rectangles at layout edges) appear during drag when enabled.
- A dead zone cross in the center prevents accidental edge docking.

### Border-Specific Drag Behavior

- Dragging an unselected border tab auto-selects it before the drag begins.
- Dropping into an empty, hidden, or collapsed border auto-expands it.
- Dragging the last tab out of a border auto-hides it (when auto-hide enabled).
- Dragging the active tab out adjusts the selection to the next tab.

## Floating Panels

- Any tab or tabset can become a floating panel (draggable overlay within the layout).
- Panels have a titlebar for dragging, 8-edge resize handles, a dock button, and a close button.
- Minimum size: 150×80px.
- Clicking a panel brings it to front. Z-order persists through save/load.
- Dock button returns the panel to the main layout.
- Resize can be realtime (live) or outline (ghost rect), per global setting.

## Popout Windows

- Any tab or tabset can pop out to a separate browser window.
- The popout window inherits parent stylesheets.
- Closing the popout returns content to the main window.
- Closing the parent window closes all popouts.

## Splitters

- Appear between sibling nodes in a split.
- Drag to resize adjacent panels. Weights adjust proportionally.
- Min/max size constraints on panels are respected.
- Optional visible handle for easier grabbing.
- Optional extra invisible hit area for easier targeting.

## Serialization

- Layout round-trips through JSON without data loss.
- All node attributes, dock states, visible tabs, float z-order, and window positions are preserved.
- Legacy JSON loads with correct defaults (backward compatible).
- Legacy "minimized" dock state auto-converts to "hidden".
- Malformed JSON (missing fields, null values, empty arrays) is handled gracefully without crashes.

## Keyboard

| Key | Context | Action |
|-----|---------|--------|
| Escape | Renaming a tab | Cancel rename |
| Escape | Popup/context menu open | Close menu |
| Escape | Dragging | Cancel drag |
| Enter | Renaming a tab | Confirm rename |
| Arrow keys | Popup menu | Navigate items |
| Double-click | Tab button | Enter rename mode |

---

## Feature Landscape & Gap Analysis

Comparison of our SolidJS port against the FlexLayout upstream (caplin), Dockview, GoldenLayout, and professional IDE patterns (VS Code, JetBrains).

Legend: ✅ = have it, ⚠️ = partial, ❌ = missing, — = not applicable

### Core Layout

| Feature | Ours | FlexLayout | Dockview | GoldenLayout | VS Code | JetBrains |
|---------|------|------------|----------|--------------|---------|-----------|
| Rows / columns (splits) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Tabsets (stacks) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Borders (edge docks) | ✅ | ✅ | — | — | ✅ | ✅ |
| Floating panels | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| Popout windows | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Nested layouts (submodels) | ❌ | ✅ | ✅ | ❌ | — | — |
| Root orientation toggle | ❌ | ✅ | ✅ | ✅ | — | — |
| Paneview (accordion) | — | — | ✅ | — | ✅ | ✅ |
| Gridview (2D grid) | — | — | ✅ | — | — | — |

### Tabs

| Feature | Ours | FlexLayout | Dockview | GoldenLayout | VS Code | JetBrains |
|---------|------|------------|----------|--------------|---------|-----------|
| Click to select | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Close button | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Drag to reorder | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Drag between tabsets | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Double-click rename | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Render on demand | ✅ | ✅ | ✅ | ❌ | — | — |
| Tab icons | ❌ | ✅ | ✅ | ❌ | ✅ | ✅ |
| Tab tooltips (helpText) | ❌ | ✅ | ❌ | ❌ | ✅ | ✅ |
| Tab wrapping (multi-line) | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Tab location top/bottom | ❌ | ✅ | ❌ | ✅ | ✅ | ❌ |
| Alt name for overflow | ❌ | ✅ | ❌ | ❌ | — | — |
| Preview / ephemeral tabs | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ |
| Pinned tabs | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| Close type (always/hover/selected) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ |
| Custom CSS class per tab | ❌ | ✅ | ❌ | ❌ | — | — |
| Per-tab min/max size | ❌ | ✅ | ✅ | ❌ | — | — |

### TabSets

| Feature | Ours | FlexLayout | Dockview | GoldenLayout | VS Code | JetBrains |
|---------|------|------------|----------|--------------|---------|-----------|
| Active tabset tracking | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Maximize / restore | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| Overflow menu | ✅ | ✅ | ✅ | ✅ | — | — |
| Delete when empty | ✅ | ✅ | ✅ | ✅ | — | — |
| Single-tab stretch | ✅ | ✅ | ✅ | ❌ | — | — |
| TabSet close button | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ |
| Active icon indicator | ❌ | ✅ | ❌ | ❌ | — | — |
| Mini scrollbar | ❌ | ✅ | ✅ | ❌ | — | — |
| Disable tab strip | ❌ | ✅ | ❌ | ❌ | — | — |
| TabSet dragging (whole group) | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ |
| Sticky buttons (Chrome +) | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Header action slots (left/right/prefix) | ❌ | ✅ | ✅ | ❌ | — | — |
| Custom placeholder (empty state) | ❌ | ✅ | ✅ | ❌ | — | — |
| Watermark (empty dock) | ❌ | ❌ | ✅ | ❌ | — | — |

### Borders

| Feature | Ours | FlexLayout | Dockview | GoldenLayout |
|---------|------|------------|----------|--------------|
| 4-edge borders | ✅ | ✅ | — | — |
| Expand / collapse / hide cycle | ✅ | ✅ | — | — |
| Auto-hide on empty | ✅ | ✅ | — | — |
| Tiling (split view) | ✅ | ✅ | — | — |
| FAB arrow for hidden borders | ✅ | ✅ | — | — |
| Priority / nesting order | ✅ | ✅ | — | — |
| Collapsed rotated labels | ✅ | ✅ | — | — |
| Per-tab border size | ❌ | ✅ | — | — |
| Auto-select when open/closed | ⚠️ | ✅ | — | — |
| Border scrollbar | ❌ | ✅ | — | — |

### Drag & Drop

| Feature | Ours | FlexLayout | Dockview | GoldenLayout | VS Code |
|---------|------|------------|----------|--------------|---------|
| Tab drag between tabsets | ✅ | ✅ | ✅ | ✅ | ✅ |
| Edge split on drop | ✅ | ✅ | ✅ | ✅ | ✅ |
| Border drop | ✅ | ✅ | — | — | — |
| Edge dock zones | ✅ | ✅ | ✅ | ✅ | — |
| Drop preview outline | ✅ | ✅ | ✅ | ✅ | ✅ |
| Escape cancels drag | ✅ | ✅ | ❌ | ❌ | ❌ |
| Dead zone cross (center) | ✅ | ✅ | ❌ | ❌ | ❌ |
| External drag source | ❌ | ✅ | ✅ | ❌ | — |
| Drag from overflow menu | ❌ | ✅ | ❌ | ❌ | — |
| Custom drag rect renderer | ❌ | ✅ | ❌ | ❌ | — |
| Drop validation callback | ❌ | ✅ | ✅ | ❌ | — |
| Shift+drag to float | ❌ | ❌ | ✅ | ❌ | ❌ |
| Copy-on-drag (Ctrl) | ❌ | ❌ | ❌ | ❌ | ✅ |
| Preventable drag/drop events | ❌ | ❌ | ✅ | ❌ | — |

### Floating Panels

| Feature | Ours | FlexLayout | Dockview | GoldenLayout |
|---------|------|------------|----------|--------------|
| Drag to create | ✅ | ✅ | ✅ | ❌ |
| Titlebar drag | ✅ | ✅ | ✅ | ❌ |
| 8-edge resize | ✅ | ✅ | ❌ | ❌ |
| Dock-back button | ✅ | ✅ | ✅ | ❌ |
| Z-order management | ✅ | ✅ | ✅ | ❌ |
| Min size constraint | ✅ | ✅ | ❌ | ❌ |
| Viewport bounds | ❌ | ❌ | ✅ | ❌ |
| Realtime vs outline resize | ✅ | ✅ | — | — |

### Splitters

| Feature | Ours | FlexLayout | Dockview | GoldenLayout |
|---------|------|------------|----------|--------------|
| Drag to resize | ✅ | ✅ | ✅ | ✅ |
| Weight-based sizing | ✅ | ✅ | ❌ | ❌ |
| Min/max constraints | ✅ | ✅ | ✅ | ✅ |
| Configurable width | ❌ | ✅ | ✅ | ❌ |
| Extended hit area | ❌ | ✅ | ❌ | ❌ |
| Visible handle | ❌ | ✅ | ❌ | ❌ |
| Realtime resize toggle | ❌ | ✅ | — | — |
| Snap to zero | ❌ | ❌ | ❌ | ❌ |
| Proportional resize | ❌ | ❌ | ✅ | ❌ |

### Serialization

| Feature | Ours | FlexLayout | Dockview | GoldenLayout |
|---------|------|------------|----------|--------------|
| JSON round-trip | ✅ | ✅ | ✅ | ✅ |
| Backward compat (legacy load) | ✅ | ✅ | ❌ | ✅ |
| Graceful malformed JSON | ✅ | ✅ | ⚠️ | ❌ |
| Config minification | ❌ | ❌ | ❌ | ✅ |
| Named layout presets | ❌ | ❌ | ❌ | ❌ |
| Per-workspace persistence | ❌ | ❌ | ❌ | ❌ |

### Theming & Customization

| Feature | Ours | FlexLayout | Dockview | GoldenLayout |
|---------|------|------------|----------|--------------|
| CSS variables | ⚠️ | ✅ | ✅ | ✅ |
| Multiple built-in themes | ❌ | ✅ (6) | ✅ (8) | ✅ |
| Dynamic theme switching | ❌ | ✅ | ✅ | ✅ |
| Class name mapper (CSS modules) | ❌ | ✅ | ❌ | ❌ |
| I18n / string mapper | ❌ | ✅ | ❌ | ✅ |
| Custom tab renderer | ❌ | ✅ | ✅ | ❌ |
| Custom tabset renderer | ❌ | ✅ | ✅ | ❌ |
| Context menu callback | ❌ | ✅ | ❌ | ❌ |
| Aux mouse click (middle/meta) | ❌ | ✅ | ❌ | ❌ |

### Events & API

| Feature | Ours | FlexLayout | Dockview | GoldenLayout |
|---------|------|------------|----------|--------------|
| Action interception | ✅ | ✅ | ✅ | ❌ |
| Model change callback | ✅ | ✅ | ✅ | ✅ |
| Tab resize event | ❌ | ✅ | ✅ | ✅ |
| Tab visibility event | ❌ | ✅ | ✅ | ✅ |
| Tab close event | ❌ | ✅ | ❌ | ✅ |
| Tab save event | ❌ | ✅ | ❌ | ❌ |
| Focus tracking | ❌ | ❌ | ✅ | ✅ |
| Panel location tracking | ❌ | ❌ | ✅ | ❌ |

### Professional IDE Features (aspirational)

| Feature | VS Code | JetBrains | Ours |
|---------|---------|-----------|------|
| Activity bar (icon sidebar) | ✅ | ❌ | ❌ |
| Dual sidebars (primary + secondary) | ✅ | ❌ | ❌ |
| Auto-hide panels (slide in/out) | ❌ | ✅ | ❌ |
| Panel alignment (center/justify) | ✅ | ❌ | ❌ |
| Split-in-group (same file side-by-side) | ✅ | ❌ | ❌ |
| Preview / ephemeral tabs | ✅ | ❌ | ❌ |
| Named layout presets | ❌ | ✅ | ❌ |
| Zen mode / distraction-free | ✅ | ✅ | ❌ |
| Widescreen optimization | ✅ | ✅ | ❌ |
| Per-workspace layouts | ✅ | ✅ | ❌ |
| Pinned tabs | ✅ | ✅ | ❌ |
| Copy-on-drag (modifier key) | ✅ | ❌ | ❌ |
| Tool window view modes (5 modes) | ❌ | ✅ | ❌ |

---

## Priority Tiers

### Tier 1 — Upstream Parity (FlexLayout features we should have)

These exist in the caplin FlexLayout and are missing from our port:

1. **Tab icons** — SVG icons in tab buttons
2. **Tab tooltips** (`helpText`) — hover text
3. **Tab location top/bottom** — tabs at bottom of tabset
4. **TabSet close button** — close entire tabset
5. **TabSet dragging** — drag whole group by header
6. **Sticky buttons** — Chrome-style + button after last tab
7. **Header action slots** — leading/buttons sections in tabset header
8. **External drag source** — accept drag from outside layout
9. **Drop validation** (`onAllowDrop`) — control valid drops
10. **Drag from overflow menu** — drag hidden tabs
11. **Splitter configuration** — `splitterSize`, `splitterExtra`, `splitterEnableHandle`
12. **Realtime resize toggle** — live vs outline splitter drag
13. **Custom tab renderer** (`onRenderTab`)
14. **Custom tabset renderer** (`onRenderTabSet`)
15. **Context menu callback** (`onContextMenu`)
16. **Tab events** — resize, close, visibility, save
17. **Multiple themes** — light, dark, gray, underline, rounded
18. **CSS variable system** — full set of custom properties
19. **I18n mapper** — translatable labels
20. **Class name mapper** — CSS modules support
21. **Per-tab border size** (`borderWidth`, `borderHeight`)
22. **Error boundary with retry** — catch tab render errors
23. **Root orientation vertical** toggle
24. **Active icon** (asterisk indicator on active tabset)
25. **Custom drag rect** (`onRenderDragRect`)
26. **TabSet placeholder** — content when tabset has no tabs
27. **Aux mouse click** — alt/meta/shift/middle click handling
28. **Custom overflow menu** (`onShowOverflowMenu`)
29. **Tab wrapping** — multi-line tab strip
30. **Mini scrollbar** in tab strip and borders

### Tier 2 — Competitive Advantage (features from Dockview / GoldenLayout / IDEs)

1. **Group locking** — prevent DnD into specific containers (Dockview)
2. **Watermark** — custom content for empty dock (Dockview)
3. **Shift+drag to float** — modifier key floats panel (Dockview)
4. **Preventable events** — `onWillDrop`, `onWillDragPanel` (Dockview)
5. **Panel visibility API** — `setVisible(bool)` independent of selection (Dockview)
6. **Panel constraints API** — runtime min/max changes (Dockview)
7. **Focus tracking** — `isFocused` with focus/blur events (Dockview, GoldenLayout)
8. **Panel location tracking** — grid vs floating vs popout (Dockview)
9. **Viewport bounds for floats** — keep panels within viewport (Dockview)
10. **Virtual components** — no reparenting, preserves iframes (GoldenLayout)
11. **Popout windows** — full multi-window support (FlexLayout, Dockview, GoldenLayout)
12. **Named layout presets** — save/restore named arrangements (JetBrains)
13. **Preview / ephemeral tabs** — italic tab that reuses slot (VS Code)
14. **Pinned tabs** — protected from bulk close (VS Code, JetBrains)
15. **Auto-hide with slide animation** — panels slide in/out (JetBrains)

### Tier 3 — Future / Aspirational

1. **Activity bar** — icon-only vertical navigation sidebar (VS Code)
2. **Dual sidebars** — primary + secondary, always opposite (VS Code)
3. **Panel alignment** — bottom panel spans editor only vs full width (VS Code)
4. **Split-in-group** — same file side-by-side within one tab (VS Code)
5. **Zen mode** — hide everything except content (VS Code, JetBrains)
6. **Per-workspace persistence** — each project remembers its layout (VS Code, JetBrains)
7. **Copy-on-drag** — Ctrl/Option to duplicate instead of move (VS Code)
8. **Widescreen optimization** — side-by-side tool windows (JetBrains)
9. **Snap to zero** — panes collapse completely on drag (allotment)
10. **Priority-based resizing** — control which panes resize first (allotment)
11. **Shadow DOM support** — for web component embedding (Dockview)
12. **Framework-agnostic core** — vanilla TS core with framework bindings (Dockview, GoldenLayout)
