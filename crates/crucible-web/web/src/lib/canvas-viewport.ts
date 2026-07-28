/**
 * Viewport maths for the canvas: pan/zoom, virtualization, and level of detail.
 *
 * Kept free of Solid and the DOM so the parts most likely to be wrong — the
 * ones involving coordinate transforms — are testable without a browser.
 *
 * # Why virtualization and LOD are here from the start
 *
 * Obsidian renders canvas nodes as DOM and hits two well-known walls. Nodes
 * mount and unmount as they cross the viewport edge, which makes panning
 * stutter — the community fix is a CSS snippet that simply enlarges the
 * wrapper so the churn happens off-screen. And it swaps most node types for a
 * lightweight preview when zoomed out, but exempts media, which is exactly why
 * a zoomed-out canvas full of images crawls.
 *
 * So: a generous overscan margin rather than a tight one, and LOD applied to
 * every node type including media.
 */

import type { CanvasNode, Rect } from './canvas-types';
import { nodeRect, rectsIntersect } from './canvas-types';

export interface Viewport {
  /** Canvas-space coordinate at the top-left of the visible area. */
  x: number;
  y: number;
  /** Pixels per canvas unit. */
  zoom: number;
  /** Visible size in screen pixels. */
  width: number;
  height: number;
}

export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 4;

/**
 * How far beyond the visible area nodes stay mounted, as a fraction of
 * viewport size.
 *
 * Half a screen in each direction. Tighter than this and a slow pan remounts
 * cards constantly; much looser and a dense canvas mounts far more DOM than it
 * needs. This is the knob Obsidian users reach for a CSS hack to widen.
 */
export const OVERSCAN = 0.5;

/**
 * Below this zoom, node content is replaced by a lightweight placeholder.
 *
 * At a third of natural size body text is illegible anyway, so rendering a
 * live editor, an image decode, or a markdown tree per card buys nothing.
 */
export const LOD_THRESHOLD = 0.35;

export function clampZoom(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}

/** Screen point (relative to the viewport element) → canvas space. */
export function screenToCanvas(
  viewport: Viewport,
  screenX: number,
  screenY: number,
): { x: number; y: number } {
  return {
    x: viewport.x + screenX / viewport.zoom,
    y: viewport.y + screenY / viewport.zoom,
  };
}

/** Canvas point → screen point relative to the viewport element. */
export function canvasToScreen(
  viewport: Viewport,
  canvasX: number,
  canvasY: number,
): { x: number; y: number } {
  return {
    x: (canvasX - viewport.x) * viewport.zoom,
    y: (canvasY - viewport.y) * viewport.zoom,
  };
}

/** The canvas-space rectangle currently visible. */
export function visibleRect(viewport: Viewport): Rect {
  return {
    x: viewport.x,
    y: viewport.y,
    width: viewport.width / viewport.zoom,
    height: viewport.height / viewport.zoom,
  };
}

/** The visible rectangle grown by [`OVERSCAN`] — the mount set's bounds. */
export function overscanRect(viewport: Viewport): Rect {
  const visible = visibleRect(viewport);
  const padX = visible.width * OVERSCAN;
  const padY = visible.height * OVERSCAN;
  return {
    x: visible.x - padX,
    y: visible.y - padY,
    width: visible.width + padX * 2,
    height: visible.height + padY * 2,
  };
}

/**
 * Nodes that should be in the DOM for this viewport.
 *
 * Document order is preserved because in JSON Canvas that order *is* the
 * z-order; filtering must never reshuffle it.
 */
export function visibleNodes(nodes: CanvasNode[], viewport: Viewport): CanvasNode[] {
  // Virtualization needs a viewport to cull against. Before the panel has been
  // measured — first paint, a collapsed split, a hidden tab — culling against a
  // zero rectangle would hide every node, and the canvas would come up blank
  // and stay blank if no resize ever arrived. Mounting everything is the safe
  // direction: correct, just briefly unoptimised.
  if (viewport.width <= 0 || viewport.height <= 0) return nodes;

  const bounds = overscanRect(viewport);
  return nodes.filter((node) => rectsIntersect(nodeRect(node), bounds));
}

/** Whether nodes should render as placeholders rather than full content. */
export function isLowDetail(viewport: Viewport): boolean {
  return viewport.zoom < LOD_THRESHOLD;
}

/**
 * Zoom about a fixed screen point, so the canvas position under the cursor
 * stays under the cursor.
 */
export function zoomAt(
  viewport: Viewport,
  screenX: number,
  screenY: number,
  factor: number,
): Viewport {
  const zoom = clampZoom(viewport.zoom * factor);
  if (zoom === viewport.zoom) return viewport;

  const anchor = screenToCanvas(viewport, screenX, screenY);
  return {
    ...viewport,
    zoom,
    x: anchor.x - screenX / zoom,
    y: anchor.y - screenY / zoom,
  };
}

