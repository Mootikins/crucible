import type { Component } from 'solid-js';

// Tab types
export type TabContentType =
  | 'file'
  | 'document'
  | 'tool'
  | 'terminal'
  | 'preview'
  | 'settings'
  | 'chat'
  | 'home'
  | 'inbox'
  | 'sessions'
  | 'explorer'
  | 'files'
  | 'search'
  | 'skills'
  | 'plugins'
  | 'activity'
  | 'source-control'
  | 'outline'
  | 'backlinks'
  | 'problems'
  | 'output';

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
  | { type: 'newFloating' };


// TabBar props discriminated union
export type TabBarProps =
  | { mode: 'center'; groupId: string; paneId: string; onPopOut?: () => void }
  | { mode: 'edge'; position: EdgePanelPosition };

export interface TabContentProps {
  tab: Tab;
  groupId: string;
}
