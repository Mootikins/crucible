/**
 * Custom collision detector, two-stage:
 *
 * 1. Pointer-first: among droppables CONTAINING the pointer, pick the
 *    smallest. The dragged tab element is ~150px wide, so pure rect
 *    intersection let a 1px graze of a small split zone beat the pane the
 *    pointer was actually in — and the reorder insert indicator already
 *    follows the pointer, so targeting must too or they disagree.
 * 2. Fallback (pointer unknown or outside every droppable): among all
 *    rect-intersecting droppables, prefer the smallest. This lets
 *    directional split zones (~20% of a pane) win over the full-pane
 *    center droppable, with center as the fallback.
 */
import type { CollisionDetector } from '@thisbeyond/solid-dnd';

type Rect = { x: number; y: number; width: number; height: number };
type Point = { x: number; y: number };

export interface DroppableLike {
  id: string | number;
  layout: Rect;
}

// The library hands detectors element layouts, not the pointer — track it
// ourselves. One passive listener; pointerdown seeds the position so a drag
// that starts without prior movement still has it.
let pointer: Point | null = null;
if (typeof document !== 'undefined') {
  const track = (e: PointerEvent) => {
    pointer = { x: e.clientX, y: e.clientY };
  };
  document.addEventListener('pointerdown', track, { passive: true });
  document.addEventListener('pointermove', track, { passive: true });
}

function intersectionArea(a: Rect, b: Rect): number {
  const left = Math.max(a.x, b.x);
  const top = Math.max(a.y, b.y);
  const right = Math.min(a.x + a.width, b.x + b.width);
  const bottom = Math.min(a.y + a.height, b.y + b.height);
  if (left < right && top < bottom) {
    return (right - left) * (bottom - top);
  }
  return 0;
}

function containsPoint(r: Rect, p: Point): boolean {
  return p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height;
}

function smallest<T extends DroppableLike>(
  candidates: T[],
  activeDroppableId: string | number | null,
): T | null {
  if (candidates.length === 0) return null;
  let best = candidates[0];
  let bestArea = best.layout.width * best.layout.height;
  for (let i = 1; i < candidates.length; i++) {
    const c = candidates[i];
    const area = c.layout.width * c.layout.height;
    if (area < bestArea || (area === bestArea && c.id === activeDroppableId)) {
      best = c;
      bestArea = area;
    }
  }
  return best;
}

/** Pure selection core — exported for tests. */
export function pickDroppable<T extends DroppableLike>(
  dragLayout: Rect,
  droppables: readonly T[],
  point: Point | null,
  activeDroppableId: string | number | null,
): T | null {
  if (point) {
    const containing = droppables.filter((d) => containsPoint(d.layout, point));
    const hit = smallest(containing, activeDroppableId);
    if (hit) return hit;
  }
  const intersecting = droppables.filter(
    (d) => intersectionArea(dragLayout, d.layout) > 0,
  );
  return smallest(intersecting, activeDroppableId);
}

export const smallestIntersecting: CollisionDetector = (
  draggable,
  droppables,
  context,
) => {
  return pickDroppable(
    draggable.transformed,
    droppables,
    pointer,
    context.activeDroppableId,
  );
};
