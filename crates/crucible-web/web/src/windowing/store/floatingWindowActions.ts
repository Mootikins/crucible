import { produce } from 'solid-js/store';
import type {
  FloatingWindow,
  LayoutNode,
  PaneNode,
} from '../model/types';
import type { WindowStoreContext } from '../model/tree';
import type { WindowPolicy } from './policy';
import {
  collapseEmptyNodes,
  findEdgePanelForPane,
  findFirstPane,
  findPaneAnywhere,
  findPaneInLayout,
  generateId,
  replacePaneWithSplit,
  updatePaneInLayout,
  updateRootWhere,
} from '../model/tree';

export interface FloatingWindowActions {
  createFloatingWindow(
    tabGroupId: string,
    x: number,
    y: number,
    width?: number,
    height?: number,
    opts?: Pick<FloatingWindow, 'transient' | 'showTabBar' | 'title'>
  ): string;
  /**
   * Move a pane's tabs into a new floating window, and return its id.
   *
   * With `tabId`, move that one tab only. The pane keeps its other tabs. When
   * the tab is the only tab of the pane, the whole group moves, as without
   * `tabId`. The policy guard (`canPopOutTab`) applies to the tab.
   */
  popOutPane(paneId: string, tabId?: string): string | null;
  /**
   * True when a tab can go into a floating window. A closed floating window
   * closes its tabs without a policy check, so a tab that the policy keeps
   * stays docked. A tab that the policy calls unavailable stays docked too.
   */
  canPopOutTab(groupId: string, tabId: string): boolean;
  removeFloatingWindow(windowId: string): void;
  closeFloatingWindow(windowId: string): void;
  updateFloatingWindow(windowId: string, updates: Partial<FloatingWindow>): void;
  bringToFront(windowId: string): void;
  minimizeFloatingWindow(windowId: string): void;
  maximizeFloatingWindow(windowId: string): void;
  restoreFloatingWindow(windowId: string): void;
  /** Promote a transient (hover) window to a normal, persisted one. */
  pinFloatingWindow(windowId: string): void;
  /**
   * Move a floating window's tabs back into the centre tiling. With `tabId`,
   * move that one tab only, and the window keeps its other tabs.
   */
  dockFloatingWindow(windowId: string, tabId?: string): void;
}

