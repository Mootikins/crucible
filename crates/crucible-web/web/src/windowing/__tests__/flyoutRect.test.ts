import { describe, it, expect } from 'vitest';
import { flyoutRect } from '@/windowing/reveal/flyoutRect';

const viewport = { width: 1280, height: 800 };

describe('flyoutRect', () => {
  it('defaults to half the viewport height, top-aligned with the anchor', () => {
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 100, width: 40, height: 32 }, viewport, width: 280 });
    expect(r).toEqual({ x: 40, y: 100, width: 280, height: 400 });
  });

  it('shifts up when the bottom would leave the viewport', () => {
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 700, width: 40, height: 32 }, viewport, width: 280 });
    expect(r.y + r.height).toBe(800 - 8);
    expect(r.height).toBe(400);
  });

  it('opens to the left of a right-rail anchor, and shifts up near the bottom', () => {
    const r = flyoutRect({ position: 'right', anchor: { x: 1240, y: 780, width: 40, height: 32 }, viewport, width: 280 });
    expect(r).toEqual({ x: 1240 - 280, y: 800 - 8 - 400, width: 280, height: 400 });
  });

  it('moves down to the top margin when the anchor is above it', () => {
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 3, width: 40, height: 32 }, viewport, width: 280 });
    expect(r).toEqual({ x: 40, y: 8, width: 280, height: 400 });
  });

  it('keeps half the height in a short viewport when that half fits', () => {
    const r = flyoutRect({ position: 'right', anchor: { x: 1240, y: 0, width: 40, height: 32 }, viewport: { width: 1280, height: 300 }, width: 280 });
    expect(r).toEqual({ x: 1240 - 280, y: 8, width: 280, height: 150 });
  });

  it('never goes under the minimum while the room allows it', () => {
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 0, width: 40, height: 32 }, viewport: { width: 400, height: 120 }, width: 50 });
    expect(r.width).toBe(100);
    expect(r.height).toBe(100);
    expect(r.y).toBe(8);
  });

  it('shrinks under the minimum to fit when the room is smaller', () => {
    const r = flyoutRect({ position: 'right', anchor: { x: 1240, y: 0, width: 40, height: 32 }, viewport: { width: 1280, height: 100 }, width: 280 });
    expect(r).toEqual({ x: 1240 - 280, y: 8, width: 280, height: 100 - 16 });
  });

  it('moves right to the left margin when a right-rail flyout would leave the viewport', () => {
    const r = flyoutRect({ position: 'right', anchor: { x: 100, y: 100, width: 40, height: 32 }, viewport, width: 280 });
    expect(r).toEqual({ x: 8, y: 100, width: 280, height: 400 });
  });

  it('shrinks the width to the room of a narrow viewport, and keeps it inside', () => {
    const r = flyoutRect({ position: 'left', anchor: { x: 0, y: 100, width: 40, height: 32 }, viewport: { width: 400, height: 800 }, width: 600 });
    expect(r.width).toBe(400 - 16);
    expect(r.x).toBe(8);
  });
});