/** Pan by a screen-space delta. */
export function panBy(viewport: Viewport, dxScreen: number, dyScreen: number): Viewport {
  return {
    ...viewport,
    x: viewport.x - dxScreen / viewport.zoom,
    y: viewport.y - dyScreen / viewport.zoom,
  };
}

/**
 * Fit `bounds` into the viewport with padding — the `Shift+1` / `Shift+2`
 * framing commands.
 */
export function frameRect(viewport: Viewport, bounds: Rect, padding = 80): Viewport {
  if (bounds.width <= 0 || bounds.height <= 0) return viewport;

  // A viewport too small to hold the padding yields a negative scale, which
  // clamps to MIN_ZOOM and leaves the canvas apparently empty (every node
  // renders as a low-detail placeholder). That is not hypothetical: a panel
  // mounted in a collapsed split or a background tab measures zero, and would
  // stay stuck at minimum zoom after becoming visible. Framing is meaningless
  // without a viewport, so decline rather than produce a wrong answer.
  const availableWidth = viewport.width - padding * 2;
  const availableHeight = viewport.height - padding * 2;
  if (availableWidth <= 0 || availableHeight <= 0) return viewport;

  const zoom = clampZoom(Math.min(availableWidth / bounds.width, availableHeight / bounds.height));

  return {
    ...viewport,
    zoom,
    x: bounds.x + bounds.width / 2 - viewport.width / (2 * zoom),
    y: bounds.y + bounds.height / 2 - viewport.height / (2 * zoom),
  };
}

/**
 * Zoom ⇄ slider position, mapped logarithmically.
 *
 * A linear slider over [MIN_ZOOM, MAX_ZOOM] is unusable: 100% lands at 24% of
 * the track and the entire 5%–100% range — where most of the useful travel is —
 * is squeezed into the first quarter. Equal slider distances should mean equal
 * *ratios* of zoom, which is what makes dragging feel even.
 */
export function zoomToSlider(zoom: number): number {
  const t = Math.log(clampZoom(zoom) / MIN_ZOOM) / Math.log(MAX_ZOOM / MIN_ZOOM);
  return Math.min(1, Math.max(0, t));
}

export function sliderToZoom(t: number): number {
  const clamped = Math.min(1, Math.max(0, t));
  return clampZoom(MIN_ZOOM * Math.pow(MAX_ZOOM / MIN_ZOOM, clamped));
}

/**
 * How close to the 100% mark counts as "on it", in slider units.
 *
 * The one zoom worth hitting exactly is 1:1 — it is the only value where a
 * card is its real size — and on a logarithmic track it is a single position
 * among thousands, so dragging to precisely 100% is otherwise luck.
 */
export const ZOOM_DETENT_TOLERANCE = 0.025;

/** Slider position → zoom, pulled to exactly 100% near the detent. */
export function sliderToZoomWithDetent(t: number): number {
  const clamped = Math.min(1, Math.max(0, t));
  return Math.abs(clamped - zoomToSlider(1)) <= ZOOM_DETENT_TOLERANCE
    ? 1
    : sliderToZoom(clamped);
}

/** Zoom about the centre of the viewport, keeping what is centred centred. */
export function zoomToLevel(viewport: Viewport, zoom: number): Viewport {
  const target = clampZoom(zoom);
  if (target === viewport.zoom) return viewport;
  return zoomAt(viewport, viewport.width / 2, viewport.height / 2, target / viewport.zoom);
}

/** Recentre the viewport on a canvas-space point, without changing zoom. */
export function centreOn(viewport: Viewport, point: { x: number; y: number }): Viewport {
  return {
    ...viewport,
    x: point.x - viewport.width / (2 * viewport.zoom),
    y: point.y - viewport.height / (2 * viewport.zoom),
  };
}

/**
 * Eased progress through a tween, as a 0..1 fraction.
 *
 * The clamp at ZERO is load-bearing, not defensive. A rAF callback is handed
 * the timestamp of the frame it belongs to, and that frame may have started
 * BEFORE the handler that scheduled the callback ran — so `elapsed` is
 * routinely negative on the first frame. Easing a negative fraction through
 * `1-(1-t)³` yields a negative result, which interpolates the viewport
 * backwards: zooming in visibly flashed *out* (100% → 62%) before climbing.
 */
export function tweenProgress(elapsedMs: number, durationMs: number): number {
  if (durationMs <= 0) return 1;
  const t = Math.min(1, Math.max(0, elapsedMs / durationMs));
  // Ease-out cubic: quick off the mark, settling rather than stopping dead.
  return 1 - Math.pow(1 - t, 3);
}

/** Snap a canvas coordinate to the grid, unless snapping is suppressed. */
export const GRID = 20;
export function snap(value: number, enabled: boolean): number {
  return enabled ? Math.round(value / GRID) * GRID : Math.round(value);
}