export function createFloatingWindowActions<C extends string>(
  context: WindowStoreContext<C>,
  policy: () => WindowPolicy<C>,
): FloatingWindowActions {
  const { store, setStore } = context;

  const canPopOutTab = (groupId: string, tabId: string): boolean => {
    const tab = store.tabGroups[groupId]?.tabs.find((t) => t.id === tabId);
    if (!tab) return false;
    return policy().mayCloseTab(store, groupId, tabId) && policy().unavailableReason(tab) === null;
  };

  /**
   * Move one tab out of its group into a new group, and return the new id.
   *
   * The caller makes sure that the group keeps one tab or more. The write is
   * its own store write: the old row unmounts, and unregisters its solid-dnd
   * ids, before a new tab bar shows the tab (see `popOutPane`).
   */
  const splitTabOff = (groupId: string, tabId: string): string => {
    const group = store.tabGroups[groupId]!;
    const index = group.tabs.findIndex((t) => t.id === tabId);
    const tab = { ...group.tabs[index]! };
    const rest = group.tabs.filter((t) => t.id !== tabId);
    const newGroupId = generateId();
    setStore(
      produce((s) => {
        s.tabGroups[groupId] = {
          ...group,
          tabs: rest,
          // The neighbour that takes the place of the tab becomes active.
          activeTabId:
            group.activeTabId === tabId
              ? (rest[Math.min(index, rest.length - 1)]?.id ?? null)
              : group.activeTabId,
        };
        s.tabGroups[newGroupId] = { id: newGroupId, tabs: [tab], activeTabId: tab.id };
      })
    );
    return newGroupId;
  };

  const removeFloatingWindow = (windowId: string) => {
    setStore(
      'floatingWindows',
      store.floatingWindows.filter((w) => w.id !== windowId)
    );
  };

  const createFloatingWindow = (
    tabGroupId: string,
    x: number,
    y: number,
    width = 400,
    height = 300,
    opts?: Pick<FloatingWindow, 'transient' | 'showTabBar' | 'title'>
  ): string => {
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
          ...opts,
        });
        s.nextZIndex = nextZ + 1;
      })
    );
    return windowId;
  };

  // Pop a pane's tab group out into a floating window. The group MOVES: the
  // pane is detached (and collapsed out of its split) so the same group is
  // never rendered by two tab bars at once — duplicate tab strips, and
  // duplicate solid-dnd draggable/droppable ids, which corrupt the DnD
  // registry ("Cannot remove nonexistent draggable").
  const popOutPane = (paneId: string, tabId?: string): string | null => {
    const pane = findPaneAnywhere(store, paneId);
    const groupId = pane?.tabGroupId;
    if (!pane || !groupId) return null;
    const group = store.tabGroups[groupId];
    if (!group || group.tabs.length === 0) return null;

    if (tabId !== undefined) {
      const tab = group.tabs.find((t) => t.id === tabId);
      if (!tab || !canPopOutTab(groupId, tabId)) return null;
      // The tab has neighbours: it leaves alone, and the pane stays.
      if (group.tabs.length > 1) {
        return createFloatingWindow(splitTabOff(groupId, tabId), 150, 150, 500, 400, {
          title: tab.title,
        });
      }
    }

    // Region resolved BEFORE the detach — the pane may be collapsed out of
    // its tree below.
    const edgePos = findEdgePanelForPane(store, paneId);
    const activeTab = group.tabs.find((t) => t.id === group.activeTabId) ?? group.tabs[0];
    // Detach FIRST, in its own store write: the pane's tab bar must unmount
    // (and unregister its solid-dnd ids) before the floating window's tab bar
    // registers the same group's ids. Batched together, the new bar registers
    // first and the old bar's cleanup then deletes those registrations —
    // leaving the floating window undraggable/undroppable.
    setStore(
      produce((s) => {
        updateRootWhere(
          s,
          (root) => !!findPaneInLayout(root, paneId),
          (root) =>
            collapseEmptyNodes(
              updatePaneInLayout(root, paneId, (p) => ({
                ...p,
                tabGroupId: null,
              })),
              s.tabGroups
            )
        );
        if (edgePos) {
          // Popping an edge panel's sole pane out must not leave the panel
          // groupless (new tabs dock into the panel's first leaf group) —
          // give it a fresh empty group and collapse it.
          const root = s.edgePanels[edgePos].layout;
          if (root.type === 'pane' && !root.tabGroupId) {
            const emptyGroupId = generateId();
            s.tabGroups[emptyGroupId] = { id: emptyGroupId, tabs: [], activeTabId: null };
            s.edgePanels[edgePos].layout = { ...root, tabGroupId: emptyGroupId };
            s.edgePanels[edgePos].mode = 'strip';
          }
        }
        if (!s.activePaneId || !findPaneAnywhere(s, s.activePaneId)) {
          s.activePaneId = findFirstPane(s.layout)?.id ?? null;
        }
      })
    );
    const windowId = createFloatingWindow(groupId, 150, 150, 500, 400);
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) w.title = activeTab.title;
      })
    );
    return windowId;
  };

  // Closing a floating window closes its tabs with it — the group must not
  // linger invisibly in tabGroups (orphaned tabs still count in "N tabs",
  // still match a lookup by tab, and can never be reached again).
  const closeFloatingWindow = (windowId: string) => {
    const window = store.floatingWindows.find((w) => w.id === windowId);
    if (!window) return;
    setStore(
      produce((s) => {
        s.floatingWindows = s.floatingWindows.filter((w) => w.id !== windowId);
        delete s.tabGroups[window.tabGroupId];
      })
    );
  };

  const updateFloatingWindow = (
    windowId: string,
    updates: Partial<FloatingWindow>
  ) => {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) Object.assign(w, updates);
      })
    );
  };

  const bringToFront = (windowId: string) => {
    const nextZ = store.nextZIndex;
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) w.zIndex = nextZ;
        s.nextZIndex = nextZ + 1;
      })
    );
  };

  const minimizeFloatingWindow = (windowId: string) => {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) w.isMinimized = true;
      })
    );
  };

  const maximizeFloatingWindow = (windowId: string) => {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w && !w.isMaximized) {
          w.restoreBounds = { x: w.x, y: w.y, width: w.width, height: w.height };
          w.isMaximized = true;
        }
      })
    );
  };

  const restoreFloatingWindow = (windowId: string) => {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) {
          w.isMinimized = false;
          if (w.isMaximized && w.restoreBounds) {
            Object.assign(w, w.restoreBounds);
            delete w.restoreBounds;
          }
          w.isMaximized = false;
        }
      })
    );
  };

  const pinFloatingWindow = (windowId: string) => {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) w.transient = false;
      })
    );
  };

  const dockFloatingWindow = (windowId: string, tabId?: string) => {
    const window = store.floatingWindows.find((w) => w.id === windowId);
    if (!window) return;
    const tabGroup = store.tabGroups[window.tabGroupId];
    if (!tabGroup || tabGroup.tabs.length === 0) {
      removeFloatingWindow(windowId);
      return;
    }
    if (tabId !== undefined && !tabGroup.tabs.some((t) => t.id === tabId)) return;

    const findEmptyPane = (node: LayoutNode): PaneNode | null => {
      if (node.type === 'pane') {
        const g = store.tabGroups[node.tabGroupId ?? ''];
        if (!node.tabGroupId || !g?.tabs.length) return node;
        return null;
      }
      return findEmptyPane(node.first) || findEmptyPane(node.second);
    };

    // The target comes first, so that a dock with no target changes nothing.
    // A split root is no pane, so the first leaf of the tiling takes the new
    // split. The old lookup found no pane there, and the dock did nothing.
    const firstEmpty = findEmptyPane(store.layout);
    const mainPane = firstEmpty ? null : findFirstPane(store.layout);
    if (!firstEmpty && !mainPane) return;

    // One tab with neighbours leaves the window alone; the window stays.
    // Otherwise the whole group leaves, and the window goes.
    const single = tabId !== undefined && tabGroup.tabs.length > 1;
    const groupId = single ? splitTabOff(window.tabGroupId, tabId) : window.tabGroupId;
    if (!single) removeFloatingWindow(windowId);

    if (firstEmpty) {
      setStore(
        produce((s) => {
          s.layout = updatePaneInLayout(s.layout, firstEmpty.id, () => ({
            ...firstEmpty,
            tabGroupId: groupId,
          }));
          s.activePaneId = firstEmpty.id;
          s.focusedRegion = 'center';
        })
      );
      return;
    }

    const newPaneId = generateId();
    const newSplit: LayoutNode = {
      id: generateId(),
      type: 'split',
      direction: 'horizontal',
      splitRatio: 0.5,
      first: mainPane!,
      second: {
        id: newPaneId,
        type: 'pane',
        tabGroupId: groupId,
      },
    };
    setStore(
      produce((s) => {
        s.layout =
          s.layout.type === 'pane'
            ? newSplit
            : replacePaneWithSplit(s.layout, mainPane!.id, newSplit);
        s.activePaneId = newPaneId;
        s.focusedRegion = 'center';
      })
    );
  };

  return {
    createFloatingWindow,
    popOutPane,
    canPopOutTab,
    removeFloatingWindow,
    closeFloatingWindow,
    updateFloatingWindow,
    bringToFront,
    minimizeFloatingWindow,
    maximizeFloatingWindow,
    restoreFloatingWindow,
    pinFloatingWindow,
    dockFloatingWindow,
  };
}
