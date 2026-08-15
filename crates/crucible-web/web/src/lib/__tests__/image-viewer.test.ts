import { describe, expect, it } from 'vitest';
import {
  MAX_SCALE,
  MIN_SCALE,
  SCALE_STEP,
  clampScale,
  fitScale,
  isPannable,
  wheelScaleFactor,
  zoomAnchoredScroll,
  type ViewGeometry,
} from '../image-viewer';

const geom = (over: Partial<ViewGeometry> = {}): ViewGeometry => ({
  containerW: 800,
  containerH: 600,
  naturalW: 2000,
  naturalH: 1000,
  ...over,
});

describe('scale bounds', () => {
  it('clamps to 10%–1000%', () => {
    expect(clampScale(0.01)).toBe(MIN_SCALE);
    expect(clampScale(50)).toBe(MAX_SCALE);
    expect(clampScale(1)).toBe(1);
  });

  it('step curve stays inside bounds under repeated application', () => {
    let s = 1;
    for (let i = 0; i < 100; i++) s = clampScale(s * SCALE_STEP);
    expect(s).toBe(MAX_SCALE);
    for (let i = 0; i < 100; i++) s = clampScale(s / SCALE_STEP);
    expect(s).toBe(MIN_SCALE);
  });
});

describe('wheelScaleFactor', () => {
  it('zooms in on wheel-up, out on wheel-down, symmetrically', () => {
    expect(wheelScaleFactor(-100)).toBeGreaterThan(1);
    expect(wheelScaleFactor(100)).toBeLessThan(1);
    // Notches compose: opposite notches cancel exactly.
    expect(wheelScaleFactor(-100) * wheelScaleFactor(100)).toBeCloseTo(1);
    // Two small deltas equal one big one — trackpads and wheels agree.
    expect(wheelScaleFactor(-50) * wheelScaleFactor(-50)).toBeCloseTo(wheelScaleFactor(-100));
  });

  it('does nothing for a zero delta', () => {
    expect(wheelScaleFactor(0)).toBe(1);
  });
});

describe('fitScale', () => {
  it('contains the image on the tighter axis', () => {
    // 2000×1000 into 800×600: width is the tighter fit → 0.4.
    expect(fitScale(800, 600, 2000, 1000)).toBeCloseTo(0.4);
    // 1000×2000 into 800×600: height governs → 0.3.
    expect(fitScale(800, 600, 1000, 2000)).toBeCloseTo(0.3);
  });

  it('never upscales a small image past 100%', () => {
    expect(fitScale(800, 600, 16, 16)).toBe(1);
  });

  it('falls back to 100% while either box is unmeasured', () => {
    expect(fitScale(0, 0, 2000, 1000)).toBe(1);
    expect(fitScale(800, 600, 0, 0)).toBe(1);
  });

  it('respects the lower scale bound for enormous images', () => {
    expect(fitScale(100, 100, 1_000_000, 1_000_000)).toBe(MIN_SCALE);
  });
});

describe('zoomAnchoredScroll', () => {
  /** Screen-x of an image-space point given scale + scroll, mirroring the
   * viewer's layout (flex-centered under container size, else scrolled). */
  const screenX = (g: ViewGeometry, scale: number, scrollLeft: number, imageX: number) => {
    const offset = Math.max(0, (g.containerW - g.naturalW * scale) / 2);
    return imageX * scale + offset - scrollLeft;
  };

  it('keeps the image point under the cursor stationary across a zoom', () => {
    const g = geom();
    const scale = 1; // 2000×1000 at 100% overflows 800×600
    const scroll = { scrollLeft: 300, scrollTop: 100 };
    const pointer = { x: 250, y: 400 };
    // Image point currently under the pointer.
    const imageX = (scroll.scrollLeft + pointer.x) / scale;

    const next = 2;
    const after = zoomAnchoredScroll(g, scale, next, scroll, pointer.x, pointer.y);
    expect(screenX(g, next, after.scrollLeft, imageX)).toBeCloseTo(pointer.x);
  });

  it('accounts for flex-centering when zooming up from a fitted image', () => {
    const g = geom({ naturalW: 400, naturalH: 300 });
    // 400×300 at 100% inside 800×600 → centered with a 200px left gutter.
    const scale = 1;
    const scroll = { scrollLeft: 0, scrollTop: 0 };
    // Cursor on the image's horizontal center (screen 400 = image x 200).
    const after = zoomAnchoredScroll(g, scale, 4, scroll, 400, 300);
    // At ×4 the image is 1600 wide, no gutter; image x 200 → 800 - pointer 400.
    expect(after.scrollLeft).toBeCloseTo(400);
    expect(after.scrollTop).toBeCloseTo(300);
  });

  it('clamps scroll to the scrollable range', () => {
    const g = geom();
    // Zooming OUT near the far corner: the naive anchor target exceeds the
    // shrunken scroll range and must clamp to it.
    const after = zoomAnchoredScroll(
      g,
      1,
      0.5,
      { scrollLeft: 1200, scrollTop: 400 },
      800,
      600,
    );
    expect(after.scrollLeft).toBeLessThanOrEqual(Math.max(0, g.naturalW * 0.5 - g.containerW));
    expect(after.scrollTop).toBeLessThanOrEqual(Math.max(0, g.naturalH * 0.5 - g.containerH));
    expect(after.scrollLeft).toBeGreaterThanOrEqual(0);
    expect(after.scrollTop).toBeGreaterThanOrEqual(0);
  });

  it('lands at zero scroll when the target scale fits the pane', () => {
    const g = geom();
    // 2000×1000 at 0.25 → 500×250, smaller than 800×600 on both axes.
    const after = zoomAnchoredScroll(g, 1, 0.25, { scrollLeft: 600, scrollTop: 200 }, 100, 100);
    expect(after).toEqual({ scrollLeft: 0, scrollTop: 0 });
  });

  it('is the identity when the scale does not change', () => {
    const g = geom();
    const scroll = { scrollLeft: 350, scrollTop: 120 };
    expect(zoomAnchoredScroll(g, 1, 1, scroll, 123, 456)).toEqual(scroll);
  });
});

describe('isPannable', () => {
  it('is pannable only once the scaled image overflows the pane', () => {
    const g = geom();
    expect(isPannable(g, 0.4)).toBe(false); // exactly fit
    expect(isPannable(g, 0.41)).toBe(true); // overflows width
    expect(isPannable(geom({ naturalW: 100, naturalH: 100 }), 1)).toBe(false);
    expect(isPannable(geom({ naturalW: 100, naturalH: 100 }), 8)).toBe(true);
  });
});
