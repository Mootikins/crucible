import { createStore, produce } from 'solid-js/store';
import type {
  LayoutNode,
  PaneNode,
  Tab,
  TabGroup,
  EdgePanelTab,
  EdgePanelPosition,
  FloatingWindow,
  SplitDirection,
  DragSource,
  DropTarget,
} from '@/types/windowTypes';

const generateId = () => Math.random().toString(36).substring(2, 11);

function findPaneInLayout(
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

function updatePaneInLayout(
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

function replacePaneWithSplit(
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

export type PaneDropPosition = 'left' | 'right' | 'top' | 'bottom';

function insertPaneRelative(
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

// Sample tabs without icons (icons can be set in UI)
const createSampleTabs = (): Tab[] => [
  { id: 'tab-1', title: 'index.tsx', contentType: 'file', isModified: false },
  { id: 'tab-2', title: 'App.tsx', contentType: 'file', isModified: true },
  { id: 'tab-3', title: 'styles.css', contentType: 'file', isModified: false },
  { id: 'tab-4', title: 'package.json', contentType: 'file', isModified: false },
];

const createSampleTabs2 = (): Tab[] => [
  { id: 'tab-5', title: 'README.md', contentType: 'document', isModified: false },
  { id: 'tab-6', title: 'preview.png', contentType: 'preview', isModified: false },
];

const createLeftPanelTabs = (): EdgePanelTab[] => [
  { id: 'explorer-tab', title: 'Explorer', contentType: 'tool', panelPosition: 'left' },
  { id: 'search-tab', title: 'Search', contentType: 'tool', panelPosition: 'left' },
  { id: 'git-tab', title: 'Source Control', contentType: 'tool', panelPosition: 'left' },
];

const createRightPanelTabs = (): EdgePanelTab[] => [
  { id: 'outline-tab', title: 'Outline', contentType: 'tool', panelPosition: 'right' },
  { id: 'debug-tab', title: 'Debug', contentType: 'tool', panelPosition: 'right' },
];

const createBottomPanelTabs = (): EdgePanelTab[] => [
  { id: 'terminal-tab-1', title: 'Terminal', contentType: 'terminal', panelPosition: 'bottom' },
  { id: 'terminal-tab-2', title: 'Terminal 2', contentType: 'terminal', panelPosition: 'bottom' },
  { id: 'problems-tab', title: 'Problems', contentType: 'tool', panelPosition: 'bottom' },
  { id: 'output-tab', title: 'Output', contentType: 'tool', panelPosition: 'bottom' },
];

function createInitialState() {
  const mainPaneId = generateId();
  const tabGroupId1 = generateId();
  const tabGroupId2 = generateId();
  return {
    layout: {
      id: 'split-root',
      type: 'split' as const,
      direction: 'horizontal' as SplitDirection,
      splitRatio: 0.5,
      first: {
        id: mainPaneId,
        type: 'pane' as const,
        tabGroupId: tabGroupId1,
      },
      second: {
        id: generateId(),
        type: 'pane' as const,
        tabGroupId: tabGroupId2,
      },
    },
    tabGroups: {
      [tabGroupId1]: {
        id: tabGroupId1,
        tabs: createSampleTabs(),
        activeTabId: 'tab-1',
      },
      [tabGroupId2]: {
        id: tabGroupId2,
        tabs: createSampleTabs2(),
        activeTabId: 'tab-5',
      },
    },
    edgePanels: {
      left: {
        id: 'left-panel',
        position: 'left' as EdgePanelPosition,
        tabs: createLeftPanelTabs(),
        activeTabId: 'explorer-tab',
        isCollapsed: false,
        width: 250,
      },
      right: {
        id: 'right-panel',
        position: 'right' as EdgePanelPosition,
        tabs: createRightPanelTabs(),
        activeTabId: 'outline-tab',
        isCollapsed: true,
        width: 250,
      },
      bottom: {
        id: 'bottom-panel',
        position: 'bottom' as EdgePanelPosition,
        tabs: createBottomPanelTabs(),
        activeTabId: 'terminal-tab-1',
        isCollapsed: false,
        height: 200,
      },
    },
    floatingWindows: [] as FloatingWindow[],
    activePaneId: mainPaneId,
    dragState: null as {
      isDragging: boolean;
      source: DragSource | null;
      target: DropTarget | null;
    } | null,
    flyoutState: null as {
      isOpen: boolean;
      panelPosition: EdgePanelPosition;
      tabId: string | null;
    } | null,
    nextZIndex: 100,
  };
}

export type WindowState = ReturnType<typeof createInitialState>;

const [store, setStore] = createStore<WindowState>(createInitialState());

export { store as windowStore, setStore };

export const windowActions = {
  addTab(groupId: string, tab: Tab, insertIndex?: number) {
    const group = store.tabGroups[groupId];
    if (!group) return;
    const newTabs =
      insertIndex !== undefined
        ? [...group.tabs.slice(0, insertIndex), tab, ...group.tabs.slice(insertIndex)]
        : [...group.tabs, tab];
    setStore('tabGroups', groupId, { tabs: newTabs, activeTabId: tab.id });
  },

  removeTab(groupId: string, tabId: string) {
    const group = store.tabGroups[groupId];
    if (!group) return;
    const newTabs = group.tabs.filter((t) => t.id !== tabId);
    const newActiveTabId =
      group.activeTabId === tabId
        ? (newTabs.length > 0 ? newTabs[newTabs.length - 1]!.id : null)
        : group.activeTabId;
    if (newTabs.length === 0) {
      setStore(
        produce((s) => {
          delete s.tabGroups[groupId];
        })
      );
      return;
    }
    setStore('tabGroups', groupId, { tabs: newTabs, activeTabId: newActiveTabId });
  },

  setActiveTab(groupId: string, tabId: string | null) {
    setStore('tabGroups', groupId, 'activeTabId', tabId);
  },

  moveTab(
    sourceGroupId: string,
    targetGroupId: string,
    tabId: string,
    insertIndex?: number
  ) {
    const sourceGroup = store.tabGroups[sourceGroupId];
    const targetGroup = store.tabGroups[targetGroupId];
    if (!sourceGroup) return;
    const tab = sourceGroup.tabs.find((t) => t.id === tabId);
    if (!tab) return;
    const newSourceTabs = sourceGroup.tabs.filter((t) => t.id !== tabId);
    const newSourceActiveId =
      sourceGroup.activeTabId === tabId
        ? (newSourceTabs.length > 0 ? newSourceTabs[0]!.id : null)
        : sourceGroup.activeTabId;

    if (sourceGroupId === targetGroupId) {
      const newTabs = [...newSourceTabs];
      newTabs.splice(insertIndex ?? newTabs.length, 0, tab);
      setStore('tabGroups', sourceGroupId, {
        tabs: newTabs,
        activeTabId: tabId,
      });
      return;
    }

    if (!targetGroup) return;
    const newTargetTabs =
      insertIndex !== undefined
        ? [
            ...targetGroup.tabs.slice(0, insertIndex),
            tab,
            ...targetGroup.tabs.slice(insertIndex),
          ]
        : [...targetGroup.tabs, tab];

    setStore(
      produce((s) => {
        if (newSourceTabs.length === 0) {
          delete s.tabGroups[sourceGroupId];
        } else {
          s.tabGroups[sourceGroupId] = {
            ...sourceGroup,
            tabs: newSourceTabs,
            activeTabId: newSourceActiveId,
          };
        }
        s.tabGroups[targetGroupId] = {
          ...targetGroup,
          tabs: newTargetTabs,
          activeTabId: tabId,
        };
      })
    );
  },

  updateTab(groupId: string, tabId: string, updates: Partial<Tab>) {
    const group = store.tabGroups[groupId];
    if (!group) return;
    setStore(
      'tabGroups',
      groupId,
      'tabs',
      group.tabs.map((t) => (t.id === tabId ? { ...t, ...updates } : t))
    );
  },

  createTabGroup(paneId?: string): string {
    const groupId = generateId();
    const newGroup: TabGroup = {
      id: groupId,
      tabs: [],
      activeTabId: null,
    };
    setStore(
      produce((s) => {
        s.tabGroups[groupId] = newGroup;
        if (paneId) {
          s.layout = updatePaneInLayout(s.layout, paneId, (p) => ({
            ...p,
            tabGroupId: groupId,
          }));
          s.activePaneId = paneId;
        }
      })
    );
    return groupId;
  },

  deleteTabGroup(groupId: string) {
    setStore(
      produce((s) => {
        delete s.tabGroups[groupId];
      })
    );
  },

  splitPane(paneId: string, direction: SplitDirection) {
    const pane = findPaneInLayout(store.layout, paneId);
    if (!pane) return;
    const newGroupId = generateId();
    const newSplit: LayoutNode = {
      id: generateId(),
      type: 'split',
      direction,
      splitRatio: 0.5,
      first: {
        id: generateId(),
        type: 'pane',
        tabGroupId: pane.tabGroupId,
      },
      second: {
        id: generateId(),
        type: 'pane',
        tabGroupId: newGroupId,
      },
    };
    setStore(
      produce((s) => {
        s.layout = replacePaneWithSplit(s.layout, paneId, newSplit);
        s.tabGroups[newGroupId] = {
          id: newGroupId,
          tabs: [],
          activeTabId: null,
        };
        s.activePaneId = (newSplit as Extract<LayoutNode, { type: 'split' }>).second.id;
      })
    );
  },

  splitPaneAndDrop(
    paneId: string,
    position: PaneDropPosition,
    sourceGroupId: string,
    tabId: string
  ) {
    const pane = findPaneInLayout(store.layout, paneId);
    if (!pane) return;
    const newPaneId = generateId();
    const newGroupId = generateId();
    setStore(
      produce((s) => {
        s.layout = insertPaneRelative(
          s.layout,
          paneId,
          position,
          newPaneId,
          newGroupId
        );
        s.tabGroups[newGroupId] = {
          id: newGroupId,
          tabs: [],
          activeTabId: tabId,
        };
        s.activePaneId = newPaneId;
      })
    );
    windowActions.moveTab(sourceGroupId, newGroupId, tabId);
  },

  setActivePane(paneId: string | null) {
    setStore('activePaneId', paneId);
  },

  toggleEdgePanel(position: EdgePanelPosition) {
    setStore(
      produce((s) => {
        s.edgePanels[position].isCollapsed =
          !s.edgePanels[position].isCollapsed;
        s.flyoutState = null;
      })
    );
  },

  setEdgePanelCollapsed(position: EdgePanelPosition, collapsed: boolean) {
    setStore('edgePanels', position, 'isCollapsed', collapsed);
  },

  setEdgePanelActiveTab(position: EdgePanelPosition, tabId: string | null) {
    setStore('edgePanels', position, 'activeTabId', tabId);
  },

  setEdgePanelSize(position: EdgePanelPosition, size: number) {
    const isVertical = position === 'left' || position === 'right';
    const clamped = isVertical
      ? Math.max(120, Math.min(600, size))
      : Math.max(100, Math.min(500, size));
    setStore(
      produce((s) => {
        if (isVertical) {
          s.edgePanels[position].width = clamped;
        } else {
          s.edgePanels[position].height = clamped;
        }
      })
    );
  },

  addEdgePanelTab(position: EdgePanelPosition, tab: EdgePanelTab) {
    setStore(
      produce((s) => {
        s.edgePanels[position].tabs.push(tab);
        s.edgePanels[position].activeTabId = tab.id;
      })
    );
  },

  removeEdgePanelTab(position: EdgePanelPosition, tabId: string) {
    setStore(
      produce((s) => {
        const panel = s.edgePanels[position];
        panel.tabs = panel.tabs.filter((t) => t.id !== tabId);
        panel.activeTabId =
          panel.activeTabId === tabId
            ? (panel.tabs[0]?.id ?? null)
            : panel.activeTabId;
      })
    );
  },

  openFlyout(position: EdgePanelPosition, tabId: string) {
    setStore({
      flyoutState: { isOpen: true, panelPosition: position, tabId },
    });
  },

  closeFlyout() {
    setStore('flyoutState', null);
  },

  createFloatingWindow(
    tabGroupId: string,
    x: number,
    y: number,
    width = 400,
    height = 300
  ): string {
    const windowId = generateId();
    const nextZ = store.nextZIndex;
    setStore(
      produce((s) => {
        s.floatingWindows.push({
          id: windowId,
          tabGroupId,
          x,
          y,
          width,
          height,
          isMinimized: false,
          isMaximized: false,
          zIndex: nextZ,
        });
        s.nextZIndex = nextZ + 1;
      })
    );
    return windowId;
  },

  removeFloatingWindow(windowId: string) {
    setStore(
      'floatingWindows',
      store.floatingWindows.filter((w) => w.id !== windowId)
    );
  },

  updateFloatingWindow(windowId: string, updates: Partial<FloatingWindow>) {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) Object.assign(w, updates);
      })
    );
  },

  bringToFront(windowId: string) {
    const nextZ = store.nextZIndex;
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) w.zIndex = nextZ;
        s.nextZIndex = nextZ + 1;
      })
    );
  },

  minimizeFloatingWindow(windowId: string) {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) w.isMinimized = true;
      })
    );
  },

  maximizeFloatingWindow(windowId: string) {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) w.isMaximized = true;
      })
    );
  },

  restoreFloatingWindow(windowId: string) {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) {
          w.isMinimized = false;
          w.isMaximized = false;
        }
      })
    );
  },

  dockFloatingWindow(windowId: string, targetPaneId?: string) {
    const window = store.floatingWindows.find((w) => w.id === windowId);
    if (!window) return;
    const tabGroup = store.tabGroups[window.tabGroupId];
    if (!tabGroup || tabGroup.tabs.length === 0) {
      windowActions.removeFloatingWindow(windowId);
      return;
    }
    if (targetPaneId) {
      const pane = findPaneInLayout(store.layout, targetPaneId);
      if (pane) {
        setStore(
          produce((s) => {
            s.layout = updatePaneInLayout(s.layout, targetPaneId, () => ({
              ...pane,
              tabGroupId: window.tabGroupId,
            }));
            s.floatingWindows = s.floatingWindows.filter((w) => w.id !== windowId);
            s.activePaneId = targetPaneId;
          })
        );
        return;
      }
    }
    const findEmptyPane = (node: LayoutNode): PaneNode | null => {
      if (node.type === 'pane') {
        const g = store.tabGroups[node.tabGroupId ?? ''];
        if (!node.tabGroupId || !g?.tabs.length) return node;
        return null;
      }
      return findEmptyPane(node.first) || findEmptyPane(node.second);
    };
    const firstEmpty = findEmptyPane(store.layout);
    if (firstEmpty) {
      setStore(
        produce((s) => {
          s.layout = updatePaneInLayout(s.layout, firstEmpty.id, () => ({
            ...firstEmpty,
            tabGroupId: window.tabGroupId,
          }));
          s.floatingWindows = s.floatingWindows.filter((w) => w.id !== windowId);
          s.activePaneId = firstEmpty.id;
        })
      );
      return;
    }
    const mainPane =
      store.layout.type === 'pane'
        ? store.layout
        : findPaneInLayout(store.layout, store.layout.id);
    if (mainPane && mainPane.type === 'pane') {
      const newPaneId = generateId();
      const newSplit: LayoutNode = {
        id: generateId(),
        type: 'split',
        direction: 'horizontal',
        splitRatio: 0.5,
        first: mainPane,
        second: {
          id: newPaneId,
          type: 'pane',
          tabGroupId: window.tabGroupId,
        },
      };
      setStore(
        produce((s) => {
          s.layout =
            s.layout.type === 'pane'
              ? newSplit
              : replacePaneWithSplit(s.layout, mainPane.id, newSplit);
          s.floatingWindows = s.floatingWindows.filter((w) => w.id !== windowId);
          s.activePaneId = newPaneId;
        })
      );
    }
  },

  startDrag(source: DragSource) {
    setStore({
      dragState: { isDragging: true, source, target: null },
    });
  },

  setDropTarget(target: DropTarget | null) {
    if (!store.dragState) return;
    setStore('dragState', 'target', target);
  },

  endDrag() {
    setStore('dragState', null);
  },

  executeDrop() {
    const state = store;
    if (!state.dragState?.source || !state.dragState?.target) return;
    const { source, target } = state.dragState;
    if (source.type === 'tab') {
      if (target.type === 'pane') {
        const pane = findPaneInLayout(state.layout, target.paneId);
        if (pane) {
          const existingId = pane.tabGroupId;
          if (existingId) {
            windowActions.moveTab(source.sourceGroupId, existingId, source.tab.id);
          } else {
            const newGroupId = windowActions.createTabGroup(pane.id);
            windowActions.moveTab(source.sourceGroupId, newGroupId, source.tab.id);
          }
        }
      } else if (target.type === 'tabGroup') {
        windowActions.moveTab(
          source.sourceGroupId,
          target.groupId,
          source.tab.id,
          target.insertIndex
        );
      } else if (target.type === 'edgePanel') {
        const panel = state.edgePanels[target.panelId as EdgePanelPosition];
        if (panel) {
          windowActions.removeTab(source.sourceGroupId, source.tab.id);
          windowActions.addEdgePanelTab(panel.position, {
            ...source.tab,
            panelPosition: panel.position,
          });
        }
      } else if (target.type === 'newFloating') {
        const newGroupId = windowActions.createTabGroup();
        windowActions.moveTab(source.sourceGroupId, newGroupId, source.tab.id);
        windowActions.createFloatingWindow(newGroupId, 100, 100, 400, 300);
      }
    }
    windowActions.endDrag();
  },

  getTabGroup(groupId: string) {
    return store.tabGroups[groupId];
  },

  getPaneTabGroupId(paneId: string): string | null {
    const pane = findPaneInLayout(store.layout, paneId);
    return pane?.tabGroupId ?? null;
  },

  findPaneById(paneId: string) {
    return findPaneInLayout(store.layout, paneId);
  },
};

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
