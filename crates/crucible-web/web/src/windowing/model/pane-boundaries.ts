import type { LayoutNode } from './types';

/**
 * The split each pane's TOP EDGE belongs to, or null for the topmost pane.
 *
 * A pane's top edge is a boundary you can drag only when some split put it
 * there. Walking the tree: everything in a split's `first` half keeps whatever
 * boundary it inherited, and the `second` half's leading edge IS this split —
 * so the first leaf under `second` answers with this split's id, and leaves
 * deeper inside answer with their own nearer split.
 *
 * The topmost pane of a tree has no split above it and no edge to drag, which
 * is why the value is nullable rather than absent.
 */
export function paneBoundaries(layout: LayoutNode): Map<string, string | null> {
  const out = new Map<string, string | null>();
  const walk = (node: LayoutNode, inherited: string | null): void => {
    if (node.type === 'pane') {
      out.set(node.id, inherited);
      return;
    }
    walk(node.first, inherited);
    walk(node.second, node.id);
  };
  walk(layout, null);
  return out;
}

/** The split node with this id, or null. */
export function findSplitInLayout(
  layout: LayoutNode,
  splitId: string,
): Extract<LayoutNode, { type: 'split' }> | null {
  if (layout.type === 'pane') return null;
  if (layout.id === splitId) return layout;
  return findSplitInLayout(layout.first, splitId) ?? findSplitInLayout(layout.second, splitId);
}
