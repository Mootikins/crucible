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
  | 'skills'
  | 'plugins'
  | 'activity'
  | 'backlinks'
  | 'graph';

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
}

export interface SplitNode {
  id: string;
  type: 'split';
  direction: SplitDirection;
  first: LayoutNode;
  second: LayoutNode;
  splitRatio: number;
}

export type LayoutNode = PaneNode | SplitNode;

// Edge panel types
export type EdgePanelPosition = 'left' | 'right' | 'bottom';

export type FocusedRegion = EdgePanelPosition | 'center';

export interface EdgePanel {
  id: string;
  tabGroupId: string;
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

// TabBar props discriminated union
export type TabBarProps =
  | { mode: 'center'; groupId: string; paneId: string; onPopOut?: () => void }
  | { mode: 'edge'; position: EdgePanelPosition };

export interface TabContentProps {
  tab: Tab;
  groupId: string;
}
