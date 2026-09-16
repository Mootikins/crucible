import type { SetStoreFunction } from 'solid-js/store';
import type {
  EdgePanelPosition,
  LayoutNode,
  PaneDropPosition,
  PaneNode,
  TabGroup,
  WindowState,
} from './types';

export interface WindowStoreContext<C extends string = string> {
  store: WindowState<C>;
  setStore: SetStoreFunction<WindowState<C>>;
}

export const generateId = () => Math.random().toString(36).substring(2, 11);

export function findPaneInLayout(
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

export function updatePaneInLayout(
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

export function replacePaneWithSplit(
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

export function findFirstPane(layout: LayoutNode): PaneNode | null {
  if (layout.type === 'pane') return layout;
  return findFirstPane(layout.first) || findFirstPane(layout.second);
}

export function collapseEmptyNodes<C extends string>(
  layout: LayoutNode,
  tabGroups: Record<string, TabGroup<C>>
): LayoutNode {
  if (layout.type === 'pane') return layout;

  const first = collapseEmptyNodes(layout.first, tabGroups);
  const second = collapseEmptyNodes(layout.second, tabGroups);

  const isEmptyPane = (node: LayoutNode): boolean =>
    node.type === 'pane' &&
    (node.tabGroupId === null || !(node.tabGroupId in tabGroups));

  if (isEmptyPane(first)) return second;
  if (isEmptyPane(second)) return first;

  return { ...layout, first, second };
}

export function insertPaneRelative(
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

/**
 * Mirror a layout tree left-to-right.
 *
 * HORIZONTAL splits swap their halves and invert the ratio; VERTICAL splits
 * only recurse. That asymmetry is the whole point: a flip reverses the COLUMN
 * order, and anything stacked inside a column keeps its stacking. The terminal
 * under the file tree stays under the file tree when the tree moves sides —
 * mirroring vertical splits too would put it above.
 *
 * Pure, and its own inverse: mirroring twice restores the original tree,
 * including every ratio.
 */
export function mirrorLayout(node: LayoutNode): LayoutNode {
  if (node.type === 'pane') return node;
  if (node.direction !== 'horizontal') {
    return { ...node, first: mirrorLayout(node.first), second: mirrorLayout(node.second) };
  }
  return {
    ...node,
    first: mirrorLayout(node.second),
    second: mirrorLayout(node.first),
    // The ratio measures the FIRST half, and the halves just traded places.
    splitRatio: 1 - node.splitRatio,
  };
}

/** In-order tab-group ids at the leaves of a layout tree. */
export function collectLeafGroupIds(layout: LayoutNode): string[] {
  if (layout.type === 'pane') {
    return layout.tabGroupId ? [layout.tabGroupId] : [];
  }
  return [...collectLeafGroupIds(layout.first), ...collectLeafGroupIds(layout.second)];
}

/** Count of leaf panes in a layout tree. */
export function countPanes(layout: LayoutNode): number {
  if (layout.type === 'pane') return 1;
  return countPanes(layout.first) + countPanes(layout.second);
}

/** In-order leaf panes of a layout tree. */
export function collectPanes(layout: LayoutNode): PaneNode[] {
  if (layout.type === 'pane') return [layout];
  return [...collectPanes(layout.first), ...collectPanes(layout.second)];
}

/** Leaf panes a rail still shows at full height. */
export function expandedPanes(layout: LayoutNode): PaneNode[] {
  return collectPanes(layout).filter((p) => !p.collapsed);
}

export function findEdgePanelForGroup<C extends string>(
  state: WindowState<C>,
  groupId: string
): EdgePanelPosition | null {
  for (const pos of ['left', 'right'] as EdgePanelPosition[]) {
    if (collectLeafGroupIds(state.edgePanels[pos].layout).includes(groupId)) {
      return pos;
    }
  }
  return null;
}

export function findEdgePanelForPane<C extends string>(
  state: WindowState<C>,
  paneId: string
): EdgePanelPosition | null {
  for (const pos of ['left', 'right'] as EdgePanelPosition[]) {
    if (findPaneInLayout(state.edgePanels[pos].layout, paneId)) return pos;
  }
  return null;
}

/** Search every layout root (center tiling + edge panels) for a pane. */
export function findPaneAnywhere<C extends string>(
  state: WindowState<C>,
  paneId: string
): PaneNode | null {
  const inMain = findPaneInLayout(state.layout, paneId);
  if (inMain) return inMain;
  for (const pos of ['left', 'right'] as EdgePanelPosition[]) {
    const pane = findPaneInLayout(state.edgePanels[pos].layout, paneId);
    if (pane) return pane;
  }
  return null;
}

/** The pane region a pane lives in: an edge position or the center tiling. */
export function regionOfPane<C extends string>(
  state: WindowState<C>,
  paneId: string
): EdgePanelPosition | 'center' {
  return findEdgePanelForPane(state, paneId) ?? 'center';
}

/**
 * Apply a tree transform to whichever layout root (center or edge panel)
 * satisfies `contains`. Mutates the draft state; returns true when a root
 * matched. This is what makes every split/drop/collapse operation work
 * identically in the center tiling and inside edge panels.
 */
export function updateRootWhere<C extends string>(
  s: WindowState<C>,
  contains: (root: LayoutNode) => boolean,
  transform: (root: LayoutNode) => LayoutNode
): boolean {
  if (contains(s.layout)) {
    s.layout = transform(s.layout);
    return true;
  }
  for (const pos of ['left', 'right'] as EdgePanelPosition[]) {
    if (contains(s.edgePanels[pos].layout)) {
      s.edgePanels[pos].layout = transform(s.edgePanels[pos].layout);
      return true;
    }
  }
  return false;
}

/** The group new tabs land in when a whole edge panel is the drop target:
 * its first leaf group (top/leading pane). */
export function primaryEdgeGroupId<C extends string>(
  state: WindowState<C>,
  pos: EdgePanelPosition
): string | null {
  return collectLeafGroupIds(state.edgePanels[pos].layout)[0] ?? null;
}

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
