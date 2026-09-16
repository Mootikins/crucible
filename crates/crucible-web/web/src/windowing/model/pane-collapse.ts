import type { LayoutNode } from './types';

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
const COLLAPSED_PANE_PX = 36;

/**
 * What a pane keeps when it yields its share to a sibling that holds content.
 *
 * Wide enough for the empty-pane affordance and for the drop zones the pane
 * still offers, and no wider — the pane held half the centre against nothing,
 * which pushed the two ends of one line of content ~250px apart.
 */
export const EMPTY_PANE_PX = 220;

/** A leaf pane the user collapsed (see `PaneNode.collapsed`). A split branch
 * is never collapsed — only its own leaves are. */
export function isCollapsedLeaf(node: LayoutNode): boolean {
  return node.type === 'pane' && node.collapsed === true;
}

/** A side pinned to a fixed size, and to what: its strip, or the affordance. */
function fixedBasisPx(node: LayoutNode, yielded: boolean): number | null {
  if (isCollapsedLeaf(node)) return COLLAPSED_PANE_PX;
  if (yielded) return EMPTY_PANE_PX;
  return null;
}

/**
 * The `flex` shorthand for BOTH halves of a split.
 *
 * A collapsed side takes its strip and nothing more; a yielding side takes the
 * affordance strip and nothing more; either way the other side grows into
 * everything left, whatever its ratio says. `splitRatio` is never rewritten to
 * express either state — it stays the size the pane opens back to.
 *
 * The two halves are decided TOGETHER because the growing half's factor
 * depends on the other one. Flexbox hands out free space in proportion to the
 * grow factors and KEEPS the remainder when they sum to under 1, so a lone
 * `0.5` beside a fixed side claimed half the free space and left the other
 * half as a hole in the layout — measured at 348px of dead centre beside a
 * yielding pane. The growing half therefore takes `1`, not its ratio.
 *
 * The basis carries its `px` unit even at zero. A browser accepts the bare
 * `0`, but jsdom's CSS parser rejects the whole declaration and silently keeps
 * the previous one — which makes every split ratio untestable and, worse,
 * makes a stale one look correct.
 */
export function splitFlex(
  first: LayoutNode,
  second: LayoutNode,
  ratio: number,
  yields: { first: boolean; second: boolean } = { first: false, second: false },
): { first: string; second: string } {
  const firstPx = fixedBasisPx(first, yields.first);
  const secondPx = fixedBasisPx(second, yields.second);
  const side = (own: number | null, other: number | null, share: number) => {
    if (own !== null) return `0 0 ${own}px`;
    if (other !== null) return '1 1 0px';
    return `${share} 1 0px`;
  };
  return {
    first: side(firstPx, secondPx, ratio),
    second: side(secondPx, firstPx, 1 - ratio),
  };
}
