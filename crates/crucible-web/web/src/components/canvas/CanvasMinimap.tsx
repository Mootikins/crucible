import { Component, For, createMemo } from 'solid-js';
import { boundsOf, nodeRect, resolveCanvasColor, type CanvasNode } from '@/lib/canvas-types';
import { visibleRect, type Viewport } from '@/lib/canvas-viewport';

/**
 * A whole-canvas overview with the current viewport drawn on it.
 *
 * The zoom readout answers "how far in am I"; on a canvas larger than the
 * screen the harder question is "where am I", which no amount of zoom control
 * answers. Clicking recentres, so it doubles as coarse navigation.
 */
export const CanvasMinimap: Component<{
  nodes: CanvasNode[];
  viewport: Viewport;
  /** Drag the canvas by a delta in canvas units — grab, not jump-to. */
  onPan: (delta: { x: number; y: number }) => void;
  width?: number;
  height?: number;
}> = (props) => {
  const width = () => props.width ?? 208;
  const height = () => props.height ?? 116;

  /**
   * The area the map covers: everything drawn, plus wherever the viewport
   * currently is.
   *
   * Unioning in the viewport matters — panned off into empty space, a map of
   * the nodes alone would show the viewport rectangle outside its own frame,
   * or clipped away entirely, with no clue which direction home was.
   */
  const extent = createMemo(() => {
    const view = visibleRect(props.viewport);
    const nodes = boundsOf(props.nodes);
    const rects = nodes ? [nodes, view] : [view];

    const minX = Math.min(...rects.map((r) => r.x));
    const minY = Math.min(...rects.map((r) => r.y));
    const maxX = Math.max(...rects.map((r) => r.x + r.width));
    const maxY = Math.max(...rects.map((r) => r.y + r.height));

    // A degenerate extent (no nodes, zero-sized viewport) would divide by zero
    // in `scale` and put every rect at NaN, which renders as nothing at all.
    const w = Math.max(maxX - minX, 1);
    const h = Math.max(maxY - minY, 1);
    const pad = Math.max(w, h) * 0.05;
    return { x: minX - pad, y: minY - pad, width: w + pad * 2, height: h + pad * 2 };
  });

  // One uniform scale, so the map is not a distorted picture of the canvas.
  const scale = createMemo(() =>
    Math.min(width() / extent().width, height() / extent().height),
  );

  const project = (x: number, y: number) => ({
    x: (x - extent().x) * scale(),
    y: (y - extent().y) * scale(),
  });

  const viewBox = createMemo(() => {
    const v = visibleRect(props.viewport);
    const origin = project(v.x, v.y);
    return {
      x: origin.x,
      y: origin.y,
      width: v.width * scale(),
      height: v.height * scale(),
    };
  });

  /**
   * Drag moves the CANVAS, not the viewport rectangle.
   *
   * Both readings are defensible — the map is showing you a viewport box, so
   * dragging it is one obvious meaning — but the canvas is the thing being
   * looked at, and grabbing it is the same gesture as dragging the surface
   * itself. Dragging the box instead inverts every motion relative to the
   * canvas underneath, which reads as backwards.
   *
   * Deltas rather than absolute positions: jumping the view so the grabbed
   * point lands under the cursor would lurch on the first pixel of movement.
   * Pointer capture keeps the gesture alive outside the little map — releasing
   * out there is otherwise a stuck drag.
   */
  let last: { x: number; y: number } | null = null;

  const onPointerDown = (e: PointerEvent & { currentTarget: SVGSVGElement }) => {
    // Left button only: a right-click here should not fling the canvas.
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    last = { x: e.clientX, y: e.clientY };
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      /* capture unavailable (jsdom); the move handler still tracks */
    }
  };

  const onPointerMove = (e: PointerEvent & { currentTarget: SVGSVGElement }) => {
    if (!last) return;
    e.preventDefault();
    props.onPan({
      x: (e.clientX - last.x) / scale(),
      y: (e.clientY - last.y) / scale(),
    });
    last = { x: e.clientX, y: e.clientY };
  };

  const endDrag = (e: PointerEvent & { currentTarget: SVGSVGElement }) => {
    if (!last) return;
    last = null;
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {
      /* nothing captured */
    }
  };

  return (
    <svg
      class="w-full cursor-grab touch-none rounded border border-hairline bg-surface-sunken active:cursor-grabbing"
      width={width()}
      height={height()}
      data-testid="canvas-minimap"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      // Touch and pen send pointercancel instead of pointerup when the gesture
      // is interrupted; without this the drag sticks.
      onPointerCancel={endDrag}
    >
      <For each={props.nodes}>
        {(node) => {
          const r = () => nodeRect(node);
          const origin = () => project(r().x, r().y);
          const accent = () => resolveCanvasColor(node.color);
          return (
            <rect
              x={origin().x}
              y={origin().y}
              width={Math.max(r().width * scale(), 1)}
              height={Math.max(r().height * scale(), 1)}
              rx={1}
              fill={accent() ?? 'var(--color-muted-dark)'}
              opacity={node.type === 'group' ? 0.25 : 0.7}
            />
          );
        }}
      </For>

      <rect
        x={viewBox().x}
        y={viewBox().y}
        width={Math.max(viewBox().width, 2)}
        height={Math.max(viewBox().height, 2)}
        fill="var(--color-primary)"
        fill-opacity="0.12"
        stroke="var(--color-primary)"
        stroke-width="1"
        data-testid="canvas-minimap-viewport"
      />
    </svg>
  );
};
