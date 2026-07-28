import { describe, expect, it } from 'vitest';
import {
  LOD_THRESHOLD,
  MAX_ZOOM,
  MIN_ZOOM,
  canvasToScreen,
  clampZoom,
  frameRect,
  isLowDetail,
  panBy,
  screenToCanvas,
  sliderToZoom,
  sliderToZoomWithDetent,
  tweenProgress,
  ZOOM_DETENT_TOLERANCE,
  snap,
  visibleNodes,
  zoomAt,
  zoomToLevel,
  zoomToSlider,
  centreOn,
  type Viewport,
} from '../canvas-viewport';
import {
  anchorPoint,
  boundsOf,
  groupMembers,
  inferSide,
  resolveCanvasColor,
  fromEndOf,
  toEndOf,
  type CanvasDoc,
  type CanvasNode,
  type GroupNode,
} from '../canvas-types';

const viewport = (over: Partial<Viewport> = {}): Viewport => ({
  x: 0,
  y: 0,
  zoom: 1,
  width: 1000,
  height: 800,
  ...over,
});

const node = (id: string, x: number, y: number, w = 100, h = 100): CanvasNode => ({
  id,
  type: 'text',
  text: id,
  x,
  y,
  width: w,
  height: h,
});

describe('coordinate transforms', () => {
  it('round-trips screen and canvas space at any zoom and pan', () => {
    for (const vp of [viewport(), viewport({ zoom: 2.5, x: -120, y: 340 }), viewport({ zoom: 0.2 })]) {
      const canvas = screenToCanvas(vp, 321, 123);
      const screen = canvasToScreen(vp, canvas.x, canvas.y);
      expect(screen.x).toBeCloseTo(321, 6);
      expect(screen.y).toBeCloseTo(123, 6);
    }
  });

  it('keeps the point under the cursor fixed while zooming', () => {
    const before = viewport({ zoom: 1, x: 50, y: 50 });
    const anchorBefore = screenToCanvas(before, 400, 300);

    const after = zoomAt(before, 400, 300, 1.8);
    const anchorAfter = screenToCanvas(after, 400, 300);

    expect(after.zoom).toBeGreaterThan(before.zoom);
    expect(anchorAfter.x).toBeCloseTo(anchorBefore.x, 6);
    expect(anchorAfter.y).toBeCloseTo(anchorBefore.y, 6);
  });

  it('pans by screen delta scaled into canvas units', () => {
    const panned = panBy(viewport({ zoom: 2 }), 100, 50);
    expect(panned.x).toBe(-50);
    expect(panned.y).toBe(-25);
  });

  it('clamps zoom to the supported range', () => {
    expect(clampZoom(1e6)).toBe(MAX_ZOOM);
    expect(clampZoom(1e-6)).toBe(MIN_ZOOM);
  });

  it('does not move the viewport when a zoom is clamped away', () => {
    const atMax = viewport({ zoom: MAX_ZOOM });
    expect(zoomAt(atMax, 10, 10, 2)).toBe(atMax);
  });
});

