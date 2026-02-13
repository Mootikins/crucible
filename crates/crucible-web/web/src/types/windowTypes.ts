import type { Component } from 'solid-js';

// Tab types
export type TabContentType =
  | 'file'
  | 'document'
  | 'tool'
  | 'terminal'
  | 'preview'
  | 'settings';

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
export type DragSource =
  | { type: 'tab'; tab: Tab; sourceGroupId: string; sourcePaneId?: string }
  | { type: 'tabGroup'; groupId: string; sourcePaneId?: string };

export type DropTarget =
  | {
      type: 'pane';
      paneId: string;
      position?: 'center' | 'left' | 'right' | 'top' | 'bottom';
    }
  | { type: 'tabGroup'; groupId: string; insertIndex?: number }
  | { type: 'edgePanel'; panelId: EdgePanelPosition; insertIndex?: number }
  | { type: 'floatingWindow'; windowId: string }
  | { type: 'empty'; position: EdgePanelPosition }
  | { type: 'newFloating' };

// Window manager state
export interface WindowManagerState {
  layout: LayoutNode;
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, EdgePanel>;
  floatingWindows: FloatingWindow[];
  activePaneId: string | null;
  focusedRegion: FocusedRegion;
  dragState: {
    isDragging: boolean;
    source: DragSource | null;
    target: DropTarget | null;
  } | null;
  flyoutState: {
    isOpen: boolean;
    panelPosition: EdgePanelPosition;
    tabId: string | null;
  } | null;
  nextZIndex: number;
}

// Action types
export interface WindowManagerActions {
  addTab: (groupId: string, tab: Tab, insertIndex?: number) => void;
  removeTab: (groupId: string, tabId: string) => void;
  setActiveTab: (groupId: string, tabId: string | null) => void;
  moveTab: (
    sourceGroupId: string,
    targetGroupId: string,
    tabId: string,
    insertIndex?: number
  ) => void;
  updateTab: (groupId: string, tabId: string, updates: Partial<Tab>) => void;
  createTabGroup: (paneId?: string) => string;
  deleteTabGroup: (groupId: string) => void;
  splitPane: (paneId: string, direction: SplitDirection) => void;
  splitPaneAndDrop: (
    paneId: string,
    position: 'left' | 'right' | 'top' | 'bottom',
    sourceGroupId: string,
    tabId: string
  ) => void;
  setActivePane: (paneId: string | null) => void;
  toggleEdgePanel: (position: EdgePanelPosition) => void;
  setEdgePanelCollapsed: (
    position: EdgePanelPosition,
    collapsed: boolean
  ) => void;
  setEdgePanelActiveTab: (
    position: EdgePanelPosition,
    tabId: string | null
  ) => void;
  setEdgePanelSize: (position: EdgePanelPosition, size: number) => void;

  openFlyout: (position: EdgePanelPosition, tabId: string) => void;
  closeFlyout: () => void;
  createFloatingWindow: (
    tabGroupId: string,
    x: number,
    y: number,
    width?: number,
    height?: number
  ) => string;
  removeFloatingWindow: (windowId: string) => void;
  updateFloatingWindow: (
    windowId: string,
    updates: Partial<FloatingWindow>
  ) => void;
  bringToFront: (windowId: string) => void;
  minimizeFloatingWindow: (windowId: string) => void;
  maximizeFloatingWindow: (windowId: string) => void;
  restoreFloatingWindow: (windowId: string) => void;
  dockFloatingWindow: (windowId: string, targetPaneId?: string) => void;
  startDrag: (source: DragSource) => void;
  setDropTarget: (target: DropTarget | null) => void;
  endDrag: () => void;
  executeDrop: () => void;

  getTabGroup: (groupId: string) => TabGroup | undefined;
  getPaneTabGroupId: (paneId: string) => string | null;
  findPaneById: (paneId: string) => PaneNode | null;
}

export type WindowManagerStore = WindowManagerState & WindowManagerActions;

// TabBar props discriminated union
export type TabBarProps =
  | { mode: 'center'; groupId: string; paneId: string; onPopOut?: () => void }
  | { mode: 'edge'; position: EdgePanelPosition };

export interface TabContentProps {
  tab: Tab;
  groupId: string;
}
