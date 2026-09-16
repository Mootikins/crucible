import { produce } from 'solid-js/store';
import type {
  EdgePanel as EdgePanelType,
  EdgePanelPosition,
  LayoutNode,
  Tab,
  TabContentType,
  TabGroup,
} from '@/types/windowTypes';
import { isEdgeCollapsed } from '@/types/windowTypes';
import { iconForContentType } from '@/lib/tab-icons';
import type { SerializedLayout } from '@/lib/layout-serializer';
import {
  deserializeLayout,
  serializeLayout,
} from '@/lib/layout-serializer';
import { markLayoutRestore } from '@/lib/layout-restore';
import type { WindowStoreContext } from './windowStoreInternals';
import type { WindowState } from '@/types/windowTypes';
import {
  collapseEmptyNodes,
  createInitialState,
  collectLeafGroupIds,
  expandedPanes,
  findFirstPane,
  generateId,
  findPaneAnywhere,
  findPaneInLayout,
  mirrorLayout,
  regionOfPane,
  updateRootWhere,
  updateSplitRatio,
} from './windowStoreInternals';
import { statusBarActions } from './statusBarStore';
import { syncShellSurface } from './shellStore';

/**
 * The two panels the shell always keeps, and the rail each one belongs to.
 *
 * Sessions on the left and Files on the right are not tabs like the others:
 * they are the two ways INTO the app. Everything else opens from one of them,
 * or from the palette, so a shell with neither is a shell with no doorway —
 * and that is the state a user reached, because a rail could be emptied and a
 * restore brought it back empty. `swapSidePanels` still moves them as a pair,
 * so the rule names a DEFAULT side, not a fixed one.
 *
 * The title travels with the rule rather than being read from the panel
 * registry: this store must be able to repair a layout during boot, before
 * anything registers a panel, and `Sessions` / `Files` are the registry's
 * titles anyway (see lib/register-panels.tsx).
 */
export const FIXED_RAIL_PANELS: Record<
  EdgePanelPosition,
  { contentType: TabContentType; title: string }
> = {
  left: { contentType: 'sessions', title: 'Sessions' },
  right: { contentType: 'files', title: 'Files' },
};

const FIXED_RAIL_TYPES: TabContentType[] = Object.values(FIXED_RAIL_PANELS).map(
  (p) => p.contentType,
);

/** Every tab of one content type, in every group, wherever it is docked. */
function tabsOfType(s: WindowState, contentType: TabContentType): Tab[] {
  return Object.values(s.tabGroups).flatMap((g) =>
    g.tabs.filter((t) => t.contentType === contentType),
  );
}

/**
 * True when closing this tab would leave the shell with no Sessions panel, or
 * no Files panel.
 *
 * The rule is "never zero", not "never leaves the rail". A user may drag
 * Sessions into the centre and keep working, and may close a SECOND copy of
 * it; what nothing may do is take the last one away.
 */
export function isLastFixedRailTab(
  s: WindowState,
  groupId: string,
  tabId: string,
): boolean {
  const tab = s.tabGroups[groupId]?.tabs.find((t) => t.id === tabId);
  if (!tab || !FIXED_RAIL_TYPES.includes(tab.contentType)) return false;
  return tabsOfType(s, tab.contentType).length <= 1;
}

/** The rail that currently holds one content type, or undefined. */
function railHolding(
  s: WindowState,
  contentType: TabContentType,
): EdgePanelPosition | undefined {
  return (['left', 'right'] as EdgePanelPosition[]).find((pos) =>
    collectLeafGroupIds(s.edgePanels[pos].layout).some((id) =>
      s.tabGroups[id]?.tabs.some((t) => t.contentType === contentType),
    ),
  );
}

/** Put one fixed panel back on a rail, and open that rail so the repair shows. */
function addFixedRailTab(
  s: WindowState,
  pos: EdgePanelPosition,
  panel: { contentType: TabContentType; title: string },
): void {
  const tab: Tab = {
    id: `${panel.contentType}-tab`,
    title: panel.title,
    contentType: panel.contentType,
    icon: iconForContentType(panel.contentType),
  };
  let groupId = collectLeafGroupIds(s.edgePanels[pos].layout).find((id) => s.tabGroups[id]);
  if (!groupId) {
    // The rail has no resolvable group — a restore of a layout with no panel
    // at this position synthesizes a pane with a null tab group.
    groupId = generateId();
    s.tabGroups[groupId] = { id: groupId, tabs: [], activeTabId: null };
    const pane = findFirstPane(s.edgePanels[pos].layout);
    if (pane) pane.tabGroupId = groupId;
    else s.edgePanels[pos].layout = { id: `${pos}-pane`, type: 'pane', tabGroupId: groupId };
  }
  const group = s.tabGroups[groupId];
  // Leading, and active: the rail's own panel reads first on its tab strip,
  // and a repair the user cannot see is not a repair.
  group.tabs = [tab, ...group.tabs];
  group.activeTabId = tab.id;
  s.edgePanels[pos].mode = 'docked';
}