describe('virtualization', () => {
  it('mounts nodes inside the viewport', () => {
    const nodes = [node('a', 0, 0), node('b', 500, 400)];
    expect(visibleNodes(nodes, viewport()).map((n) => n.id)).toEqual(['a', 'b']);
  });

  it('drops nodes far outside the overscan margin', () => {
    const nodes = [node('near', 0, 0), node('far', 100_000, 100_000)];
    expect(visibleNodes(nodes, viewport()).map((n) => n.id)).toEqual(['near']);
  });

  /** The remount-churn guard: a node just past the edge stays mounted. */
  it('keeps just-offscreen nodes mounted so panning does not remount them', () => {
    const vp = viewport();
    const justPast = node('edge', vp.width + 50, 0);
    expect(visibleNodes([justPast], vp).map((n) => n.id)).toEqual(['edge']);
  });

  /** Document order is z-order; filtering must not reorder. */
  it('preserves document order', () => {
    const nodes = [node('back', 0, 0), node('mid', 10, 10), node('front', 20, 20)];
    expect(visibleNodes(nodes, viewport()).map((n) => n.id)).toEqual(['back', 'mid', 'front']);
  });

  /**
   * Before measurement — first paint, a collapsed split, a hidden tab — there is
   * nothing to cull against. Culling anyway hid every node and left the canvas
   * blank; mounting everything is briefly unoptimised but never wrong.
   */
  it('mounts everything when the viewport has not been measured yet', () => {
    const nodes = [node('a', 0, 0), node('b', 50_000, 50_000)];
    expect(visibleNodes(nodes, viewport({ width: 0, height: 0 })).map((n) => n.id)).toEqual([
      'a',
      'b',
    ]);
  });

  it('mounts a huge node that straddles the viewport without containing it', () => {
    const huge = node('group', -10_000, -10_000, 30_000, 30_000);
    expect(visibleNodes([huge], viewport())).toHaveLength(1);
  });
});

describe('level of detail', () => {
  it('is full detail at natural zoom and low detail when zoomed far out', () => {
    expect(isLowDetail(viewport({ zoom: 1 }))).toBe(false);
    expect(isLowDetail(viewport({ zoom: LOD_THRESHOLD / 2 }))).toBe(true);
  });

  it('switches exactly at the threshold', () => {
    expect(isLowDetail(viewport({ zoom: LOD_THRESHOLD }))).toBe(false);
    expect(isLowDetail(viewport({ zoom: LOD_THRESHOLD - 0.001 }))).toBe(true);
  });
});

describe('framing', () => {
  it('centres the given bounds', () => {
    const vp = frameRect(viewport(), { x: 0, y: 0, width: 2000, height: 1000 });
    const centreScreen = canvasToScreen(vp, 1000, 500);
    expect(centreScreen.x).toBeCloseTo(500, 4);
    expect(centreScreen.y).toBeCloseTo(400, 4);
  });

  it('fits the whole bounds on screen', () => {
    const bounds = { x: -500, y: -200, width: 4000, height: 300 };
    const vp = frameRect(viewport(), bounds);
    const topLeft = canvasToScreen(vp, bounds.x, bounds.y);
    const bottomRight = canvasToScreen(vp, bounds.x + bounds.width, bounds.y + bounds.height);
    expect(topLeft.x).toBeGreaterThanOrEqual(0);
    expect(bottomRight.x).toBeLessThanOrEqual(vp.width);
  });

  it('leaves the viewport alone for degenerate bounds', () => {
    const vp = viewport();
    expect(frameRect(vp, { x: 0, y: 0, width: 0, height: 0 })).toBe(vp);
  });

  /**
   * A panel in a collapsed split or a background tab measures zero. Framing
   * against that produced a negative scale that clamped to MIN_ZOOM, leaving
   * every node rendered as a low-detail placeholder even after the panel became
   * visible.
   */
  it('declines to frame into a viewport with no usable area', () => {
    const unmeasured = viewport({ width: 0, height: 0 });
    expect(frameRect(unmeasured, { x: 0, y: 0, width: 500, height: 500 })).toBe(unmeasured);

    const tiny = viewport({ width: 40, height: 40 });
    expect(frameRect(tiny, { x: 0, y: 0, width: 500, height: 500 })).toBe(tiny);
  });
});

describe('snapping', () => {
  it('snaps to the grid when enabled and rounds otherwise', () => {
    expect(snap(31, true)).toBe(40);
    expect(snap(31.4, false)).toBe(31);
  });
});

