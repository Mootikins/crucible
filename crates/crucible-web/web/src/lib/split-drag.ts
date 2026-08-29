import { windowActions } from '@/stores/windowStore';

/** A split never drags past this much of its container, either way. */
export const SPLIT_RATIO_MIN = 0.1;
export const SPLIT_RATIO_MAX = 0.9;

export interface SplitDragOptions {
  event: PointerEvent;
  splitId: string;
  direction: 'horizontal' | 'vertical';
  startRatio: number;
  /** The box the ratio is measured against — the split's own container. */
  getContainerRect: () => DOMRect | null;
  /** Live ratio during the drag, for a local preview. */
  onPreview: (ratio: number) => void;
  /** Fired once, after the ratio is committed to the store. */
  onEnd: () => void;
}

/**
 * One drag implementation for a split, wherever the grab happened.
 *
 * TWO surfaces move the same boundary: the splitter drawn between the panes,
 * and the rule above a pane's ribbon marker — the marker sits exactly on the
 * boundary, so the boundary is what it grabs. Two copies of this arithmetic
 * would drift, and a drift here reads as the rail and the ribbon disagreeing
 * about where the pane starts, which is the exact bug the measured strip
 * exists to prevent.
 *
 * Returns a cleanup that ends the drag early (component teardown mid-drag).
 */
export function startSplitDrag(opts: SplitDragOptions): () => void {
  const { event, splitId, direction, startRatio } = opts;
  const el = event.currentTarget as HTMLElement;
  el.setPointerCapture(event.pointerId);

  const startX = event.clientX;
  const startY = event.clientY;
  let ratio = startRatio;

  const onMove = (e: PointerEvent) => {
    const rect = opts.getContainerRect();
    if (!rect) return;
    const next =
      direction === 'horizontal'
        ? startRatio + (e.clientX - startX) / rect.width
        : startRatio + (e.clientY - startY) / rect.height;
    ratio = Math.max(SPLIT_RATIO_MIN, Math.min(SPLIT_RATIO_MAX, next));
    opts.onPreview(ratio);
  };

  const finish = () => {
    document.removeEventListener('pointermove', onMove);
    document.removeEventListener('pointerup', onUp);
  };

  const onUp = (e: PointerEvent) => {
    windowActions.commitSplitRatio(splitId, ratio);
    try {
      el.releasePointerCapture(e.pointerId);
    } catch {
      // The element may already be gone if the layout changed mid-drag.
    }
    finish();
    opts.onEnd();
  };

  document.addEventListener('pointermove', onMove);
  document.addEventListener('pointerup', onUp);

  return () => {
    finish();
    opts.onEnd();
  };
}