/**
 * Put back any fixed panel the incoming layout lost. Mutates the draft.
 *
 * Runs on the two paths that write a whole layout — restore and reset — which
 * are the only ways a state the store did not build enters it. Nothing is
 * added while a copy is open anywhere else, so a user who docked Sessions in
 * the centre keeps one Sessions panel, not two.
 */
export function ensureFixedRails(s: WindowState): void {
  for (const pos of ['left', 'right'] as EdgePanelPosition[]) {
    const panel = FIXED_RAIL_PANELS[pos];
    if (tabsOfType(s, panel.contentType).length > 0) continue;
    const other: EdgePanelPosition = pos === 'left' ? 'right' : 'left';
    // A swapped layout holds the other fixed panel on this side. Re-add on
    // the free rail rather than stacking both on one.
    const target = railHolding(s, FIXED_RAIL_PANELS[other].contentType) === pos ? other : pos;
    addFixedRailTab(s, target, panel);
  }
}

export interface LayoutActions {
  setActivePane(paneId: string | null): void;
  toggleEdgePanel(position: EdgePanelPosition): void;
  swapSidePanels(): void;
  setEdgePanelCollapsed(position: EdgePanelPosition, collapsed: boolean): void;
  setEdgePanelActiveTab(position: EdgePanelPosition, tabId: string | null): void;
  setEdgePanelSize(position: EdgePanelPosition, size: number): void;
  setPaneCollapsed(
    paneId: string,
    collapsed: boolean
  ): void;
  togglePaneCollapsed(paneId: string): void;
  getTabGroup(groupId: string): TabGroup | undefined;
  getPaneTabGroupId(paneId: string): string | null;
  findPaneById(paneId: string): ReturnType<typeof findPaneAnywhere>;
  commitSplitRatio(splitId: string, ratio: number): void;
  exportLayout(): SerializedLayout;
  importLayout(json: SerializedLayout): void;
  /** Throw the local pane layout away and start from the shipped default. */
  resetLayoutToDefaults(): void;
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
   * `layout`, `width` and `mode` all travel with the contents. A file
   * tree dragged out to 320px stays 320px on its new side rather than being
   * re-cramped every swap. A panel the user stowed stays stowed, and a panel
   * the user opened stays open. The flip moves panels between sides; it does
   * not open or stow anything.
   *
   * The collapse flag (`isCollapsed`, now `mode`) used to stay with the SIDE,
   * to make one gesture work: with the right rail stowed, one flip put the
   * tree on the visible left. But it
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
        // WindowManager keys the shell row by it, which is what lets Solid
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
    /**
     * A restored layout has to satisfy the same invariant every mutation
     * maintains: no pane may point at a tab group that does not exist.
     *
     * It did not. `collapseEmptyNodes` ran on tab close and on window
     * operations, but never on RESTORE — so a saved layout carrying a pane
     * whose `tabGroupId` is dangling brought that pane back on every load,
     * drawing EmptyPane beside the real work. Nothing the user could do
     * reached it: opening a session or a file adds a tab to some OTHER pane,
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
        // The rails are fixed: a stored layout that lost one gets it back
        // here, on the state the restore just wrote.
        ensureFixedRails(s);
        const firstPane = findFirstPane(s.layout);
        if (firstPane) s.activePaneId = firstPane.id;
      })
    ));
  };

  /**
   * Throw the local pane layout away and start from the shipped default.
   *
   * In-place rather than a page reload: a reload would also drop every open
   * session's live SSE stream and the editor's unsaved buffers, which a
   * request to rearrange PANES never asked for. `createInitialState` is the
   * same function that builds the layout on a first run, so "reset" and
   * "never opened this app before" land on exactly one shape.
   *
   * Deleting the SERVER's copy is the caller's job (`resetLayout()` in
   * lib/api) — this store does not know the persistence layer exists, and the
   * auto-save that follows this write would otherwise put the default straight
   * back on disk under a new version anyway.
   */
  const resetLayoutToDefaults = () => {
    const fresh = createInitialState();
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
          // The seed ships both rails, so this changes nothing today. It is
          // here so that "reset" cannot drift away from the invariant the
          // restore path enforces — one function decides what a rail is.
          ensureFixedRails(s);
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
