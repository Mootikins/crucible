import { produce } from 'solid-js/store';
import type {
  FloatingWindow,
  LayoutNode,
  PaneNode,
} from '@/types/windowTypes';
import type { WindowStoreContext } from './windowStoreInternals';
import {
  collapseEmptyNodes,
  findFirstPane,
  findPaneInLayout,
  generateId,
  replacePaneWithSplit,
  updatePaneInLayout,
} from './windowStoreInternals';

export interface FloatingWindowActions {
  createFloatingWindow(
    tabGroupId: string,
    x: number,
    y: number,
    width?: number,
    height?: number
  ): string;
  popOutPane(paneId: string): string | null;
  removeFloatingWindow(windowId: string): void;
  closeFloatingWindow(windowId: string): void;
  updateFloatingWindow(windowId: string, updates: Partial<FloatingWindow>): void;
  bringToFront(windowId: string): void;
  minimizeFloatingWindow(windowId: string): void;
  maximizeFloatingWindow(windowId: string): void;
  restoreFloatingWindow(windowId: string): void;
  dockFloatingWindow(windowId: string, targetPaneId?: string): void;
}

export function createFloatingWindowActions(
  context: WindowStoreContext
): FloatingWindowActions {
  const { store, setStore } = context;

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
    height = 300
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
  const popOutPane = (paneId: string): string | null => {
    const pane = findPaneInLayout(store.layout, paneId);
    const groupId = pane?.tabGroupId;
    if (!pane || !groupId) return null;
    const group = store.tabGroups[groupId];
    if (!group || group.tabs.length === 0) return null;

    const activeTab = group.tabs.find((t) => t.id === group.activeTabId) ?? group.tabs[0];
    // Detach FIRST, in its own store write: the pane's tab bar must unmount
    // (and unregister its solid-dnd ids) before the floating window's tab bar
    // registers the same group's ids. Batched together, the new bar registers
    // first and the old bar's cleanup then deletes those registrations —
    // leaving the floating window undraggable/undroppable.
    setStore(
      produce((s) => {
        s.layout = updatePaneInLayout(s.layout, paneId, (p) => ({
          ...p,
          tabGroupId: null,
        }));
        s.layout = collapseEmptyNodes(s.layout, s.tabGroups);
        if (!s.activePaneId || !findPaneInLayout(s.layout, s.activePaneId)) {
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
  // still match findTabByFilePath, and can never be reached again).
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
        if (w) w.isMaximized = true;
      })
    );
  };

  const restoreFloatingWindow = (windowId: string) => {
    setStore(
      produce((s) => {
        const w = s.floatingWindows.find((x) => x.id === windowId);
        if (w) {
          w.isMinimized = false;
          w.isMaximized = false;
        }
      })
    );
  };

  const dockFloatingWindow = (windowId: string, targetPaneId?: string) => {
    const window = store.floatingWindows.find((w) => w.id === windowId);
    if (!window) return;
    const tabGroup = store.tabGroups[window.tabGroupId];
    if (!tabGroup || tabGroup.tabs.length === 0) {
      removeFloatingWindow(windowId);
      return;
    }

    if (targetPaneId) {
      const pane = findPaneInLayout(store.layout, targetPaneId);
      if (pane) {
        // Window unmounts first so its tab bar unregisters before the pane's
        // tab bar re-registers the same group ids (see popOutPane).
        removeFloatingWindow(windowId);
        setStore(
          produce((s) => {
            s.layout = updatePaneInLayout(s.layout, targetPaneId, () => ({
              ...pane,
              tabGroupId: window.tabGroupId,
            }));
            s.activePaneId = targetPaneId;
            s.focusedRegion = 'center';
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
      removeFloatingWindow(windowId);
      setStore(
        produce((s) => {
          s.layout = updatePaneInLayout(s.layout, firstEmpty.id, () => ({
            ...firstEmpty,
            tabGroupId: window.tabGroupId,
          }));
          s.activePaneId = firstEmpty.id;
          s.focusedRegion = 'center';
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
      removeFloatingWindow(windowId);
      setStore(
        produce((s) => {
          s.layout =
            s.layout.type === 'pane'
              ? newSplit
              : replacePaneWithSplit(s.layout, mainPane.id, newSplit);
          s.activePaneId = newPaneId;
          s.focusedRegion = 'center';
        })
      );
    }
  };

  return {
    createFloatingWindow,
    popOutPane,
    removeFloatingWindow,
    closeFloatingWindow,
    updateFloatingWindow,
    bringToFront,
    minimizeFloatingWindow,
    maximizeFloatingWindow,
    restoreFloatingWindow,
    dockFloatingWindow,
  };
}
