import type { LayoutNode, TabGroup } from './types';

/**
 * Which panes hold something, for the two rules that ask that question: the
 * empty-pane affordance, and the split-size rule that lets a pane with content
 * claim the width an empty sibling does not use.
 *
 * The tab groups arrive as a plain record rather than through the store, so
 * both rules are testable without a mounted component.
 */
export type TabGroups = Record<string, TabGroup>;

/** A leaf pane holds content when its tab group holds at least one tab. */
export function paneHasTabs(groups: TabGroups, node: LayoutNode): boolean {
  if (node.type !== 'pane' || !node.tabGroupId) return false;
  return (groups[node.tabGroupId]?.tabs.length ?? 0) > 0;
}

/** Any leaf pane under this node holds content. */
export function subtreeHasTabs(groups: TabGroups, node: LayoutNode): boolean {
  if (node.type === 'pane') return paneHasTabs(groups, node);
  return subtreeHasTabs(groups, node.first) || subtreeHasTabs(groups, node.second);
}

/**
 * Some pane OTHER than `paneId` holds content in this tree.
 *
 * This separates the two empty states an empty pane can be in. False means the
 * region has nothing open at all; true means the user emptied this one pane
 * beside panes that still hold work.
 */
export function hasTabsOutsidePane(
  groups: TabGroups,
  root: LayoutNode,
  paneId: string,
): boolean {
  if (root.type === 'pane') {
    return root.id !== paneId && paneHasTabs(groups, root);
  }
  return (
    hasTabsOutsidePane(groups, root.first, paneId) ||
    hasTabsOutsidePane(groups, root.second, paneId)
  );
}