describe('canvas document helpers', () => {
  it('applies asymmetric spec defaults to edge ends', () => {
    const bare = { id: 'e', fromNode: 'a', toNode: 'b' };
    expect(fromEndOf(bare)).toBe('none');
    expect(toEndOf(bare)).toBe('arrow');
  });

  it('maps preset colours to tokens and passes hex through', () => {
    expect(resolveCanvasColor('1')).toContain('--canvas-red');
    expect(resolveCanvasColor('#abcdef')).toBe('#abcdef');
    expect(resolveCanvasColor(undefined)).toBeUndefined();
    expect(resolveCanvasColor('not-a-colour')).toBeUndefined();
  });

  it('anchors edges to the correct side midpoints', () => {
    const n = node('a', 0, 0, 100, 50);
    expect(anchorPoint(n, 'top')).toEqual({ x: 50, y: 0 });
    expect(anchorPoint(n, 'bottom')).toEqual({ x: 50, y: 50 });
    expect(anchorPoint(n, 'left')).toEqual({ x: 0, y: 25 });
    expect(anchorPoint(n, 'right')).toEqual({ x: 100, y: 25 });
  });

  /** Hard-coding right→left makes edges cross their own cards in a column. */
  it('infers the side facing the other node', () => {
    const origin = node('a', 0, 0);
    expect(inferSide(origin, node('b', 500, 0))).toBe('right');
    expect(inferSide(origin, node('b', -500, 0))).toBe('left');
    expect(inferSide(origin, node('b', 0, 500))).toBe('bottom');
    expect(inferSide(origin, node('b', 0, -500))).toBe('top');
  });

  it('computes bounds over several nodes', () => {
    expect(boundsOf([node('a', 0, 0, 100, 100), node('b', 400, 200, 100, 100)])).toEqual({
      x: 0,
      y: 0,
      width: 500,
      height: 300,
    });
    expect(boundsOf([])).toBeNull();
  });

  /**
   * Group membership in JSON Canvas is geometric — there is no child list — so
   * this derivation is what every "drag the group" operation depends on.
   */
  it('derives group membership from containment, not a child list', () => {
    const group: GroupNode = {
      id: 'g',
      type: 'group',
      x: 0,
      y: 0,
      width: 500,
      height: 500,
    };
    const doc: CanvasDoc = {
      nodes: [group, node('inside', 50, 50), node('straddling', 450, 450), node('outside', 900, 900)],
      edges: [],
    };

    expect(groupMembers(doc, group).map((n) => n.id)).toEqual(['inside']);
  });
});

describe('zoom slider mapping', () => {
  it('puts the ends of the track at the zoom limits', () => {
    expect(zoomToSlider(MIN_ZOOM)).toBeCloseTo(0);
    expect(zoomToSlider(MAX_ZOOM)).toBeCloseTo(1);
    expect(sliderToZoom(0)).toBeCloseTo(MIN_ZOOM);
    expect(sliderToZoom(1)).toBeCloseTo(MAX_ZOOM);
  });

  it('round-trips a zoom through the slider position', () => {
    for (const zoom of [0.1, 0.35, 1, 2, 3.5]) {
      expect(sliderToZoom(zoomToSlider(zoom))).toBeCloseTo(zoom, 5);
    }
  });

  /**
   * The mapping is logarithmic on purpose. Linearly, 100% sits at 22% of the
   * track and everything below it — most of the useful travel — is crushed into
   * the first fifth. Equal distances should mean equal RATIOS of zoom.
   */
  it('gives equal slider distances equal zoom ratios', () => {
    const a = sliderToZoom(0.3);
    const b = sliderToZoom(0.5);
    const c = sliderToZoom(0.7);
    expect(b / a).toBeCloseTo(c / b, 5);
  });

  it('clamps out-of-range slider positions instead of escaping the limits', () => {
    expect(sliderToZoom(-1)).toBeCloseTo(MIN_ZOOM);
    expect(sliderToZoom(2)).toBeCloseTo(MAX_ZOOM);
    expect(zoomToSlider(100)).toBeCloseTo(1);
    expect(zoomToSlider(0.0001)).toBeCloseTo(0);
  });
});

