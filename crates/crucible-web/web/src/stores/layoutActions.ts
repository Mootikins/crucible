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
  expandedPanes,
  findFirstPane,
  findPaneAnywhere,
  findPaneInLayout,
  mirrorLayout,
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
  setRailPaneCollapsed(
    position: EdgePanelPosition,
    paneId: string,
    collapsed: boolean
  ): void;
  toggleRailPaneCollapsed(position: EdgePanelPosition, paneId: string): void;
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
   * Mirror the WHOLE workspace left-to-right — a true flip, not a rail swap.
   *
   * Every column reverses: the two rails trade sides, and the centre tiling
   * reverses with them, so a conversation left of its editor ends up right of
   * it. Anything STACKED inside a column keeps its stacking — a terminal below
   * the file tree is still below it after the flip. `mirrorLayout` encodes
   * that rule (horizontal splits swap, vertical splits do not).
   *
   * It used to move the rails only, which left the centre unmirrored: after a
   * flip the tree sat left, the session list right, and the conversation was
   * still to the left of the editor it belongs to. Half a mirror reads as a
   * bug, because the eye checks the whole row.
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
        left.layout = mirrorLayout(right.layout);
        left.width = right.width;
        right.layout = mirrorLayout(leftLayout);
        right.width = leftWidth;
        // The centre reverses too, or the flip is only half done.
        s.layout = mirrorLayout(s.layout);
        // The focus ring is drawn where this says, and the panes it named
        // just moved. Every other mover recomputes it from the tree
        // (findEdgePanelForPane / ForGroup); here the answer is known, so
        // flip it. A focus on the centre is untouched.
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
        // Both docks are side rails now, so width is the only axis. The
        // `height` branch belonged to the bottom dock.
        s.edgePanels[position].width = clamped;
      })
    );
  };

  /**
   * Collapse ONE pane of a rail to its tab strip, leaving the rail open.
   *
   * Scoped to a rail on purpose: the centre tiling has no ribbon to expand a
   * pane from again, so a collapsed centre pane would be a state with no way
   * out. Passing the position is what enforces that.
   */
  const setRailPaneCollapsed = (
    position: EdgePanelPosition,
    paneId: string,
    collapsed: boolean
  ) => {
    const rail = store.edgePanels[position];
    if (!findPaneInLayout(rail.layout, paneId)) return;
    // The LAST expanded pane may not collapse. A rail of nothing but bars
    // reads as a broken rail, and hiding everything is what the rail's own
    // collapse already does.
    if (
      collapsed &&
      expandedPanes(rail.layout).every((p) => p.id === paneId)
    ) {
      return;
    }
    setStore(
      produce((s) => {
        // Mutated in place rather than rebuilt: only this leaf's flag changes,
        // so the panes around it keep their nodes and never re-render.
        const pane = findPaneInLayout(s.edgePanels[position].layout, paneId);
        if (pane) pane.collapsed = collapsed;
      })
    );
  };

  const toggleRailPaneCollapsed = (
    position: EdgePanelPosition,
    paneId: string
  ) => {
    const pane = findPaneInLayout(store.edgePanels[position].layout, paneId);
    if (!pane) return;
    setRailPaneCollapsed(position, paneId, !pane.collapsed);
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
    setRailPaneCollapsed,
    toggleRailPaneCollapsed,
    getTabGroup,
    getPaneTabGroupId,
    findPaneById,
    commitSplitRatio,
    exportLayout,
    importLayout,
  };
}
