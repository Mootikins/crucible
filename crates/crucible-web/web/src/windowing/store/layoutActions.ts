import { produce } from 'solid-js/store';
import type {
  EdgeCue,
  EdgeMode,
  EdgePanel as EdgePanelType,
  EdgePanelPosition,
  LayoutNode,
  PaneReveal,
  TabGroup,
} from '../model/types';
import { isEdgeCollapsed } from '../model/types';
import type { SerializedLayout, StoredLayout } from '../model/serializer';
import { deserializeLayout, serializeLayout } from '../model/serializer';
import { markLayoutRestore } from '../model/layout-restore';
import type { WindowStoreContext } from '../model/tree';
import {
  collapseEmptyNodes,
  collectLeafGroupIds,
  expandedPanes,
  findFirstPane,
  findPaneAnywhere,
  findPaneInLayout,
  mirrorLayout,
  regionOfPane,
  updateRootWhere,
  updateSplitRatio,
} from '../model/tree';
import type { WindowPolicy } from './policy';

export interface LayoutActions<C extends string = string> {
  setActivePane(paneId: string | null): void;
  toggleEdgePanel(position: EdgePanelPosition): void;
  swapSidePanels(): void;
  setEdgePanelCollapsed(position: EdgePanelPosition, collapsed: boolean): void;
  /**
   * Set how a rail presents. `cue` changes only when given: the cue is a
   * stored preference, and only the `hidden` mode reads it.
   */
  setEdgeMode(position: EdgePanelPosition, mode: EdgeMode, opts?: { cue?: EdgeCue }): void;
  /** Set how a pane opens from its band. The pane can be in the centre or in a rail. */
  setPaneReveal(paneId: string, reveal: PaneReveal): void;
  setEdgePanelActiveTab(position: EdgePanelPosition, tabId: string | null): void;
  setEdgePanelSize(position: EdgePanelPosition, size: number): void;
  setPaneCollapsed(
    paneId: string,
    collapsed: boolean
  ): void;
  togglePaneCollapsed(paneId: string): void;
  getTabGroup(groupId: string): TabGroup<C> | undefined;
  getPaneTabGroupId(paneId: string): string | null;
  findPaneById(paneId: string): ReturnType<typeof findPaneAnywhere>;
  commitSplitRatio(splitId: string, ratio: number): void;
  exportLayout(): SerializedLayout<C>;
  importLayout(json: StoredLayout<C>): void;
  /** Throw the local pane layout away and start from the policy seed. */
  resetLayoutToDefaults(): void;
}

