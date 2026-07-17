import { produce } from 'solid-js/store';
import type {
  LayoutNode,
  SplitDirection,
  Tab,
  TabGroup,
} from '@/types/windowTypes';
import type { PaneDropPosition } from './windowStoreTypes';
import type { WindowStoreContext } from './windowStoreInternals';
import { statusBarActions } from './statusBarStore';
import { syncShellSurface } from './shellStore';

/** Keep the status bar's "active session" in sync with tab focus so
 * session-scoped commands (Ctrl+K clear, switch-model) hit the chat the
 * user is looking at, not the one that bootstrapped last. */
function syncActiveSession(tab: Tab | undefined | null): void {
  const sessionId = tab?.metadata?.sessionId;
  if (typeof sessionId === 'string') {
    statusBarActions.setActiveSessionId(sessionId);
  }
  syncShellSurface(tab);
}
import {
  collapseEmptyNodes,
  findEdgePanelForGroup,
  findFirstPane,
  findPaneInLayout,
  generateId,
  insertPaneRelative,
  replacePaneWithSplit,
  updatePaneInLayout,
} from './windowStoreInternals';

export interface TabActions {
  addTab(groupId: string, tab: Tab, insertIndex?: number): void;
  removeTab(groupId: string, tabId: string): void;
  setActiveTab(groupId: string, tabId: string | null): void;
  moveTab(
    sourceGroupId: string,
    targetGroupId: string,
    tabId: string,
    insertIndex?: number
  ): void;
  updateTab(groupId: string, tabId: string, updates: Partial<Tab>): void;
  createTabGroup(paneId?: string): string;
  deleteTabGroup(groupId: string): void;
  splitPane(paneId: string, direction: SplitDirection): void;
  splitPaneAndDrop(
    paneId: string,
    position: PaneDropPosition,
    sourceGroupId: string,
    tabId: string
  ): void;
}