describe('zoomToLevel', () => {
  const view = (zoom: number): Viewport => ({ x: 0, y: 0, zoom, width: 1000, height: 600 });

  /** Zooming from a control, unlike the wheel, has no cursor to anchor on. */
  it('keeps the centre of the view fixed', () => {
    const before = view(1);
    const centreBefore = screenToCanvas(before, before.width / 2, before.height / 2);

    const after = zoomToLevel(before, 2);
    const centreAfter = screenToCanvas(after, after.width / 2, after.height / 2);

    expect(after.zoom).toBeCloseTo(2);
    expect(centreAfter.x).toBeCloseTo(centreBefore.x);
    expect(centreAfter.y).toBeCloseTo(centreBefore.y);
  });

  it('respects the zoom limits', () => {
    expect(zoomToLevel(view(1), 99).zoom).toBeCloseTo(MAX_ZOOM);
    expect(zoomToLevel(view(1), 0).zoom).toBeCloseTo(MIN_ZOOM);
  });
});

describe('centreOn', () => {
  it('puts the requested point at the middle of the view, leaving zoom alone', () => {
    const before: Viewport = { x: 0, y: 0, zoom: 2, width: 800, height: 400 };
    const after = centreOn(before, { x: 1000, y: 500 });

    const centre = screenToCanvas(after, after.width / 2, after.height / 2);
    expect(centre.x).toBeCloseTo(1000);
    expect(centre.y).toBeCloseTo(500);
    expect(after.zoom).toBe(before.zoom);
  });
});

describe('zoom detent', () => {
  it('snaps to exactly 100% near the mark', () => {
    const at100 = zoomToSlider(1);
    expect(sliderToZoomWithDetent(at100)).toBe(1);
    expect(sliderToZoomWithDetent(at100 + ZOOM_DETENT_TOLERANCE / 2)).toBe(1);
    expect(sliderToZoomWithDetent(at100 - ZOOM_DETENT_TOLERANCE / 2)).toBe(1);
  });

  /** A detent that captured a wide band would make nearby zooms unreachable. */
  it('leaves zooms outside the detent alone', () => {
    const at100 = zoomToSlider(1);
    const outside = at100 + ZOOM_DETENT_TOLERANCE * 2;
    expect(sliderToZoomWithDetent(outside)).not.toBe(1);
    expect(sliderToZoomWithDetent(outside)).toBeCloseTo(sliderToZoom(outside), 6);
  });

  it('still honours the ends of the track', () => {
    expect(sliderToZoomWithDetent(0)).toBeCloseTo(MIN_ZOOM);
    expect(sliderToZoomWithDetent(1)).toBeCloseTo(MAX_ZOOM);
  });
});

describe('tweenProgress', () => {
  it('runs from 0 to 1 over the duration', () => {
    expect(tweenProgress(0, 100)).toBe(0);
    expect(tweenProgress(100, 100)).toBe(1);
    expect(tweenProgress(50, 100)).toBeGreaterThan(0);
    expect(tweenProgress(50, 100)).toBeLessThan(1);
  });

  it('eases out — more progress in the first half than the second', () => {
    const firstHalf = tweenProgress(50, 100);
    expect(firstHalf).toBeGreaterThan(0.5);
  });

  /**
   * A rAF callback receives the timestamp of the frame it belongs to, and that
   * frame can have started BEFORE the handler that scheduled it ran — so the
   * first frame's elapsed time is routinely negative. Eased unclamped, that
   * interpolates the viewport BACKWARDS: zooming in flashed out to 62% before
   * climbing to 182%.
   */
  it('never reports negative progress, however early the first frame is', () => {
    expect(tweenProgress(-1, 100)).toBe(0);
    expect(tweenProgress(-1000, 100)).toBe(0);
  });

  it('saturates rather than overshooting past the end', () => {
    expect(tweenProgress(10_000, 100)).toBe(1);
  });

  it('treats a zero-length tween as already finished', () => {
    expect(tweenProgress(0, 0)).toBe(1);
  });
});
