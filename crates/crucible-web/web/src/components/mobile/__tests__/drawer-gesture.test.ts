import { describe, it, expect } from 'vitest';
import { EDGE_ZONE_PX, settleDrawer, startsInEdge } from '@/components/mobile/drawer-gesture';

describe('startsInEdge', () => {
  it('claims a touch inside the left edge zone for the left drawer', () => {
    expect(startsInEdge({ x: 4, viewportWidth: 390, side: 'left' })).toBe(true);
  });

  it('ignores a touch outside the left edge zone', () => {
    expect(startsInEdge({ x: EDGE_ZONE_PX + 1, viewportWidth: 390, side: 'left' })).toBe(false);
  });

  it('measures the right edge zone from the right side of the viewport', () => {
    expect(startsInEdge({ x: 386, viewportWidth: 390, side: 'right' })).toBe(true);
    expect(startsInEdge({ x: 390 - EDGE_ZONE_PX - 1, viewportWidth: 390, side: 'right' })).toBe(false);
  });
});

describe('settleDrawer', () => {
  const width = 320;

  it('opens past forty percent of the drawer width', () => {
    expect(settleDrawer({ width, openPx: 0.41 * width, velocity: 0 })).toBe(true);
  });

  it('closes short of forty percent of the drawer width', () => {
    expect(settleDrawer({ width, openPx: 0.39 * width, velocity: 0 })).toBe(false);
  });

  it('lets a fast flick toward open win over a short drag', () => {
    expect(settleDrawer({ width, openPx: 10, velocity: 0.8 })).toBe(true);
  });

  it('lets a fast flick toward closed win over a long drag', () => {
    expect(settleDrawer({ width, openPx: 300, velocity: -0.8 })).toBe(false);
  });

  it('treats a slow release as position-only', () => {
    expect(settleDrawer({ width, openPx: 300, velocity: -0.3 })).toBe(true);
  });
});
