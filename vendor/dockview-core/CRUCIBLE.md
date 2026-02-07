# dockview-core (Crucible Fork)

**Source:** https://github.com/mathuo/dockview
**Upstream version:** 4.13.1
**Reason:** Add "docked" as 4th location type for sliding side panels

## Patches Applied

### 1. Added "docked" location type

**Files modified:**
- `src/dockview/dockviewGroupPanelModel.ts`
  - Added `DockedSide = 'left' | 'right' | 'top' | 'bottom'` type
  - Extended `DockviewGroupLocation` with `{ type: 'docked', side: DockedSide }`
  - Updated location setter to handle 'docked' case (dropTarget center-only, CSS class)

**Files created:**
- `src/dockview/dockviewDockedGroupPanel.ts`
  - New class wrapping DockviewGroupPanel for docked panels
  - Manages side, size, collapsed state
  - CSS transitions for smooth expand/collapse

### 2. DOM structure for docked panels

**Files modified:**
- `src/dockview/dockviewComponent.ts`
  - Added `_dockedGroups: DockviewDockedGroupPanel[]` array
  - Added DOM containers: `_dockedLeftContainer`, `_dockedRightContainer`, `_centerContainer`, `_dockedTopContainer`, `_dockedBottomContainer`, `_gridContainer`
  - Restructured DOM in constructor: element > [left, center[top, grid, bottom], right]
  - Moved `overlayRenderContainer` to component.element (floating panels overlap docked)
  - Added `addDockedGroup()`, `removeDockedGroup()`, `getDockedGroups()`, `toggleDockedSide()` methods
  - Added `_getDockedContainer()` helper to route to correct container

### 3. Serialization support

**Files modified:**
- `src/dockview/dockviewComponent.ts`
  - Added `DockedGroupOptions` interface (side, size, collapsed, index)
  - Added `SerializedDockedGroup` interface (data, side, size, collapsed)
  - Extended `toJSON()` to serialize `_dockedGroups` array
  - Extended `fromJSON()` to deserialize docked groups
  - Backward compatible: `data.dockedGroups ?? []` defaults to empty array

### 4. Location transitions (DnD support)

**Files modified:**
- `src/dockview/dockviewComponent.ts`
  - Updated `moveGroupOrPanel()` to handle docked → grid transitions
  - Updated `moveGroup()` to handle grid/floating → docked transitions
  - Added docked group creation in moveGroup() after floating handling
  - Updated `doRemoveGroup()` to handle docked groups (with bugfix: return dockedGroup.group not floatingGroup.group)

### 5. CSS transitions

**Files modified:**
- `src/dockview/dockviewDockedGroupPanel.ts`
  - Added `transition: flex-basis 200ms ease-out` to element
  - `setCollapsed()` updates flex-basis (0 vs size) and overflow
  - `setSize()` updates flex-basis when not collapsed
  - Initial state applied in constructor

### 6. Location-type switches updated

**Files modified:**
- `src/dockview/dockviewComponent.ts`
  - Line 880: addPopoutGroup switch (remove docked group)
  - Line 1904: conditional (allow docked in center merge)
  - Line 2180: removeGroup (dispose docked group)
  - Line 2581: moveGroupWithoutDestroying switch (dispose docked)
  - Line 2648: popout location update (set docked location with side)
  - Line 2815-2821: moveGroup location setter (docked case)
  - Line 2893-2902: moveGroup docked group creation

- `src/dockview/components/titlebar/tabs.ts`
  - Line 198: prevent drag from single-panel docked group

All changes marked with `// NOTE(crucible): docked pane support` comments.

## Updating

To pull in upstream changes:

```bash
cd vendor/dockview-core
git init  # if needed
git remote add upstream https://github.com/mathuo/dockview
git fetch upstream
git diff upstream/master -- packages/dockview-core/src/
# Apply any new fixes manually, preserving our patches
```

## Build Configuration

The web package.json uses a file: dependency:

```json
"dockview-core": "file:../../../vendor/dockview-core"
```
