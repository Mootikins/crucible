/**
 * Custom collision detector: among all intersecting droppables, prefer the
 * smallest.  This lets directional split zones (~20% of pane) win over the
 * full-pane center droppable, while the center still acts as fallback.
 */
import type { CollisionDetector } from '@thisbeyond/solid-dnd';

function intersectionArea(
  a: { x: number; y: number; width: number; height: number },
  b: { x: number; y: number; width: number; height: number },
): number {
  const left = Math.max(a.x, b.x);
  const top = Math.max(a.y, b.y);
  const right = Math.min(a.x + a.width, b.x + b.width);
  const bottom = Math.min(a.y + a.height, b.y + b.height);
  if (left < right && top < bottom) {
    return (right - left) * (bottom - top);
  }
  return 0;
}

export const smallestIntersecting: CollisionDetector = (
  draggable,
  droppables,
  context,
) => {
  const dragLayout = draggable.transformed;

  type Hit = { droppable: (typeof droppables)[number]; area: number };
  const hits: Hit[] = [];

  for (const droppable of droppables) {
    const area = intersectionArea(dragLayout, droppable.layout);
    if (area > 0) {
      hits.push({ droppable, area });
    }
  }

  if (hits.length === 0) return null;

  let best: Hit = hits[0];
  let bestArea = best.droppable.layout.width * best.droppable.layout.height;



  for (let i = 1; i < hits.length; i++) {
    const hit = hits[i];
    const hitArea = hit.droppable.layout.width * hit.droppable.layout.height;
    if (
      hitArea < bestArea ||
      (hitArea === bestArea && hit.droppable.id === context.activeDroppableId)
    ) {
      best = hit;
      bestArea = hitArea;
    }
  }

  return best.droppable;
};
