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
 * - Width: the rail's stored width, and never less than `FLYOUT_MIN`.
 * - Height: half the viewport height, and never less than `FLYOUT_MIN`.
 *   The room is the viewport height minus a margin at the top and at the
 *   bottom. The height never exceeds the room. When the room is less than
 *   `FLYOUT_MIN`, the room wins, so the flyout stays on the screen.
 * - Top: the top of the anchor. When the bottom would pass the bottom
 *   margin, the flyout moves up to that margin. When the anchor is above the
 *   top margin, the flyout moves down to that margin.
 * - Side: a left-rail flyout starts at the right edge of the anchor. A
 *   right-rail flyout ends at the left edge of the anchor.
 *
 * Ported from the old flexlayout core, without the bottom dock.
 */
export function flyoutRect(p: FlyoutParams): Rect {
  const width = Math.max(FLYOUT_MIN, p.width);
  const room = Math.max(0, p.viewport.height - 2 * FLYOUT_MARGIN);
  const height = Math.min(room, Math.max(FLYOUT_MIN, p.viewport.height * FLYOUT_HEIGHT_FRACTION));
  const lowestTop = p.viewport.height - FLYOUT_MARGIN - height;
  // height <= room, so lowestTop >= FLYOUT_MARGIN and the clamp is well formed.
  const y = Math.min(lowestTop, Math.max(FLYOUT_MARGIN, p.anchor.y));
  const x = p.position === 'left' ? p.anchor.x + p.anchor.width : p.anchor.x - width;
  return { x, y, width, height };
}
