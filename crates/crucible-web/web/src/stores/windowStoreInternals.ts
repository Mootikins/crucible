import type { SetStoreFunction } from 'solid-js/store';
import type {
  EdgePanelPosition,
  LayoutNode,
  PaneNode,
  Tab,
  TabGroup,
} from '@/types/windowTypes';
import {
  Activity,
  FolderTree,
  Link2,
  MessageCircle,
  Terminal,
} from '@/lib/icons';
import type { PaneDropPosition, WindowState } from './windowStoreTypes';

export interface WindowStoreContext {
  store: WindowState;
  setStore: SetStoreFunction<WindowState>;
}

export const generateId = () => Math.random().toString(36).substring(2, 11);

export function findPaneInLayout(
  layout: LayoutNode,
  paneId: string
): PaneNode | null {
  if (layout.type === 'pane') {
    return layout.id === paneId ? layout : null;
  }
  return (
    findPaneInLayout(layout.first, paneId) ||
    findPaneInLayout(layout.second, paneId)
  );
}

export function updatePaneInLayout(
  layout: LayoutNode,
  paneId: string,
  updater: (pane: PaneNode) => PaneNode
): LayoutNode {
  if (layout.type === 'pane') {
    if (layout.id === paneId) return updater(layout);
    return layout;
  }
  return {
    ...layout,
    first: updatePaneInLayout(layout.first, paneId, updater),
    second: updatePaneInLayout(layout.second, paneId, updater),
  };
}

export function replacePaneWithSplit(
  layout: LayoutNode,
  paneId: string,
  newSplit: LayoutNode
): LayoutNode {
  if (layout.type === 'pane') {
    if (layout.id === paneId) return newSplit;
    return layout;
  }
  return {
    ...layout,
    first: replacePaneWithSplit(layout.first, paneId, newSplit),
    second: replacePaneWithSplit(layout.second, paneId, newSplit),
  };
}

export function findFirstPane(layout: LayoutNode): PaneNode | null {
  if (layout.type === 'pane') return layout;
  return findFirstPane(layout.first) || findFirstPane(layout.second);
}

export function collapseEmptyNodes(
  layout: LayoutNode,
  tabGroups: Record<string, TabGroup>
): LayoutNode {
  if (layout.type === 'pane') return layout;

  const first = collapseEmptyNodes(layout.first, tabGroups);
  const second = collapseEmptyNodes(layout.second, tabGroups);

  const isEmptyPane = (node: LayoutNode): boolean =>
    node.type === 'pane' &&
    (node.tabGroupId === null || !(node.tabGroupId in tabGroups));

  if (isEmptyPane(first)) return second;
  if (isEmptyPane(second)) return first;

  return { ...layout, first, second };
}

export function insertPaneRelative(
  layout: LayoutNode,
  paneId: string,
  position: PaneDropPosition,
  newPaneId: string,
  newGroupId: string
): LayoutNode {
  const pane = findPaneInLayout(layout, paneId);
  if (!pane) return layout;
  const isHorizontal = position === 'left' || position === 'right';
  const newPane: PaneNode = {
    id: newPaneId,
    type: 'pane',
    tabGroupId: newGroupId,
  };
  const first =
    position === 'left' || position === 'top' ? newPane : pane;
  const second =
    position === 'left' || position === 'top' ? pane : newPane;
  const newSplit: LayoutNode = {
    id: generateId(),
    type: 'split',
    direction: isHorizontal ? 'horizontal' : 'vertical',
    splitRatio: 0.5,
    first,
    second,
  };
  return replacePaneWithSplit(layout, paneId, newSplit);
}

/** In-order tab-group ids at the leaves of a layout tree. */
export function collectLeafGroupIds(layout: LayoutNode): string[] {
  if (layout.type === 'pane') {
    return layout.tabGroupId ? [layout.tabGroupId] : [];
  }
  return [...collectLeafGroupIds(layout.first), ...collectLeafGroupIds(layout.second)];
}

/** Count of leaf panes in a layout tree. */
export function countPanes(layout: LayoutNode): number {
  if (layout.type === 'pane') return 1;
  return countPanes(layout.first) + countPanes(layout.second);
}

export function findEdgePanelForGroup(
  state: WindowState,
  groupId: string
): EdgePanelPosition | null {
  for (const pos of ['left', 'right', 'bottom'] as EdgePanelPosition[]) {
    if (collectLeafGroupIds(state.edgePanels[pos].layout).includes(groupId)) {
      return pos;
    }
  }
  return null;
}

export function findEdgePanelForPane(
  state: WindowState,
  paneId: string
): EdgePanelPosition | null {
  for (const pos of ['left', 'right', 'bottom'] as EdgePanelPosition[]) {
    if (findPaneInLayout(state.edgePanels[pos].layout, paneId)) return pos;
  }
  return null;
}

/** Search every layout root (center tiling + edge panels) for a pane. */
export function findPaneAnywhere(
  state: WindowState,
  paneId: string
): PaneNode | null {
  const inMain = findPaneInLayout(state.layout, paneId);
  if (inMain) return inMain;
  for (const pos of ['left', 'right', 'bottom'] as EdgePanelPosition[]) {
    const pane = findPaneInLayout(state.edgePanels[pos].layout, paneId);
    if (pane) return pane;
  }
  return null;
}