export function createLayoutActions<C extends string>(
  context: WindowStoreContext<C>,
  policy: () => WindowPolicy<C>,
): LayoutActions<C> {
  const { store, setStore } = context;

  const setActivePane = (paneId: string | null) => {
    setStore('activePaneId', paneId);
    // Panes live in the center tiling OR inside an edge panel's tree —
    // focus follows the pane's actual region.
    setStore('focusedRegion', paneId ? regionOfPane(store, paneId) : 'center');
    // Focusing a pane focuses its active tab, so the policy hears of it the
    // same way it does when a tab activates (see setActiveTab).
    if (paneId) {
      const pane = findPaneAnywhere(store, paneId);
      const group = pane?.tabGroupId ? store.tabGroups[pane.tabGroupId] : null;
      policy().onActiveTabChange(group?.tabs.find((t) => t.id === group.activeTabId));
    }
  };

  /**
   * The rail toggle knows two modes only. Any mode that is not `docked`
   * goes to `docked`, and `docked` goes to `strip`. The toggle thus never
   * enters `flyout` or `hidden`, and it always brings a hidden rail back.
   */
  const toggleEdgePanel = (position: EdgePanelPosition) => {
    setStore(
      produce((s) => {
        s.edgePanels[position].mode = isEdgeCollapsed(s.edgePanels[position])
          ? 'docked'
          : 'strip';
      })
    );
  };

  /**
   * Mirror the WHOLE workspace left-to-right — a true flip, not a rail swap.
   *
   * Every column reverses: the two rails trade sides, and the centre tiling
   * reverses with them, so a pane left of its neighbour ends up right of it.
   * Anything STACKED inside a column keeps its stacking — a pane below another
   * pane is still below it after the flip. `mirrorLayout` encodes
   * that rule (horizontal splits swap, vertical splits do not).
   *
   * It used to move the rails only, which left the centre unmirrored: after a
   * flip the rails traded sides, but a centre pane was still to the left of
   * the pane it belongs beside. Half a mirror reads as a bug, because the eye
   * checks the whole row.
   *
   * `layout`, `width` and `mode` all travel with the contents. A rail
   * dragged out to 320px stays 320px on its new side rather than being
   * re-cramped every swap. A panel the user stowed stays stowed, and a panel
   * the user opened stays open. The flip moves panels between sides; it does
   * not open or stow anything.
   *
   * The collapse flag (`isCollapsed`, now `mode`) used to stay with the SIDE,
   * to make one gesture work: with the right rail stowed, one flip put the
   * stowed rail's panel on the visible left. But it
   * separated a panel's collapse from its width and its contents, so a flip
   * silently opened one panel and stowed the other. It also broke the
   * involution — two presses could land somewhere other than the start.
   *
   * To stow a panel, use its own toggle. The toggles stay POSITIONAL, so
   * they need no remapping after a flip.
   */
  const swapSidePanels = () => {
    setStore(
      produce((s) => {
        const { left, right } = s.edgePanels;
        // `id` travels with the contents too. Nothing resolves a panel BY
        // this id — drop targets and every other caller name a position —
        // so it is free to be what it reads as: the moving panel's name.
        // WindowManager keys the rail row by it, which is what lets Solid
        // MOVE a rail across the row instead of rebuilding it.
        const leftLayout = left.layout;
        const leftWidth = left.width;
        const leftMode = left.mode;
        const leftCue = left.cue;
        const leftId = left.id;
        left.layout = mirrorLayout(right.layout);
        left.width = right.width;
        left.mode = right.mode;
        left.cue = right.cue;
        left.id = right.id;
        right.layout = mirrorLayout(leftLayout);
        right.width = leftWidth;
        right.mode = leftMode;
        right.cue = leftCue;
        right.id = leftId;
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
    setStore('edgePanels', position, 'mode', collapsed ? 'strip' : 'docked');
  };

  const setEdgeMode = (
    position: EdgePanelPosition,
    mode: EdgeMode,
    opts?: { cue?: EdgeCue },
  ) => {
    setStore(
      produce((s) => {
        const panel = s.edgePanels[position];
        panel.mode = mode;
        if (opts?.cue !== undefined) panel.cue = opts.cue;
      })
    );
  };

  const setPaneReveal = (paneId: string, reveal: PaneReveal) => {
    setStore(
      produce((s) => {
        updateRootWhere(
          s,
          (root) => !!findPaneInLayout(root, paneId),
          (root) => {
            // Mutated in place, as setPaneCollapsed does, so the panes
            // around this one keep their nodes.
            const pane = findPaneInLayout(root, paneId);
            if (pane) pane.reveal = reveal;
            return root;
          }
        );
      })
    );
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
   * Collapse ONE pane to its tab strip, leaving the region around it open.
   *
   * A pane id is unique across every root, so this takes the id ALONE and
   * finds the root itself — the same thing every other layout action does. It
   * used to demand an `EdgePanelPosition` as well, which asked the caller to
   * tell the dock layer something the dock layer already knows, and made a
   * capability of the layout tree read as a property of the rails.
   *
   * The guard that actually matters is not "which region": it is that a root
   * must keep at least one pane at full height. A region of nothing but bars
   * reads as broken, and hiding everything is what collapsing the whole rail
   * already does. That invariant holds per root, so it covers the centre
   * tiling for free.
   */
  const setPaneCollapsed = (paneId: string, collapsed: boolean) => {
    setStore(
      produce((s) => {
        updateRootWhere(
          s,
          (root) => !!findPaneInLayout(root, paneId),
          (root) => {
            if (collapsed && expandedPanes(root).every((p) => p.id === paneId)) {
              return root;
            }
            // Mutated in place rather than rebuilt: only this leaf's flag
            // changes, so the panes around it keep their nodes and never
            // re-render.
            const pane = findPaneInLayout(root, paneId);
            if (pane) pane.collapsed = collapsed;
            return root;
          }
        );
      })
    );
  };

  const togglePaneCollapsed = (paneId: string) => {
    const pane = findPaneAnywhere(store, paneId);
    if (!pane) return;
    setPaneCollapsed(paneId, !pane.collapsed);
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

  const exportLayout = (): SerializedLayout<C> => {
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

  const importLayout = (json: StoredLayout<C>) => {
    const p = policy();
    const restored = deserializeLayout(json, p.layoutHooks, (type) => p.iconFor(type));
    /**
     * A restored layout has to satisfy the same invariant every mutation
     * maintains: no pane may point at a tab group that does not exist.
     *
     * It did not. `collapseEmptyNodes` ran on tab close and on window
     * operations, but never on RESTORE — so a saved layout carrying a pane
     * whose `tabGroupId` is dangling brought that pane back on every load,
     * drawing EmptyPane beside the real work. Nothing the user could do
     * reached it: opening a new tab adds it to some OTHER pane,
     * and none of those paths collapse anything. The pane was unremovable by
     * construction, and it came back after a reload even if it was collapsed
     * away in a previous run.
     *
     * Sanitising here rather than at the serializer, or on the server, on
     * purpose: this is the one place a layout the store did not build enters
     * the store, so it is the boundary that owes the check. Layouts already
     * on disk repair themselves on the next load.
     *
     * A layout where EVERY pane dangles collapses to a single empty pane
     * rather than to nothing, which is the legitimate "Nothing open" state.
     */
    restored.layout = collapseEmptyNodes(restored.layout, restored.tabGroups);
    // Snap-not-tween marker: effects reacting to this store swap (edge-panel
    // collapse states) must apply instantly — see windowing/model/layout-restore.
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
        // The policy repairs the state the restore just wrote, for example
        // to give back a panel that the stored layout lost.
        policy().repairLayout(s);
        const firstPane = findFirstPane(s.layout);
        if (firstPane) s.activePaneId = firstPane.id;
      })
    ));
  };

  /**
   * Throw the local pane layout away and start from the policy seed.
   *
   * In-place rather than a page reload: a reload would also drop the live
   * state and the unsaved work of every open panel, which a request to
   * rearrange PANES never asked for. The seed is the same
   * function that builds the layout on a first run, so "reset" and
   * "never opened this app before" land on exactly one shape.
   *
   * Deleting the SERVER's copy is the caller's job (`resetLayout()` in
   * lib/api) — this store does not know the persistence layer exists, and the
   * auto-save that follows this write would otherwise put the default straight
   * back on disk under a new version anyway.
   */
  const resetLayoutToDefaults = () => {
    const fresh = policy().seed();
    markLayoutRestore(() =>
      setStore(
        produce((s) => {
          s.layout = fresh.layout;
          s.tabGroups = fresh.tabGroups;
          s.edgePanels = fresh.edgePanels;
          s.floatingWindows = [];
          s.activePaneId = fresh.activePaneId;
          s.focusedRegion = 'center';
          s.nextZIndex = 100;
          // The same repair as a restore, so that "reset" cannot drift away
          // from the invariant the restore path enforces.
          policy().repairLayout(s);
        }),
      ),
    );
  };

  return {
    resetLayoutToDefaults,
    setActivePane,
    toggleEdgePanel,
    swapSidePanels,
    setEdgePanelCollapsed,
    setEdgeMode,
    setPaneReveal,
    setEdgePanelActiveTab,
    setEdgePanelSize,
    setPaneCollapsed,
    togglePaneCollapsed,
    getTabGroup,
    getPaneTabGroupId,
    findPaneById,
    commitSplitRatio,
    exportLayout,
    importLayout,
  };
}
