import type { EdgePanelPosition } from '../model/types';

/** The smallest flyout side, in px, while the viewport has room for it. */
export const FLYOUT_MIN = 100;
/** The gap between the flyout and the top or bottom of the viewport, in px. */
export const FLYOUT_MARGIN = 8;
/** The default flyout height, as a part of the viewport height. */
export const FLYOUT_HEIGHT_FRACTION = 0.5;

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface FlyoutParams {
  position: EdgePanelPosition;
  /** The ribbon button that opened the flyout, in viewport coordinates. */
  anchor: Rect;
  viewport: { width: number; height: number };
  /** The rail's stored width. */
  width: number;
}

/**
 * Where a flyout goes. A pure function of its parameters.
 *
 * The rule is the same on both axes. The room on an axis is the viewport
 * size minus a margin at each end. A side is never less than `FLYOUT_MIN`
 * and never more than the room. When the room is less than `FLYOUT_MIN`,
 * the room wins, so the flyout stays on the screen. A clamp then keeps the
 * position between the two margins.
 *
 * - Width: the rail's stored width.
 * - Height: half the viewport height.
 * - Top: the top of the anchor. When the bottom would pass the bottom
 *   margin, the flyout moves up to that margin. When the anchor is above the
 *   top margin, the flyout moves down to that margin.
 * - Left: a left-rail flyout starts at the right edge of the anchor. A
 *   right-rail flyout ends at the left edge of the anchor. When that puts
 *   the flyout past a side margin, the flyout moves back to that margin.
 *
 * Ported from the old flexlayout core, without the bottom dock.
 */
export function flyoutRect(p: FlyoutParams): Rect {
  const width = fit(p.viewport.width, p.width);
  const height = fit(p.viewport.height, p.viewport.height * FLYOUT_HEIGHT_FRACTION);
  const x = place(p.viewport.width, width, p.position === 'left' ? p.anchor.x + p.anchor.width : p.anchor.x - width);
  const y = place(p.viewport.height, height, p.anchor.y);
  return { x, y, width, height };
}

/** A side on an axis of length `axis`: at least `FLYOUT_MIN`, at most the room. */
function fit(axis: number, preferred: number): number {
  const room = Math.max(0, axis - 2 * FLYOUT_MARGIN);
  return Math.min(room, Math.max(FLYOUT_MIN, preferred));
}

/**
 * A start on an axis of length `axis`, between the two margins. `size` is
 * at most the room, so the upper bound is never below the lower bound.
 */
function place(axis: number, size: number, preferred: number): number {
  return Math.min(axis - FLYOUT_MARGIN - size, Math.max(FLYOUT_MARGIN, preferred));
}
