/**
 * Zoom/pan maths for the image viewer (`ImageViewer.tsx`).
 *
 * Kept free of Solid and the DOM — same discipline as `canvas-viewport.ts` —
 * so the parts most likely to be wrong (the cursor-anchored scroll transform)
 * are testable without a browser.
 *
 * The viewer's layout model: a scroll container (`overflow: auto`) holds a
 * flex-centered wrapper that is at least container-sized, holding the image at
 * `natural × scale` pixels. So when the scaled image is SMALLER than the
 * container on an axis, it is centered by flexbox and the scroll offset is 0;
 * when LARGER, the wrapper grows to the image and native scrolling takes over.
 * All functions here mirror that model.
 */

/** 10% – 1000%. Wider than the canvas range: pixel-peeping is the point. */
export const MIN_SCALE = 0.1;
export const MAX_SCALE = 10;

/** Button/keyboard zoom step. Matches the canvas toolbar's ×1.25. */
export const SCALE_STEP = 1.25;

export function clampScale(scale: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

/**
 * Wheel notch → multiplicative zoom factor.
 *
 * Same curve as the canvas: exponential in deltaY so notches compose (two
 * notches = one double notch) and trackpad micro-deltas produce proportionally
 * small steps. A standard 100-unit notch is ≈ ×1.28 / ×0.78.
 */
export function wheelScaleFactor(deltaY: number): number {
  return Math.exp(-deltaY / 400);
}

/**
 * The scale at which the image fits the pane — contain, DOWNSCALE ONLY.
 *
 * A 16px icon blown up to fill an 800px pane is nobody's idea of "fit", so
 * images smaller than the pane sit at 100% instead of being upscaled. Returns
 * 1 while either box is unmeasured (first paint, hidden tab, image still
 * loading) — the predictable default, corrected on the next measure.
 */
export function fitScale(
  containerW: number,
  containerH: number,
  naturalW: number,
  naturalH: number,
): number {
  if (containerW <= 0 || containerH <= 0 || naturalW <= 0 || naturalH <= 0) return 1;
  return clampScale(Math.min(1, containerW / naturalW, containerH / naturalH));
}

export interface ScrollPos {
  scrollLeft: number;
  scrollTop: number;
}

export interface ViewGeometry {
  containerW: number;
  containerH: number;
  naturalW: number;
  naturalH: number;
}

/** One axis of the cursor-anchored zoom. Positions are in scroll-axis units. */
function anchorAxis(
  container: number,
  natural: number,
  scale: number,
  nextScale: number,
  scroll: number,
  pointer: number,
): number {
  // Where the image starts inside the wrapper: flex-centered when smaller
  // than the container, flush at 0 once it overflows.
  const offset = Math.max(0, (container - natural * scale) / 2);
  // The image-space point (natural pixels) currently under the pointer.
  const point = (scroll + pointer - offset) / scale;
  const nextScaled = natural * nextScale;
  const nextOffset = Math.max(0, (container - nextScaled) / 2);
  // Scroll so that point lands back under the pointer, clamped to what the
  // scroll container will actually accept.
  const target = point * nextScale + nextOffset - pointer;
  return Math.min(Math.max(0, target), Math.max(0, nextScaled - container));
}

/**
 * Scroll offsets that keep the image point under `(pointerX, pointerY)`
 * stationary across a scale change — the wheel-zoom anchor.
 *
 * Pointer coordinates are relative to the scroll container's top-left (client
 * point minus bounding rect). Returns clamped offsets to apply AFTER the new
 * scale has been rendered.
 */
export function zoomAnchoredScroll(
  geom: ViewGeometry,
  scale: number,
  nextScale: number,
  scroll: ScrollPos,
  pointerX: number,
  pointerY: number,
): ScrollPos {
  return {
    scrollLeft: anchorAxis(geom.containerW, geom.naturalW, scale, nextScale, scroll.scrollLeft, pointerX),
    scrollTop: anchorAxis(geom.containerH, geom.naturalH, scale, nextScale, scroll.scrollTop, pointerY),
  };
}

/** Whether the scaled image overflows the pane on either axis (⇒ pannable). */
export function isPannable(geom: ViewGeometry, scale: number): boolean {
  return geom.naturalW * scale > geom.containerW || geom.naturalH * scale > geom.containerH;
}
