import { produce } from 'solid-js/store';
import type {
  EdgePanelPosition,
  LayoutNode,
  PaneDropPosition,
  SplitDirection,
  Tab,
  TabGroup,
  WindowState,
} from '../model/types';
import { isEdgeCollapsed } from '../model/types';
import type { WindowStoreContext } from '../model/tree';
import {
  collapseEmptyNodes,
  countPanes,
  findEdgePanelForGroup,
  findEdgePanelForPane,
  findFirstPane,
  findPaneAnywhere,
  findPaneInLayout,
  generateId,
  insertPaneRelative,
  replacePaneWithSplit,
  updatePaneInLayout,
  updateRootWhere,
} from '../model/tree';
import type { WindowPolicy } from './policy';

/** Drop an emptied group that lives in an edge panel: a multi-pane panel
 * collapses the empty pane out of its tree; the sole remaining pane keeps an
 * empty group and collapses the panel instead (mirrors the old single-group
 * behavior). Call inside produce(). */
function releaseEdgeGroup<C extends string>(
  s: WindowState<C>,
  pos: EdgePanelPosition,
  group: TabGroup<C>
): void {
  const panel = s.edgePanels[pos];
  if (countPanes(panel.layout) > 1) {
    delete s.tabGroups[group.id];
    panel.layout = collapseEmptyNodes(panel.layout, s.tabGroups);
    // The collapsed pane may have been the active one — apply the same
    // guard the center branches use, else every activePaneId-driven
    // shortcut (Ctrl+W, split, next-tab) silently no-ops until the user
    // clicks another pane.
    if (s.activePaneId && !findPaneAnywhere(s, s.activePaneId)) {
      s.activePaneId =
        findFirstPane(panel.layout)?.id ?? findFirstPane(s.layout)?.id ?? null;
    }
  } else {
    s.tabGroups[group.id] = { ...group, tabs: [], activeTabId: null };
    panel.mode = 'strip';
  }
}

export interface TabActions<C extends string = string> {
  addTab(groupId: string, tab: Tab<C>, insertIndex?: number): void;
  removeTab(groupId: string, tabId: string): void;
  /** True when the policy lets this tab close. */
  canCloseTab(groupId: string, tabId: string): boolean;
  setActiveTab(groupId: string, tabId: string | null): void;
  moveTab(
    sourceGroupId: string,
    targetGroupId: string,
    tabId: string,
    insertIndex?: number
  ): void;
  updateTab(groupId: string, tabId: string, updates: Partial<Tab<C>>): void;
  createTabGroup(paneId?: string): string;
  splitPane(paneId: string, direction: SplitDirection): void;
  /** Open a new tab in a new pane beside `paneId`; returns the new group id. */
  openTabInNewPane(paneId: string, position: PaneDropPosition, tab: Tab<C>): string | null;
  splitPaneAndDrop(
    paneId: string,
    position: PaneDropPosition,
    sourceGroupId: string,
    tabId: string
  ): void;
}

