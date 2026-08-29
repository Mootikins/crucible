import type { Component } from 'solid-js';

// Tab types — every entry is a REGISTERED panel (register-panels.tsx),
// except 'tool', a neutral dummy the windowing unit tests use.
export type TabContentType =
  | 'file'
  | 'tool'
  | 'terminal'
  | 'settings'
  | 'chat'
  | 'chat-draft'
  | 'inbox'
  | 'sessions'
  | 'files'
  | 'search'
  | 'skills'
  | 'plugins'
  | 'activity'
  | 'backlinks'
  | 'graph'
  | 'canvas';

export interface Tab {
  id: string;
  title: string;
  icon?: Component<{ class?: string }>;
  contentType: TabContentType;
  isModified?: boolean;
  isPinned?: boolean;
  metadata?: Record<string, unknown>;
}

export interface TabGroup {
  id: string;
  tabs: Tab[];
  activeTabId: string | null;
}

// Pane types
export type SplitDirection = 'horizontal' | 'vertical';

export interface PaneNode {
  id: string;
  type: 'pane';
  tabGroupId: string | null;
  /**
   * Collapsed to its tab strip, independently of the rail's own collapse.
   *
   * The flag lives on the NODE, not in a set of pane ids on the EdgePanel,
   * because the node is the one thing every layout transform already carries.
   * `mirrorLayout` returns leaves untouched, so a flip keeps it for free;
   * `insertPaneRelative`, `collapseEmptyNodes` and `updateRootWhere` rebuild
   * trees around leaves; and `serializeLayout` deep-clones the tree, so it
   * persists with no new field. A side set would need its own mirror rule, its
   * own serialized slot, and a sweep for ids no pane owns any more.
   *
   * Optional, not required: a v7 layout deserializes unchanged, and an absent
   * flag means expanded.
   */
  collapsed?: boolean;
}

interface SplitNode {
  id: string;
  type: 'split';
  direction: SplitDirection;
  first: LayoutNode;
  second: LayoutNode;
  splitRatio: number;
}

export type LayoutNode = PaneNode | SplitNode;

// Edge panel types
/**
 * The two docks. There was a third, `bottom`, a full-width dock that held the
 * terminal — so showing a shell cost the EDITOR its height, for a tool that
 * belongs beside the files it runs against. The terminal moved into the file
 * rail as a pane under the tree; nothing else ever lived down there.
 *
 * Two values, not three-with-one-unused: a dock nothing can reach is a drop
 * target, a ribbon, a toggle, a shortcut and a serialized slot that all still
 * have to be maintained.
 */
export type EdgePanelPosition = 'left' | 'right';

export type FocusedRegion = EdgePanelPosition | 'center';

export interface EdgePanel {
  id: string;
  /** Panel content is a full binary layout tree (same shape as the center
   * tiling) — leaves are PaneNodes referencing tab groups, so edge panels
   * split exactly like the center area. */
  layout: LayoutNode;
  isCollapsed: boolean;
  width?: number;
  height?: number;
}

// Floating window types
export interface FloatingWindow {
  id: string;
  tabGroupId: string;
  x: number;
  y: number;
  width: number;
  height: number;
  isMinimized: boolean;
  isMaximized: boolean;
  zIndex: number;
  title?: string;
  /** Hover-spawned popover: auto-closes on hover-away and is excluded from
   * layout persistence. Pinning (or dragging/resizing — Hover Editor's
   * auto-pin) clears it, promoting the popover to a normal window. */
  transient?: boolean;
  /** false hides the tab bar (compact hover-editor look); the titlebar
   * toggle brings it back for native tab drag-and-drop. */
  showTabBar?: boolean;
  /** Bounds to restore when un-maximizing. */
  restoreBounds?: { x: number; y: number; width: number; height: number };
}

// Drag and drop types
// 'tab' moves an existing tab between groups; 'newTab' spawns a tab that has
// no source group yet (e.g. dragging a file out of a wikilink hover card) —
// every drop target treats both alike, so any surface can join the window
// system by carrying a Tab payload.
export type DragSource =
  | { type: 'tab'; tab: Tab; sourceGroupId: string }
  | { type: 'newTab'; tab: Tab };

export type DropTarget =
  | {
      type: 'pane';
      paneId: string;
      position?: 'center' | 'left' | 'right' | 'top' | 'bottom';
    }
  | { type: 'tabGroup'; groupId: string; insertIndex?: number }
  | { type: 'edgePanel'; panelId: EdgePanelPosition; insertIndex?: number }
  // `at` = viewport point to spawn the window at (hover-card tear-off).
  | { type: 'newFloating'; at?: { x: number; y: number } };


// ---------------------------------------------------------------------------
// File-tree drag-and-drop lives in `@/lib/file-dnd` (pragmatic-drag-and-drop,
// native HTML5 drags) — NOT in this solid-dnd pipeline. The original Phase-2
// plan (a second solid-dnd provider) was superseded: solid-dnd only matches
// within its nearest provider, which is exactly what blocks the cross-surface
// drops the feature needs (tree → pane open, tree → editor insert). The two
// systems coexist because solid-dnd is pointer-event based and pragmatic is
// native dragstart/drop.
// ---------------------------------------------------------------------------

// One TabBar for every region: edge panels host the same Pane/TabBar stack
// as the center tiling (the bar derives its region from the group).
export interface TabBarProps {
  groupId: string;
  paneId: string;
  onPopOut?: () => void;
}

