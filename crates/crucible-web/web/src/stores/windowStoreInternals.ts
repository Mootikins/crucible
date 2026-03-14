import type { SetStoreFunction } from 'solid-js/store';
import type {
  EdgePanelPosition,
  LayoutNode,
  PaneNode,
  Tab,
  TabGroup,
} from '@/types/windowTypes';
import {
  AlertTriangle,
  ClipboardList,
  FileOutput,
  FolderTree,
  GitBranch,
  ListTree,
  MessageCircle,
  Search,
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

export function findEdgePanelForGroup(
  state: WindowState,
  groupId: string
): EdgePanelPosition | null {
  for (const pos of ['left', 'right', 'bottom'] as EdgePanelPosition[]) {
    if (state.edgePanels[pos].tabGroupId === groupId) return pos;
  }
  return null;
}

const createSampleTabs = (): Tab[] => [];

const createLeftPanelTabs = (): Tab[] => [
  {
    id: 'sessions-tab',
    title: 'Sessions',
    contentType: 'sessions',
    icon: ClipboardList,
  },
  {
    id: 'explorer-tab',
    title: 'Explorer',
    contentType: 'explorer',
    icon: FolderTree,
  },
  {
    id: 'search-tab',
    title: 'Search',
    contentType: 'search',
    icon: Search,
  },
  {
    id: 'source-control-tab',
    title: 'Source Control',
    contentType: 'source-control',
    icon: GitBranch,
  },
];

const createRightPanelTabs = (): Tab[] => [
  {
    id: 'outline-tab',
    title: 'Outline',
    contentType: 'outline',
    icon: ListTree,
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
    id: 'problems-tab',
    title: 'Problems',
    contentType: 'problems',
    icon: AlertTriangle,
  },
  {
    id: 'output-tab',
    title: 'Output',
    contentType: 'output',
    icon: FileOutput,
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
        tabs: createLeftPanelTabs(),
        activeTabId: 'sessions-tab',
      },
      [rightGroupId]: {
        id: rightGroupId,
        tabs: createRightPanelTabs(),
        activeTabId: 'outline-tab',
      },
      [bottomGroupId]: {
        id: bottomGroupId,
        tabs: createBottomPanelTabs(),
        activeTabId: 'terminal-tab-1',
      },
    },
    edgePanels: {
      left: {
        id: 'left-panel',
        tabGroupId: leftGroupId,
        isCollapsed: false,
        width: 280,
      },
      right: {
        id: 'right-panel',
        tabGroupId: rightGroupId,
        isCollapsed: true,
        width: 250,
      },
      bottom: {
        id: 'bottom-panel',
        tabGroupId: bottomGroupId,
        isCollapsed: true,
        height: 200,
      },
    },
    floatingWindows: [],
    activePaneId: mainPaneId,
    focusedRegion: 'center',
    dragState: null,
    flyoutState: null,
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
