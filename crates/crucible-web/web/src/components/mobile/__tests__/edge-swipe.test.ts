import { describe, it, expect, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { createEdgeSwipe, type SwipePoint } from '@/components/mobile/edge-swipe';

const WIDTH = 300;
const VIEWPORT = 390;

function setup(side: 'left' | 'right', open = false) {
  const onSettle = vi.fn();
  let swipe!: ReturnType<typeof createEdgeSwipe>;
  createRoot(() => {
    swipe = createEdgeSwipe({
      side,
      width: () => WIDTH,
      viewportWidth: () => VIEWPORT,
      isOpen: () => open,
      onSettle,
    });
  });
  return { swipe, onSettle };
}

const at = (x: number, t: number, y = 200, inDrawer = false): SwipePoint => ({ x, y, t, inDrawer });

describe('createEdgeSwipe', () => {
  it('follows the finger from the left edge, then settles open past 40%', () => {
    const { swipe, onSettle } = setup('left');
    swipe.down(at(5, 0));
    swipe.move(at(60, 100));
    swipe.move(at(155, 400));
    expect(swipe.dragPx()).toBe(150);
    swipe.up(at(155, 1000));
    expect(onSettle).toHaveBeenCalledWith(true);
    expect(swipe.dragPx()).toBeNull();
  });

  it('settles closed when released short of 40%, slowly', () => {
    const { swipe, onSettle } = setup('left');
    swipe.down(at(5, 0));
    swipe.move(at(50, 500));
    swipe.up(at(50, 1500));
    expect(onSettle).toHaveBeenCalledWith(false);
  });

  it('ignores a touch that starts outside the edge zone', () => {
    const { swipe, onSettle } = setup('left');
    swipe.down(at(100, 0));
    swipe.move(at(250, 50));
    swipe.up(at(250, 60));
    expect(swipe.dragPx()).toBeNull();
    expect(onSettle).not.toHaveBeenCalled();
  });

  // A vertical scroll that happens to start at the edge must stay a scroll.
  it('lets a mostly vertical drag go, so the content still scrolls', () => {
    const { swipe, onSettle } = setup('left');
    swipe.down(at(5, 0, 100));
    swipe.move(at(15, 50, 300));
    swipe.up(at(15, 60, 320));
    expect(swipe.dragPx()).toBeNull();
    expect(onSettle).not.toHaveBeenCalled();
  });

  it('opens the right drawer from the right edge, moving left', () => {
    const { swipe, onSettle } = setup('right');
    swipe.down(at(386, 0));
    swipe.move(at(300, 100));
    swipe.move(at(200, 400));
    expect(swipe.dragPx()).toBe(186);
    swipe.up(at(200, 1000));
    expect(onSettle).toHaveBeenCalledWith(true);
  });

  it('drags an open drawer closed from inside it, and a flick wins', () => {
    const { swipe, onSettle } = setup('left', true);
    swipe.down(at(250, 0, 200, true));
    swipe.move(at(230, 10, 200, true));
    swipe.move(at(200, 20, 200, true));
    expect(swipe.dragPx()).toBe(WIDTH - 50);
    swipe.up(at(190, 30, 200, true));
    expect(onSettle).toHaveBeenCalledWith(false);
  });

  it('does not start a close drag from outside an open drawer', () => {
    const { swipe, onSettle } = setup('left', true);
    swipe.down(at(250, 0, 200, false));
    swipe.move(at(150, 20, 200, false));
    swipe.up(at(150, 30, 200, false));
    expect(swipe.dragPx()).toBeNull();
    expect(onSettle).not.toHaveBeenCalled();
  });

  it('never drags past fully open or fully closed', () => {
    const { swipe } = setup('left');
    swipe.down(at(5, 0));
    swipe.move(at(40, 50));
    swipe.move(at(900, 100));
    expect(swipe.dragPx()).toBe(WIDTH);
    swipe.move(at(-200, 150));
    expect(swipe.dragPx()).toBe(0);
  });
});