/** The pane region a pane lives in: an edge position or the center tiling. */
export function regionOfPane(
  state: WindowState,
  paneId: string
): EdgePanelPosition | 'center' {
  return findEdgePanelForPane(state, paneId) ?? 'center';
}

/**
 * Apply a tree transform to whichever layout root (center or edge panel)
 * satisfies `contains`. Mutates the draft state; returns true when a root
 * matched. This is what makes every split/drop/collapse operation work
 * identically in the center tiling and inside edge panels.
 */
export function updateRootWhere(
  s: WindowState,
  contains: (root: LayoutNode) => boolean,
  transform: (root: LayoutNode) => LayoutNode
): boolean {
  if (contains(s.layout)) {
    s.layout = transform(s.layout);
    return true;
  }
  for (const pos of ['left', 'right', 'bottom'] as EdgePanelPosition[]) {
    if (contains(s.edgePanels[pos].layout)) {
      s.edgePanels[pos].layout = transform(s.edgePanels[pos].layout);
      return true;
    }
  }
  return false;
}

/** The group new tabs land in when a whole edge panel is the drop target:
 * its first leaf group (top/leading pane). */
export function primaryEdgeGroupId(
  state: WindowState,
  pos: EdgePanelPosition
): string | null {
  return collectLeafGroupIds(state.edgePanels[pos].layout)[0] ?? null;
}

const createSampleTabs = (): Tab[] => [];

// Only IMPLEMENTED panels ship in the default layout — no placeholder tabs.
// The Navigator absorbed the old separate Files/Sessions tabs; persisted
// layouts get remapped by migrateRetiredPanels, and this default must match
// it or a FRESH profile opens two tabs with no registered panel ("Unknown
// content type").
const createLeftPanelTabs = (): Tab[] => [
  {
    id: 'navigator-tab',
    title: 'Navigator',
    contentType: 'navigator',
    icon: FolderTree,
  },
];

const createRightPanelTabs = (): Tab[] => [
  {
    id: 'backlinks-tab',
    title: 'Backlinks',
    contentType: 'backlinks',
    icon: Link2,
  },
  {
    id: 'activity-tab',
    title: 'Activity',
    contentType: 'activity',
    icon: Activity,
  },
];

const createBottomPanelTabs = (): Tab[] => [
  {
    id: 'terminal-tab-1',
    title: 'Terminal',
    contentType: 'terminal',
    icon: Terminal,
  },
  {
    id: 'chat-tab',
    title: 'Chat',
    contentType: 'chat',
    icon: MessageCircle,
  },
];

export function createInitialState(): WindowState {
  const mainPaneId = generateId();
  const tabGroupId1 = generateId();
  const leftGroupId = generateId();
  const rightGroupId = generateId();
  const bottomGroupId = generateId();
  // Open each edge panel on its FIRST tab, derived rather than hard-coded: a
  // literal id that a tab-roster change orphans leaves the panel showing "No
  // tab selected" (it has happened for both the left and right panels).
  const leftTabs = createLeftPanelTabs();
  const rightTabs = createRightPanelTabs();
  const bottomTabs = createBottomPanelTabs();
  return {
    layout: {
      id: mainPaneId,
      type: 'pane' as const,
      tabGroupId: tabGroupId1,
    },
    tabGroups: {
      [tabGroupId1]: {
        id: tabGroupId1,
        tabs: createSampleTabs(),
        activeTabId: null,
      },
      [leftGroupId]: {
        id: leftGroupId,
        tabs: leftTabs,
        activeTabId: leftTabs[0]?.id ?? null,
      },
      [rightGroupId]: {
        id: rightGroupId,
        tabs: rightTabs,
        activeTabId: rightTabs[0]?.id ?? null,
      },
      [bottomGroupId]: {
        id: bottomGroupId,
        tabs: bottomTabs,
        activeTabId: bottomTabs[0]?.id ?? null,
      },
    },
    edgePanels: {
      left: {
        id: 'left-panel',
        layout: { id: 'left-pane', type: 'pane' as const, tabGroupId: leftGroupId },
        isCollapsed: false,
        width: 280,
      },
      right: {
        id: 'right-panel',
        layout: { id: 'right-pane', type: 'pane' as const, tabGroupId: rightGroupId },
        isCollapsed: true,
        // Sessions dock here — needs chat-worthy width, not a sidebar sliver.
        width: 520,
      },
      bottom: {
        id: 'bottom-panel',
        layout: { id: 'bottom-pane', type: 'pane' as const, tabGroupId: bottomGroupId },
        isCollapsed: true,
        height: 200,
      },
    },
    floatingWindows: [],
    activePaneId: mainPaneId,
    focusedRegion: 'center',
    nextZIndex: 100,
  };
}

export function updateSplitRatio(
  layout: LayoutNode,
  splitId: string,
  newRatio: number
): LayoutNode {
  if (layout.type === 'pane') return layout;
  if (layout.id === splitId) return { ...layout, splitRatio: newRatio };
  return {
    ...layout,
    first: updateSplitRatio(layout.first, splitId, newRatio),
    second: updateSplitRatio(layout.second, splitId, newRatio),
  };
}
