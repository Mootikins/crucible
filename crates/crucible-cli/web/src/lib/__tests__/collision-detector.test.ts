import { describe, it, expect } from 'vitest';
import { pickDroppable, type DroppableLike } from '../collision-detector';

const drop = (id: string, x: number, y: number, w: number, h: number): DroppableLike => ({
  id,
  layout: { x, y, width: w, height: h },
});

// A pane (800x600) with a left split zone (20% strip) — the real geometry
// from Pane.tsx. The dragged tab element is ~150x30.
const CENTER = drop('pane:p1:center', 0, 0, 800, 600);
const LEFT_ZONE = drop('pane:p1:left', 0, 0, 160, 600);
const dragRect = (x: number, y: number) => ({ x, y, width: 150, height: 30 });

describe('pickDroppable — pointer-first drop target selection', () => {
  it('picks the zone the pointer is inside, even when the element also overlaps a bigger target', () => {
    const pointer = { x: 80, y: 300 }; // inside the left zone
    const picked = pickDroppable(dragRect(10, 285), [CENTER, LEFT_ZONE], pointer, null);
    expect(picked?.id).toBe('pane:p1:left');
  });

  it('does NOT pick a zone the element merely grazes when the pointer is in the center', () => {
    // Pointer well inside the center; the wide tab element still overlaps the
    // left zone's right edge. Rect-based selection used to pick the zone.
    const pointer = { x: 300, y: 300 };
    const picked = pickDroppable(dragRect(155, 285), [CENTER, LEFT_ZONE], pointer, null);
    expect(picked?.id).toBe('pane:p1:center');
  });

  it('among pointer-containing targets, the smallest wins', () => {
    const pointer = { x: 80, y: 300 }; // inside both center and left zone
    const picked = pickDroppable(dragRect(70, 290), [CENTER, LEFT_ZONE], pointer, null);
    expect(picked?.id).toBe('pane:p1:left');
  });

  it('falls back to smallest rect intersection when the pointer is in no droppable', () => {
    const pointer = { x: 2000, y: 2000 };
    const picked = pickDroppable(dragRect(100, 590), [CENTER, LEFT_ZONE], pointer, null);
    expect(picked?.id).toBe('pane:p1:left');
  });

  it('falls back to rect intersection when no pointer is known', () => {
    const picked = pickDroppable(dragRect(300, 300), [CENTER, LEFT_ZONE], null, null);
    expect(picked?.id).toBe('pane:p1:center');
  });

  it('returns null when nothing intersects', () => {
    const picked = pickDroppable(dragRect(3000, 3000), [CENTER, LEFT_ZONE], null, null);
    expect(picked).toBeNull();
  });

  it('same-size ties stick with the active droppable', () => {
    const a = drop('a', 0, 0, 100, 100);
    const b = drop('b', 50, 0, 100, 100);
    const pointer = { x: 75, y: 50 }; // inside both, equal areas
    const picked = pickDroppable(dragRect(60, 40), [a, b], pointer, 'b');
    expect(picked?.id).toBe('b');
  });
});
