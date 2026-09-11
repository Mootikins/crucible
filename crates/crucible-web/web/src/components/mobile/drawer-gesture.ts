/**
 * The arithmetic of a drawer swipe, kept free of events so a test can pin it.
 * The numbers come from `docs/Meta/Architecture/Mobile Shell.md`, section 5.
 */

export type DrawerSide = 'left' | 'right';

/** How far in from the screen edge a touch may start and still open a drawer. */
export const EDGE_ZONE_PX = 20;

/** The open fraction past which a slow release opens the drawer. */
const OPEN_FRACTION = 0.4;

/** A release faster than this, in px/ms, wins over the position test. */
const FLICK_VELOCITY = 0.5;

/** Whether a touch at `x` starts inside the edge zone that opens `side`. */
export function startsInEdge(opts: { x: number; viewportWidth: number; side: DrawerSide }): boolean {
  return opts.side === 'left'
    ? opts.x <= EDGE_ZONE_PX
    : opts.x >= opts.viewportWidth - EDGE_ZONE_PX;
}

/**
 * Whether a released drawer settles open.
 *
 * `openPx` is how far the drawer is open, 0 to `width`. `velocity` is px/ms in
 * the OPENING direction, so a flick toward closed is negative on either side.
 */
export function settleDrawer(opts: { width: number; openPx: number; velocity: number }): boolean {
  if (Math.abs(opts.velocity) > FLICK_VELOCITY) return opts.velocity > 0;
  return opts.openPx > OPEN_FRACTION * opts.width;
}