export function createTabActions<C extends string>(
  context: WindowStoreContext<C>,
  policy: () => WindowPolicy<C>,
): TabActions<C> {
  const { store, setStore } = context;

  const addTab = (groupId: string, tab: Tab<C>, insertIndex?: number) => {
    const group = store.tabGroups[groupId];
    if (!group) {
      // Self-heal a ghost reference: a layout pane can point at a tabGroupId
      // whose group object was lost from persisted state (pre-v3 layouts).
      // The pane node is the authority for the id, so materialize the group
      // instead of silently dropping the tab (which made every center open —
      // from a click, a command or a drop was a no-op on such layouts).
      setStore('tabGroups', groupId, { id: groupId, tabs: [tab], activeTabId: tab.id });
      policy().onActiveTabChange(tab);
      return;
    }
    const newTabs =
      insertIndex !== undefined
        ? [
            ...group.tabs.slice(0, insertIndex),
            tab,
            ...group.tabs.slice(insertIndex),
          ]
        : [...group.tabs, tab];
    setStore('tabGroups', groupId, { tabs: newTabs, activeTabId: tab.id });
    policy().onActiveTabChange(tab);
  };

  const removeTab = (groupId: string, tabId: string) => {
    const group = store.tabGroups[groupId];
    if (!group) return;
    // The policy may keep a tab. The guard is here rather than in the tab
    // strip because every close path reaches this one function — the tab's
    // own button, Close Others, Close to the Right, and a pane closing with
    // its tabs — and a rule enforced in one of them is not a rule.
    if (!policy().mayCloseTab(store, groupId, tabId)) return;
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
            releaseEdgeGroup(s, pos, group);
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

  const canCloseTab = (groupId: string, tabId: string): boolean =>
    policy().mayCloseTab(store, groupId, tabId);

  const setActiveTab = (groupId: string, tabId: string | null) => {
    setStore('tabGroups', groupId, 'activeTabId', tabId);
    // Activating a tab focuses its region — otherwise an edge panel's active
    // tab never gets the focused (ember) treatment, since nothing else on the
    // click path sets focusedRegion for edges.
    setStore('focusedRegion', findEdgePanelForGroup(store, groupId) ?? 'center');
    policy().onActiveTabChange(store.tabGroups[groupId]?.tabs.find((t) => t.id === tabId));
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
            releaseEdgeGroup(s, sourcePos, sourceGroup);
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

        if (targetPos && isEdgeCollapsed(s.edgePanels[targetPos])) {
          s.edgePanels[targetPos].mode = 'docked';
        }
      })
    );
  };

  const updateTab = (groupId: string, tabId: string, updates: Partial<Tab<C>>) => {
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
    const newGroup: TabGroup<C> = {
      id: groupId,
      tabs: [],
      activeTabId: null,
    };
    setStore(
      produce((s) => {
        s.tabGroups[groupId] = newGroup;
        if (paneId) {
          updateRootWhere(
            s,
            (root) => !!findPaneInLayout(root, paneId),
            (root) =>
              updatePaneInLayout(root, paneId, (p) => ({
                ...p,
                tabGroupId: groupId,
              }))
          );
          s.activePaneId = paneId;
          s.focusedRegion = findEdgePanelForPane(s, paneId) ?? 'center';
        }
      })
    );
    return groupId;
  };

  const splitPane = (paneId: string, direction: SplitDirection) => {
    const pane = findPaneAnywhere(store, paneId);
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
        updateRootWhere(
          s,
          (root) => !!findPaneInLayout(root, paneId),
          (root) => replacePaneWithSplit(root, paneId, newSplit)
        );
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
        const newPaneId = (newSplit as Extract<LayoutNode, { type: 'split' }>).second.id;
        s.activePaneId = newPaneId;
        s.focusedRegion = findEdgePanelForPane(s, newPaneId) ?? 'center';
      })
    );
  };

  /**
   * Open a NEW tab in a NEW pane beside an existing one.
   *
   * `splitPaneAndDrop` is the drag path — it MOVES a tab that already exists.
   * This is the programmatic path: a new tab that opens beside another tab
   * has no source group to move from, and routing it through a temporary group only
   * to move it out again would fire two layout writes for one gesture.
   *
   * Returns the new group's id so the caller can act on it.
   */
  const openTabInNewPane = (
    paneId: string,
    position: PaneDropPosition,
    tab: Tab<C>,
  ): string | null => {
    if (!findPaneAnywhere(store, paneId)) {
      console.warn(`openTabInNewPane: pane ${paneId} not found in layout`);
      return null;
    }
    const newPaneId = generateId();
    const newGroupId = generateId();
    setStore(
      produce((s) => {
        updateRootWhere(
          s,
          (root) => !!findPaneInLayout(root, paneId),
          (root) => insertPaneRelative(root, paneId, position, newPaneId, newGroupId)
        );
        s.tabGroups[newGroupId] = {
          id: newGroupId,
          tabs: [tab],
          activeTabId: tab.id,
        };
        s.activePaneId = newPaneId;
        s.focusedRegion = findEdgePanelForPane(s, newPaneId) ?? 'center';
      })
    );
    return newGroupId;
  };

  const splitPaneAndDrop = (
    paneId: string,
    position: PaneDropPosition,
    sourceGroupId: string,
    tabId: string
  ) => {
    const pane = findPaneAnywhere(store, paneId);
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
        updateRootWhere(
          s,
          (root) => !!findPaneInLayout(root, paneId),
          (root) => insertPaneRelative(root, paneId, position, newPaneId, newGroupId)
        );
        s.tabGroups[newGroupId] = {
          id: newGroupId,
          tabs: [],
          activeTabId: tabId,
        };
        s.activePaneId = newPaneId;
        s.focusedRegion = findEdgePanelForPane(s, newPaneId) ?? 'center';
      })
    );
    moveTab(sourceGroupId, newGroupId, tabId);
  };

  return {
    addTab,
    removeTab,
    canCloseTab,
    setActiveTab,
    moveTab,
    updateTab,
    createTabGroup,
    splitPane,
    openTabInNewPane,
    splitPaneAndDrop,
  };
}
