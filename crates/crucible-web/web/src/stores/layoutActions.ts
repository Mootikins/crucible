import { produce } from 'solid-js/store';
import type {
  EdgePanel as EdgePanelType,
  EdgePanelPosition,
  LayoutNode,
  TabGroup,
} from '@/types/windowTypes';
import type { SerializedLayout } from '@/lib/layout-serializer';
import {
  deserializeLayout,
  serializeLayout,
} from '@/lib/layout-serializer';
import { markLayoutRestore } from '@/lib/layout-restore';
import type { WindowStoreContext } from './windowStoreInternals';
import {
  collectLeafGroupIds,
  findFirstPane,
  findPaneAnywhere,
  regionOfPane,
  updateRootWhere,
  updateSplitRatio,
} from './windowStoreInternals';
import { statusBarActions } from './statusBarStore';
import { syncShellSurface } from './shellStore';

export interface LayoutActions {
  setActivePane(paneId: string | null): void;
  toggleEdgePanel(position: EdgePanelPosition): void;
  swapSidePanels(): void;
  setEdgePanelCollapsed(position: EdgePanelPosition, collapsed: boolean): void;
  setEdgePanelActiveTab(position: EdgePanelPosition, tabId: string | null): void;
  setEdgePanelSize(position: EdgePanelPosition, size: number): void;
  getTabGroup(groupId: string): TabGroup | undefined;
  getPaneTabGroupId(paneId: string): string | null;
  findPaneById(paneId: string): ReturnType<typeof findPaneAnywhere>;
  commitSplitRatio(splitId: string, ratio: number): void;
  exportLayout(): SerializedLayout;
  importLayout(json: SerializedLayout): void;
}

