import { createSignal, type Accessor } from 'solid-js';
import { settleDrawer, startsInEdge, type DrawerSide } from '@/components/mobile/drawer-gesture';

/** One pointer sample, reduced to what the gesture reads. */
export interface SwipePoint {
  x: number;
  y: number;
  /** A timestamp in ms. */
  t: number;
  /** Whether the pointer is over the drawer or its scrim. */
  inDrawer: boolean;
}

/** Movement below this, in px, has no direction yet. */
const SLOP_PX = 8;

/**
 * Track one drawer's swipe: open from the screen edge, or close from inside.
 *
 * The caller feeds pointer samples in and reads `dragPx` out — how far open the
 * drawer should draw while a finger holds it, or `null` when no finger does.
 * On release it calls `onSettle(open)`. The handlers take plain samples, not
 * DOM events, so the shell adapts pointer events once and a test needs no DOM.
 */
export function createEdgeSwipe(opts: {
  side: DrawerSide;
  width: Accessor<number>;
  viewportWidth: Accessor<number>;
  isOpen: Accessor<boolean>;
  onSettle: (open: boolean) => void;
}) {
  const [dragPx, setDragPx] = createSignal<number | null>(null);

  let start: SwipePoint | null = null;
  let mode: 'open' | 'close' | null = null;
  let engaged = false;
  let last: { openPx: number; t: number } | null = null;
  let prev: { openPx: number; t: number } | null = null;

  const reset = () => {
    start = null;
    mode = null;
    engaged = false;
    last = null;
    prev = null;
    setDragPx(null);
  };

  /** Travel in the OPENING direction: rightward for the left drawer. */
  const openingTravel = (p: SwipePoint) =>
    start ? (opts.side === 'left' ? p.x - start.x : start.x - p.x) : 0;

  return {
    dragPx,
    down(p: SwipePoint) {
      reset();
      if (!opts.isOpen() && startsInEdge({ x: p.x, viewportWidth: opts.viewportWidth(), side: opts.side })) {
        mode = 'open';
      } else if (opts.isOpen() && p.inDrawer) {
        mode = 'close';
      } else {
        return;
      }
      start = p;
    },
    move(p: SwipePoint) {
      if (!start || !mode) return;
      const dx = Math.abs(p.x - start.x);
      const dy = Math.abs(p.y - start.y);
      if (!engaged) {
        if (dx < SLOP_PX && dy < SLOP_PX) return;
        // A mostly vertical drag is a scroll. Let it go for the whole gesture.
        if (dy >= dx) {
          reset();
          return;
        }
        engaged = true;
      }
      const base = mode === 'open' ? 0 : opts.width();
      const openPx = Math.min(opts.width(), Math.max(0, base + openingTravel(p)));
      prev = last;
      last = { openPx, t: p.t };
      setDragPx(openPx);
    },
    up(p: SwipePoint) {
      if (!engaged || !last) {
        reset();
        return;
      }
      // Velocity over the last sample interval, in the opening direction.
      const ref = prev ?? { openPx: mode === 'open' ? 0 : opts.width(), t: start!.t };
      const final = Math.min(opts.width(), Math.max(0, (mode === 'open' ? 0 : opts.width()) + openingTravel(p)));
      const dt = Math.max(1, p.t - ref.t);
      const velocity = (final - ref.openPx) / dt;
      opts.onSettle(settleDrawer({ width: opts.width(), openPx: final, velocity }));
      reset();
    },
    cancel: reset,
  };
}