export function createTabActions(context: WindowStoreContext): TabActions {
  const { store, setStore } = context;

  const addTab = (groupId: string, tab: Tab, insertIndex?: number) => {
    const group = store.tabGroups[groupId];
    if (!group) return;
    const newTabs =
      insertIndex !== undefined
        ? [
            ...group.tabs.slice(0, insertIndex),
            tab,
            ...group.tabs.slice(insertIndex),
          ]
        : [...group.tabs, tab];
    setStore('tabGroups', groupId, { tabs: newTabs, activeTabId: tab.id });
    syncActiveSession(tab);
  };

  const removeTab = (groupId: string, tabId: string) => {
    const group = store.tabGroups[groupId];
    if (!group) return;
    const newTabs = group.tabs.filter((t) => t.id !== tabId);
    const newActiveTabId =
      group.activeTabId === tabId
        ? (newTabs.length > 0 ? newTabs[newTabs.length - 1]!.id : null)
        : group.activeTabId;

    setStore(
      produce((s) => {
        if (newTabs.length === 0) {
          const pos = findEdgePanelForGroup(store, groupId);
          const floating = s.floatingWindows.find((w) => w.tabGroupId === groupId);
          if (floating) {
            // A floating window with no tabs is a zombie — close it with its
            // group instead of leaving an empty shell.
            s.floatingWindows = s.floatingWindows.filter((w) => w.id !== floating.id);
            delete s.tabGroups[groupId];
          } else if (pos) {
            s.tabGroups[groupId] = { ...group, tabs: [], activeTabId: null };
            s.edgePanels[pos].isCollapsed = true;
          } else {
            delete s.tabGroups[groupId];
            s.layout = collapseEmptyNodes(s.layout, s.tabGroups);
            const firstPane = findFirstPane(s.layout);
            if (
              firstPane &&
              (!s.activePaneId || !findPaneInLayout(s.layout, s.activePaneId))
            ) {
              s.activePaneId = firstPane.id;
              s.focusedRegion = 'center';
            }
          }
        } else {
          s.tabGroups[groupId] = {
            ...group,
            tabs: newTabs,
            activeTabId: newActiveTabId,
          };
        }
      })
    );
  };

  const setActiveTab = (groupId: string, tabId: string | null) => {
    setStore('tabGroups', groupId, 'activeTabId', tabId);
    syncActiveSession(store.tabGroups[groupId]?.tabs.find((t) => t.id === tabId));
  };

  const moveTab = (
    sourceGroupId: string,
    targetGroupId: string,
    tabId: string,
    insertIndex?: number
  ) => {
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
    const clonedTab = { ...tab };
    const newTargetTabs =
      insertIndex !== undefined
        ? [
            ...targetGroup.tabs.slice(0, insertIndex),
            clonedTab,
            ...targetGroup.tabs.slice(insertIndex),
          ]
        : [...targetGroup.tabs, clonedTab];

    setStore(
      produce((s) => {
        if (newSourceTabs.length === 0) {
          const sourcePos = findEdgePanelForGroup(store, sourceGroupId);
          const floating = s.floatingWindows.find((w) => w.tabGroupId === sourceGroupId);
          if (floating) {
            // Last tab dragged out of a floating window: the window goes too.
            s.floatingWindows = s.floatingWindows.filter((w) => w.id !== floating.id);
            delete s.tabGroups[sourceGroupId];
          } else if (sourcePos) {
            s.tabGroups[sourceGroupId] = {
              ...sourceGroup,
              tabs: [],
              activeTabId: null,
            };
            s.edgePanels[sourcePos].isCollapsed = true;
          } else {
            delete s.tabGroups[sourceGroupId];
            s.layout = collapseEmptyNodes(s.layout, s.tabGroups);
            const firstPane = findFirstPane(s.layout);
            if (
              firstPane &&
              (!s.activePaneId || !findPaneInLayout(s.layout, s.activePaneId))
            ) {
              s.activePaneId = firstPane.id;
            }
          }
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

        const targetPos = findEdgePanelForGroup(store, targetGroupId);
        s.focusedRegion = targetPos ?? 'center';

        if (targetPos && s.edgePanels[targetPos].isCollapsed) {
          s.edgePanels[targetPos].isCollapsed = false;
        }
      })
    );
  };

  const updateTab = (groupId: string, tabId: string, updates: Partial<Tab>) => {
    const group = store.tabGroups[groupId];
    if (!group) return;
    setStore(
      'tabGroups',
      groupId,
      'tabs',
      group.tabs.map((t) => (t.id === tabId ? { ...t, ...updates } : t))
    );
  };

  const createTabGroup = (paneId?: string): string => {
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
          s.focusedRegion = 'center';
        }
      })
    );
    return groupId;
  };

  const deleteTabGroup = (groupId: string) => {
    setStore(
      produce((s) => {
        delete s.tabGroups[groupId];
      })
    );
  };

  const splitPane = (paneId: string, direction: SplitDirection) => {
    const pane = findPaneInLayout(store.layout, paneId);
    if (!pane) return;
    const firstGroupId = generateId();
    const secondGroupId = generateId();
    const originalGroup = pane.tabGroupId ? store.tabGroups[pane.tabGroupId] : null;
    const newSplit: LayoutNode = {
      id: generateId(),
      type: 'split',
      direction,
      splitRatio: 0.5,
      first: {
        id: generateId(),
        type: 'pane',
        tabGroupId: firstGroupId,
      },
      second: {
        id: generateId(),
        type: 'pane',
        tabGroupId: secondGroupId,
      },
    };
    setStore(
      produce((s) => {
        s.layout = replacePaneWithSplit(s.layout, paneId, newSplit);
        s.tabGroups[firstGroupId] = {
          id: firstGroupId,
          tabs: originalGroup ? [...originalGroup.tabs] : [],
          activeTabId: originalGroup?.activeTabId ?? null,
        };
        s.tabGroups[secondGroupId] = {
          id: secondGroupId,
          tabs: [],
          activeTabId: null,
        };
        if (pane.tabGroupId && pane.tabGroupId in s.tabGroups) {
          delete s.tabGroups[pane.tabGroupId];
        }
        s.activePaneId = (newSplit as Extract<LayoutNode, { type: 'split' }>).second.id;
        s.focusedRegion = 'center';
      })
    );
  };

  const splitPaneAndDrop = (
    paneId: string,
    position: PaneDropPosition,
    sourceGroupId: string,
    tabId: string
  ) => {
    const pane = findPaneInLayout(store.layout, paneId);
    if (!pane) {
      // A miss here means a drop target carried a pane id that's no longer
      // in the layout (historically: stale droppable after a layout restore).
      // Never swallow it silently — the drag just "does nothing" otherwise.
      console.warn(`splitPaneAndDrop: pane ${paneId} not found in layout`);
      return;
    }
    const newPaneId = generateId();
    const newGroupId = generateId();
    setStore(
      produce((s) => {
        s.layout = insertPaneRelative(s.layout, paneId, position, newPaneId, newGroupId);
        s.tabGroups[newGroupId] = {
          id: newGroupId,
          tabs: [],
          activeTabId: tabId,
        };
        s.activePaneId = newPaneId;
        s.focusedRegion = 'center';
      })
    );
    moveTab(sourceGroupId, newGroupId, tabId);
  };

  return {
    addTab,
    removeTab,
    setActiveTab,
    moveTab,
    updateTab,
    createTabGroup,
    deleteTabGroup,
    splitPane,
    splitPaneAndDrop,
  };
}
