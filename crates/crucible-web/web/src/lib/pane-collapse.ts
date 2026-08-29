import type { LayoutNode } from '@/types/windowTypes';

/**
 * The size rule for a collapsed pane, shared by the rail body and the ribbon.
 *
 * The ribbon draws one band per pane at the SAME proportions as the panel, so
 * a pane's marker sits beside the pane it controls. Two copies of this rule
 * would drift the moment one of them changed, and the drift is invisible in
 * both places on its own — it only shows as a marker pointing at the wrong
 * pane.
 */

/** A collapsed pane keeps exactly its tab strip: TabBar is `h-9`. */
export const COLLAPSED_PANE_PX = 36;

/** A leaf pane the user collapsed (see `PaneNode.collapsed`). A split branch
 * is never collapsed — only its own leaves are. */
export function isCollapsedLeaf(node: LayoutNode): boolean {
  return node.type === 'pane' && node.collapsed === true;
}

/**
 * The `flex` shorthand for one half of a split.
 *
 * A collapsed side takes its strip and nothing more; the other side grows into
 * everything left, whatever its ratio says. `splitRatio` is never rewritten to
 * express a collapse — it stays the size the pane opens back to.
 *
 * The basis carries its `px` unit even at zero. A browser accepts the bare
 * `0`, but jsdom's CSS parser rejects the whole declaration and silently keeps
 * the previous one — which makes every split ratio untestable and, worse,
 * makes a stale one look correct.
 */
export function paneFlex(node: LayoutNode, ratio: number): string {
  return isCollapsedLeaf(node) ? `0 0 ${COLLAPSED_PANE_PX}px` : `${ratio} 1 0px`;
}
