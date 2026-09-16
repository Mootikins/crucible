import { describe, it, expect } from 'vitest';
import { flyoutRect, FLYOUT_MIN, FLYOUT_MARGIN, FLYOUT_HEIGHT_FRACTION } from '@/windowing/reveal/flyoutRect';

const viewport = { width: 1280, height: 800 };
const halfHeight = viewport.height * FLYOUT_HEIGHT_FRACTION;

describe('flyoutRect', () => {
  it('defaults to half the viewport height, top-aligned with the anchor', () => {
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 100, width: 40, height: 32 }, viewport, width: 280 });
    expect(r).toEqual({ x: 40, y: 100, width: 280, height: halfHeight });
  });

  it('shifts up when the bottom would leave the viewport', () => {
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 700, width: 40, height: 32 }, viewport, width: 280 });
    expect(r.y + r.height).toBe(viewport.height - FLYOUT_MARGIN);
    expect(r.height).toBe(halfHeight);
  });

  it('opens to the left of a right-rail anchor, and shifts up near the bottom', () => {
    const r = flyoutRect({ position: 'right', anchor: { x: 1240, y: 780, width: 40, height: 32 }, viewport, width: 280 });
    expect(r).toEqual({ x: 1240 - 280, y: viewport.height - FLYOUT_MARGIN - halfHeight, width: 280, height: halfHeight });
  });

  it('moves down to the top margin when the anchor is above it', () => {
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 3, width: 40, height: 32 }, viewport, width: 280 });
    expect(r).toEqual({ x: 40, y: FLYOUT_MARGIN, width: 280, height: halfHeight });
  });

  it('keeps half the height in a short viewport when that half fits', () => {
    const shortViewport = { width: 1280, height: 300 };
    const r = flyoutRect({ position: 'right', anchor: { x: 1240, y: 0, width: 40, height: 32 }, viewport: shortViewport, width: 280 });
    expect(r).toEqual({ x: 1240 - 280, y: FLYOUT_MARGIN, width: 280, height: shortViewport.height * FLYOUT_HEIGHT_FRACTION });
  });

  it('never goes under the minimum while the room allows it', () => {
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 0, width: 40, height: 32 }, viewport: { width: 400, height: 120 }, width: 50 });
    expect(r.width).toBe(FLYOUT_MIN);
    expect(r.height).toBe(FLYOUT_MIN);
    expect(r.y).toBe(FLYOUT_MARGIN);
  });

  it('shrinks under the minimum to fit when the room is smaller', () => {
    const shortViewport = { width: 1280, height: 100 };
    const r = flyoutRect({ position: 'right', anchor: { x: 1240, y: 0, width: 40, height: 32 }, viewport: shortViewport, width: 280 });
    expect(r).toEqual({ x: 1240 - 280, y: FLYOUT_MARGIN, width: 280, height: shortViewport.height - 2 * FLYOUT_MARGIN });
  });

  it('moves right to the left margin when a right-rail flyout would leave the viewport', () => {
    const r = flyoutRect({ position: 'right', anchor: { x: 100, y: 100, width: 40, height: 32 }, viewport, width: 280 });
    expect(r).toEqual({ x: FLYOUT_MARGIN, y: 100, width: 280, height: halfHeight });
  });

  it('shrinks the width to the room of a narrow viewport, and keeps it inside', () => {
    const narrowViewport = { width: 400, height: 800 };
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 100, width: 40, height: 32 }, viewport: narrowViewport, width: 600 });
    expect(r.width).toBe(narrowViewport.width - 2 * FLYOUT_MARGIN);
    expect(r.x).toBe(FLYOUT_MARGIN);
  });
});
