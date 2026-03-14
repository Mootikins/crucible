import { produce } from 'solid-js/store';
import type {
  FloatingWindow,
  LayoutNode,
  PaneNode,
} from '@/types/windowTypes';
import type { WindowStoreContext } from './windowStoreInternals';
import {
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
  removeFloatingWindow(windowId: string): void;
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
        setStore(
          produce((s) => {
            s.layout = updatePaneInLayout(s.layout, targetPaneId, () => ({
              ...pane,
              tabGroupId: window.tabGroupId,
            }));
            s.floatingWindows = s.floatingWindows.filter((w) => w.id !== windowId);
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
      setStore(
        produce((s) => {
          s.layout = updatePaneInLayout(s.layout, firstEmpty.id, () => ({
            ...firstEmpty,
            tabGroupId: window.tabGroupId,
          }));
          s.floatingWindows = s.floatingWindows.filter((w) => w.id !== windowId);
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
      setStore(
        produce((s) => {
          s.layout =
            s.layout.type === 'pane'
              ? newSplit
              : replacePaneWithSplit(s.layout, mainPane.id, newSplit);
          s.floatingWindows = s.floatingWindows.filter((w) => w.id !== windowId);
          s.activePaneId = newPaneId;
          s.focusedRegion = 'center';
        })
      );
    }
  };

  return {
    createFloatingWindow,
    removeFloatingWindow,
    updateFloatingWindow,
    bringToFront,
    minimizeFloatingWindow,
    maximizeFloatingWindow,
    restoreFloatingWindow,
    dockFloatingWindow,
  };
}
