import { Component, createEffect, createSignal, on, onCleanup, onMount } from 'solid-js';
import { Frame, ZoomIn, ZoomOut } from '@/lib/icons';
import {
  SCALE_STEP,
  clampScale,
  fitScale,
  isPannable,
  wheelScaleFactor,
  zoomAnchoredScroll,
  type ViewGeometry,
} from '@/lib/image-viewer';

interface ImageViewerProps {
  /** Raw-bytes URL (`/api/file/raw?...`). */
  src: string;
  alt: string;
}

/**
 * In-pane image viewer: toolbar (zoom out / % / zoom in / fit / 1:1),
 * wheel-zoom anchored on the cursor, drag-to-pan when the image overflows,
 * and +/−/0/1 on the keyboard while the pane has focus.
 *
 * Plain wheel — no modifier — zooms. This is a dedicated image pane, so the
 * wheel has no scroll job to do (fit never overflows, and beyond fit the
 * drag pans); requiring Ctrl would collide with browser page zoom, which is
 * exactly the UX this component exists to avoid. The event is consumed
 * (preventDefault + stopPropagation) so it never reaches pane scroll.
 *
 * Zoom state is component-local and the component is keyed on `src` by its
 * parent, so a different image always opens at fit.
 */
export const ImageViewer: Component<ImageViewerProps> = (props) => {
  let scroller: HTMLDivElement | undefined;

  // 'fit' tracks the pane through resizes; a number is an explicit scale.
  const [mode, setMode] = createSignal<'fit' | number>('fit');
  const [natural, setNatural] = createSignal({ w: 0, h: 0 });
  const [container, setContainer] = createSignal({ w: 0, h: 0 });
  const [panning, setPanning] = createSignal(false);

  const geometry = (): ViewGeometry => ({
    containerW: container().w,
    containerH: container().h,
    naturalW: natural().w,
    naturalH: natural().h,
  });

  const scale = () => {
    const m = mode();
    return m === 'fit' ? fitScale(container().w, container().h, natural().w, natural().h) : m;
  };

  // A different image in the same panel instance starts over at fit.
  createEffect(on(() => props.src, () => {
    setMode('fit');
    setNatural({ w: 0, h: 0 });
  }, { defer: true }));

  onMount(() => {
    const measure = () => {
      if (scroller) setContainer({ w: scroller.clientWidth, h: scroller.clientHeight });
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (scroller) observer.observe(scroller);
    onCleanup(() => observer.disconnect());
  });

  /**
   * Set an explicit scale, keeping the image point at (pointerX, pointerY) —
   * container-relative — stationary. Solid renders the new size synchronously
   * on the signal write, so the corrected scroll offsets apply right after.
   */
  const zoomTo = (nextScale: number, pointerX?: number, pointerY?: number) => {
    const from = scale();
    const to = clampScale(nextScale);
    const geom = geometry();
    const px = pointerX ?? geom.containerW / 2;
    const py = pointerY ?? geom.containerH / 2;
    const el = scroller;
    const scroll = el
      ? { scrollLeft: el.scrollLeft, scrollTop: el.scrollTop }
      : { scrollLeft: 0, scrollTop: 0 };
    setMode(to);
    if (el) {
      const next = zoomAnchoredScroll(geom, from, to, scroll, px, py);
      el.scrollLeft = next.scrollLeft;
      el.scrollTop = next.scrollTop;
    }
  };

  const zoomBy = (factor: number, pointerX?: number, pointerY?: number) =>
    zoomTo(scale() * factor, pointerX, pointerY);

  const onWheel = (e: WheelEvent) => {
    // The wheel belongs to this pane while the cursor is over the image —
    // never to pane scroll, never to browser zoom.
    e.preventDefault();
    e.stopPropagation();
    if (!scroller) return;
    const rect = scroller.getBoundingClientRect();
    zoomBy(wheelScaleFactor(e.deltaY), e.clientX - rect.left, e.clientY - rect.top);
  };

  const onDblClick = (e: MouseEvent) => {
    if (!scroller) return;
    const rect = scroller.getBoundingClientRect();
    if (mode() === 'fit') {
      // Fit → 100%, anchored where the user pointed so the detail they
      // double-clicked is what ends up in view.
      zoomTo(1, e.clientX - rect.left, e.clientY - rect.top);
    } else {
      setMode('fit');
    }
  };

  // Drag-to-pan: native scroll offsets driven by pointer deltas. Pointer
  // capture keeps the pan alive when the cursor leaves the pane mid-drag.
  let pan: { id: number; startX: number; startY: number; left: number; top: number } | null = null;
  const onPointerDown = (e: PointerEvent) => {
    if (!scroller) return;
    // The keyboard shortcuts live on this element, so clicking must focus it.
    scroller.focus();
    if (e.button !== 0 || !isPannable(geometry(), scale())) return;
    pan = {
      id: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      left: scroller.scrollLeft,
      top: scroller.scrollTop,
    };
    scroller.setPointerCapture?.(e.pointerId);
    setPanning(true);
    e.preventDefault();
  };
  const onPointerMove = (e: PointerEvent) => {
    if (!pan || !scroller || e.pointerId !== pan.id) return;
    scroller.scrollLeft = pan.left - (e.clientX - pan.startX);
    scroller.scrollTop = pan.top - (e.clientY - pan.startY);
  };
  const onPointerUp = (e: PointerEvent) => {
    if (!pan || e.pointerId !== pan.id) return;
    scroller?.releasePointerCapture?.(pan.id);
    pan = null;
    setPanning(false);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    switch (e.key) {
      case '+':
      case '=':
        zoomBy(SCALE_STEP);
        break;
      case '-':
        zoomBy(1 / SCALE_STEP);
        break;
      case '0':
        setMode('fit');
        break;
      case '1':
        zoomTo(1);
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  };

  const toolButton = 'rounded p-1 hover:bg-hover-wash';

  return (
    <div class="h-full min-h-0 flex flex-col" data-testid="image-viewer">
      <div class="flex items-center gap-2 border-b border-hairline px-3 py-1.5 text-xs">
        <span class="truncate font-medium text-shell-ink">{props.alt}</span>
        <div class="ml-auto flex items-center gap-1 text-muted-dark">
          <button
            type="button"
            class={toolButton}
            title="Zoom out (-)"
            aria-label="Zoom out (minus key)"
            data-testid="image-zoom-out"
            onClick={() => zoomBy(1 / SCALE_STEP)}
          >
            <ZoomOut class="h-3.5 w-3.5" />
          </button>
          <span
            class="min-w-[3.5rem] text-center tabular-nums"
            data-testid="image-zoom-level"
          >
            {Math.round(scale() * 100)}%
          </span>
          <button
            type="button"
            class={toolButton}
            title="Zoom in (+)"
            aria-label="Zoom in (plus key)"
            data-testid="image-zoom-in"
            onClick={() => zoomBy(SCALE_STEP)}
          >
            <ZoomIn class="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            class={toolButton}
            title="Fit to pane (0)"
            aria-label="Fit to pane (zero key)"
            data-testid="image-zoom-fit"
            onClick={() => setMode('fit')}
          >
            <Frame class="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            class="rounded px-1.5 py-0.5 tabular-nums hover:bg-hover-wash"
            title="Actual size (1)"
            aria-label="Actual size, 100% (one key)"
            data-testid="image-zoom-actual"
            onClick={() => zoomTo(1)}
          >
            1:1
          </button>
        </div>
      </div>
      <div
        ref={scroller}
        class="relative min-h-0 flex-1 overflow-auto outline-none focus-visible:ring-1 focus-visible:ring-primary/50"
        classList={{
          'cursor-grab': !panning() && isPannable(geometry(), scale()),
          'cursor-grabbing': panning(),
        }}
        tabindex={0}
        data-testid="image-scroller"
        onWheel={onWheel}
        onDblClick={onDblClick}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        // Touch/pen deliver pointercancel INSTEAD of pointerup when the
        // gesture is interrupted; without this the pan state sticks.
        onPointerCancel={onPointerUp}
        onKeyDown={onKeyDown}
      >
        {/* w-max lets the wrapper grow to the scaled image (scrollable); the
            min-w/h keep it pane-sized so a fitted image stays centered. No
            padding: fitScale() measures the scroller's client box, so any
            wrapper padding would push a "fitted" image into overflow. */}
        <div class="flex h-max w-max min-h-full min-w-full items-center justify-center">
          <img
            src={props.src}
            alt={props.alt}
            draggable={false}
            data-testid="file-image"
            class="max-w-none select-none"
            // Unmeasured (still loading / jsdom): fall back to intrinsic
            // sizing so the image is never rendered 0×0.
            style={
              natural().w > 0
                ? {
                    width: `${natural().w * scale()}px`,
                    height: `${natural().h * scale()}px`,
                  }
                : { 'max-width': '100%', 'max-height': '100%' }
            }
            onLoad={(e) =>
              setNatural({
                w: e.currentTarget.naturalWidth,
                h: e.currentTarget.naturalHeight,
              })
            }
          />
        </div>
      </div>
    </div>
  );
};