export function createLayoutActions(context: WindowStoreContext): LayoutActions {
  const { store, setStore } = context;

  const setActivePane = (paneId: string | null) => {
    setStore('activePaneId', paneId);
    // Panes live in the center tiling OR inside an edge panel's tree —
    // focus follows the pane's actual region.
    setStore('focusedRegion', paneId ? regionOfPane(store, paneId) : 'center');
    // Focusing a pane makes its visible chat the target of session-scoped
    // commands (Ctrl+K clear, switch-model) — see syncActiveSession in
    // tabActions for the tab-activation half.
    if (paneId) {
      const pane = findPaneAnywhere(store, paneId);
      const group = pane?.tabGroupId ? store.tabGroups[pane.tabGroupId] : null;
      const activeTab = group?.tabs.find((t) => t.id === group.activeTabId);
      const sessionId = activeTab?.metadata?.sessionId;
      if (typeof sessionId === 'string') {
        statusBarActions.setActiveSessionId(sessionId);
      }
      syncShellSurface(activeTab);
    }
  };

  const toggleEdgePanel = (position: EdgePanelPosition) => {
    setStore(
      produce((s) => {
        s.edgePanels[position].isCollapsed = !s.edgePanels[position].isCollapsed;
      })
    );
  };

  /**
   * Mirror the two side panels: what was on the left is now on the right.
   *
   * The CONTENTS move, the positions do not. Panels are keyed by position
   * here, and the panel toggles are positional too (`toggleEdgePanel('left')`
   * means "the left side", not "the session list"), so after a swap every
   * toggle, ribbon and drop target still says what it does — nothing has to
   * be remapped.
   *
   * `layout` and `width` travel with the contents: a file tree dragged out to
   * 320px stays 320px on its new side rather than being re-cramped every
   * swap. `isCollapsed` stays with the SIDE, because it describes the side
   * you are looking at. That is what makes the common gesture work — with the
   * right rail collapsed, one swap brings the tree onto the visible left and
   * stows the session list. Carrying collapse across would instead hide both
   * rails at once, which reads as the feature being broken.
   *
   * The cost is that it is not a strict involution when the two sides differ
   * in collapsed state; two presses can land somewhere other than the start.
   * Deliberate: predictable-and-useful beats symmetric-and-surprising.
   */
  const swapSidePanels = () => {
    setStore(
      produce((s) => {
        const { left, right } = s.edgePanels;
        // `id` stays with the side alongside `isCollapsed` — it names the
        // panel, and findEdgePanelForGroup answers in positions.
        const leftLayout = left.layout;
        const leftWidth = left.width;
        left.layout = right.layout;
        left.width = right.width;
        right.layout = leftLayout;
        right.width = leftWidth;
        // The focus ring is drawn where this says, and the panes it named
        // just moved. Every other mover recomputes it from the tree
        // (findEdgePanelForPane / ForGroup); here the answer is known, so
        // flip it. A focus on the center or the bottom is untouched.
        if (s.focusedRegion === 'left') s.focusedRegion = 'right';
        else if (s.focusedRegion === 'right') s.focusedRegion = 'left';
      })
    );
  };

  const setEdgePanelCollapsed = (
    position: EdgePanelPosition,
    collapsed: boolean
  ) => {
    setStore('edgePanels', position, 'isCollapsed', collapsed);
  };

  const setEdgePanelActiveTab = (
    position: EdgePanelPosition,
    tabId: string | null
  ) => {
    // The panel is a layout tree — activate the tab in whichever leaf group
    // holds it (null clears the first group's active tab).
    const groupIds = collectLeafGroupIds(store.edgePanels[position].layout);
    const groupId =
      tabId === null
        ? groupIds[0]
        : groupIds.find((id) =>
            store.tabGroups[id]?.tabs.some((t) => t.id === tabId)
          );
    if (!groupId) return;
    setStore('tabGroups', groupId, 'activeTabId', tabId);
    setStore('focusedRegion', position);
  };

  const setEdgePanelSize = (position: EdgePanelPosition, size: number) => {
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
  };

  const getTabGroup = (groupId: string) => {
    return store.tabGroups[groupId];
  };

  const getPaneTabGroupId = (paneId: string): string | null => {
    const pane = findPaneAnywhere(store, paneId);
    return pane?.tabGroupId ?? null;
  };

  const findPaneById = (paneId: string) => {
    return findPaneAnywhere(store, paneId);
  };

  /** Persist a splitter drag — the split may live in the center tiling or
   * inside an edge panel's tree. */
  const commitSplitRatio = (splitId: string, ratio: number) => {
    const containsSplit = (root: LayoutNode): boolean => {
      if (root.type === 'pane') return false;
      if (root.id === splitId) return true;
      return containsSplit(root.first) || containsSplit(root.second);
    };
    setStore(
      produce((s) => {
        updateRootWhere(s, containsSplit, (root) =>
          updateSplitRatio(root, splitId, ratio)
        );
      })
    );
  };

  const exportLayout = (): SerializedLayout => {
    // Transient (hover) windows are popovers, not workspace state — a saved
    // layout must not resurrect them (or their tab groups) on reload.
    const transientGroups = new Set(
      store.floatingWindows.filter((w) => w.transient).map((w) => w.tabGroupId)
    );
    const tabGroups = Object.fromEntries(
      Object.entries(store.tabGroups).filter(([id]) => !transientGroups.has(id))
    );
    return serializeLayout({
      layout: store.layout,
      tabGroups,
      edgePanels: { ...store.edgePanels } as Record<EdgePanelPosition, EdgePanelType>,
      floatingWindows: store.floatingWindows.filter((w) => !w.transient),
    });
  };

  const importLayout = (json: SerializedLayout) => {
    const restored = deserializeLayout(json);
    // Snap-not-tween marker: effects reacting to this store swap (edge-panel
    // collapse states) must apply instantly — see lib/layout-restore.
    markLayoutRestore(() =>
    setStore(
      produce((s) => {
        s.layout = restored.layout;
        s.tabGroups = restored.tabGroups;
        s.edgePanels = restored.edgePanels as Record<EdgePanelPosition, EdgePanelType>;
        s.floatingWindows = restored.floatingWindows;
        s.activePaneId = null;
        s.focusedRegion = 'center';
        s.nextZIndex = 100;
        const firstPane = findFirstPane(s.layout);
        if (firstPane) s.activePaneId = firstPane.id;
      })
    ));
  };

  return {
    setActivePane,
    toggleEdgePanel,
    swapSidePanels,
    setEdgePanelCollapsed,
    setEdgePanelActiveTab,
    setEdgePanelSize,
    getTabGroup,
    getPaneTabGroupId,
    findPaneById,
    commitSplitRatio,
    exportLayout,
    importLayout,
  };
}
