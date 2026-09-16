import { Component, createEffect, createSignal, on, onCleanup } from 'solid-js';
import { windowStore, windowActions } from '@/windowing/store';
import type { EdgePanelPosition } from '@/windowing/model/types';
import { isEdgeCollapsed } from '@/windowing/model/types';
import { isRestoringLayout } from '@/windowing/model/layout-restore';
import { SplitPane } from './SplitPane';

const EDGE_PANEL_MIN_WIDTH = 120;
// No fixed max — an edge panel hosting a chat session should be able to take
// most of the viewport; just keep a sliver of center pane usable.
const edgePanelMaxWidth = () => Math.max(600, window.innerWidth - 320);
const EDGE_PANEL_MIN_HEIGHT = 100;
const EDGE_PANEL_MAX_HEIGHT = 500;

function EdgePanelResizeHandle(props: { position: EdgePanelPosition }) {
  const panel = () => windowStore.edgePanels[props.position];
  const isVertical = () =>
    props.position === 'left' || props.position === 'right';
  let cleanup: (() => void) | null = null;

  onCleanup(() => {
    if (cleanup) {
      cleanup();
      cleanup = null;
    }
  });

  const handlePointerDown = (e: PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startY = e.clientY;
    const startSize = isVertical()
      ? panel().width ?? 250
      : panel().height ?? 200;

    const handlePointerMove = (e: PointerEvent) => {
      if (props.position === 'left') {
        const delta = e.clientX - startX;
        windowActions.setEdgePanelSize(
          props.position,
          Math.max(EDGE_PANEL_MIN_WIDTH, Math.min(edgePanelMaxWidth(), startSize + delta))
        );
      } else if (props.position === 'right') {
        const delta = startX - e.clientX;
        windowActions.setEdgePanelSize(
          props.position,
          Math.max(EDGE_PANEL_MIN_WIDTH, Math.min(edgePanelMaxWidth(), startSize + delta))
        );
      } else {
        const delta = startY - e.clientY;
        windowActions.setEdgePanelSize(
          props.position,
          Math.max(EDGE_PANEL_MIN_HEIGHT, Math.min(EDGE_PANEL_MAX_HEIGHT, startSize + delta))
        );
      }
    };

    const handlePointerUp = (e: PointerEvent) => {
      el.releasePointerCapture(e.pointerId);
      document.removeEventListener('pointermove', handlePointerMove);
      document.removeEventListener('pointerup', handlePointerUp);
      cleanup = null;
    };

    document.addEventListener('pointermove', handlePointerMove);
    document.addEventListener('pointerup', handlePointerUp);
    cleanup = () => {
      document.removeEventListener('pointermove', handlePointerMove);
      document.removeEventListener('pointerup', handlePointerUp);
    };
  };

  // 1px visible line; the after: pseudo extends the pointer target ±4px so
  // the thin separator is still comfortable to grab (Obsidian-style).
  return (
    <div
      role="separator"
      aria-orientation={isVertical() ? 'vertical' : 'horizontal'}
      classList={{
        'relative flex-shrink-0 z-10 bg-control hover:bg-hover-wash active:bg-primary transition-colors after:content-[\'\'] after:absolute': true,
        'w-px cursor-col-resize after:inset-y-0 after:-inset-x-1': isVertical(),
        'h-px cursor-row-resize after:inset-x-0 after:-inset-y-1': !isVertical(),
      }}
      on:pointerdown={handlePointerDown}
    />
  );
}

/**
 * The body of a rail: its pane tree and its resize handle, inside the frame
 * that slides the body open and shut.
 *
 * The body stays MOUNTED while the rail is collapsed. See the tween below.
 */
export const DockedBody: Component<{ position: EdgePanelPosition }> = (props) => {
  const panel = () => windowStore.edgePanels[props.position];
  const isCollapsed = () => isEdgeCollapsed(panel());
  const isVertical = () => props.position === 'left' || props.position === 'right';

  const expandedPanel = () => (
    <>
      {props.position === 'right' && (
        <EdgePanelResizeHandle position={props.position} />
      )}
      {/* No border here — the ribbon and handle lines are the separators.
          The panel body is a full layout tree rendered by the same
          SplitPane/Pane stack as the center tiling, so edge panels split,
          host tab bars, and accept drops exactly like center panes. */}
      <div
        data-edge-panel-body={props.position}
        class="flex flex-col overflow-hidden"
        style={
          isVertical()
            ? { width: panel().width ? `${panel().width}px` : '250px', 'min-width': '0' }
            : { height: panel().height ? `${panel().height}px` : '200px', 'min-height': '0' }
        }
      >
        <div class="flex-1 min-h-0 min-w-0">
          <SplitPane node={panel().layout} />
        </div>
      </div>
      {props.position === 'left' && <EdgePanelResizeHandle position={props.position} />}
    </>
  );

  // Slide with SYNCHRONIZED reflow, no remount: the panel content stays
  // MOUNTED while collapsed — clipped to zero size and visibility:hidden —
  // so a toggle never re-mounts the panel subtree (the old mount-on-expand
  // lifecycle spent ~500ms building the file tree exactly when the slide
  // should start; staying mounted also preserves tree expansion and
  // terminal scrollback). One rAF loop drives BOTH the clip frame's size
  // and the inner panel's translate from a single progress value, so the
  // neighboring content reflows smoothly across the whole toggle and the
  // clip edge stays pixel-locked to the panel edge. CSS transitions are
  // deliberately NOT used: width/height transitions run on the main thread
  // while `translate` runs on the compositor, and under load the two
  // desync — the panel visibly tears against its own clip edge.
  const TWEEN_MS = 200;

  /**
   * Whether the viewer asked for less motion.
   *
   * `matchMedia` and not a CSS variable: this tween runs in JavaScript, so the
   * media query has to be read rather than cascaded. Guarded because jsdom
   * (and any environment without `matchMedia`) must fall through to animating
   * rather than throw at panel construction.
   */
  const prefersReducedMotion = () =>
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const [progress, setProgress] = createSignal(isCollapsed() ? 0 : 1);
  let tweenRaf: number | undefined;

  createEffect(
    on(
      isCollapsed,
      (collapsed) => {
        const target = collapsed ? 0 : 1;
        if (tweenRaf !== undefined) cancelAnimationFrame(tweenRaf);
        const from = progress();
        if (from === target) return;
        // A layout RESTORE snaps: it's initialization, not an interaction —
        // tweening on page load looks wrong and slides the center layout
        // under anything that just measured it (stale-coordinate drags).
        //
        // So does a REDUCED-MOTION preference. index.css zeroes every
        // animation and transition under `prefers-reduced-motion`, and this is
        // the one animation in the app deliberately moved OUT of CSS — which
        // silently opted it out of that promise. A user who asked for no motion
        // still got a 200ms slide across a third of the window, which is the
        // largest moving thing the shell draws.
        //
        // Queried here rather than cached at module load so a preference
        // changed mid-session takes effect on the next toggle.
        if (isRestoringLayout() || prefersReducedMotion()) {
          setProgress(target);
          return;
        }
        // Duration scales with remaining distance so a mid-flight reversal
        // doesn't crawl.
        const dur = Math.max(1, TWEEN_MS * Math.abs(target - from));
        const start = performance.now();
        const step = (now: number) => {
          const t = Math.min(1, (now - start) / dur);
          const eased = 1 - (1 - t) * (1 - t); // ease-out
          setProgress(from + (target - from) * eased);
          tweenRaf = t < 1 ? requestAnimationFrame(step) : undefined;
        };
        tweenRaf = requestAnimationFrame(step);
      },
      { defer: true },
    ),
  );
  onCleanup(() => {
    if (tweenRaf !== undefined) cancelAnimationFrame(tweenRaf);
  });

  // Panel size + the 1px resize handle that lives inside the wrapper.
  const fullSize = () => (isVertical() ? (panel().width || 250) : (panel().height || 200)) + 1;
  const frameStyle = () => ({
    [isVertical() ? 'width' : 'height']: `${Math.round(fullSize() * progress())}px`,
    // Fully closed panels leave paint, hit-testing, and the tab order —
    // clipped-but-visible content is still keyboard-reachable otherwise.
    visibility: progress() > 0 ? ('visible' as const) : ('hidden' as const),
  });
  const innerStyle = () => {
    const off = (1 - progress()) * 100;
    const translate =
      props.position === 'left'
        ? `${-off}% 0`
        : props.position === 'right'
          ? `${off}% 0`
          : `0 ${off}%`;
    return {
      [isVertical() ? 'width' : 'height']: `${fullSize()}px`,
      translate,
    };
  };

  return (
    // Clip frame: snaps 0 ↔ full size (one reflow per toggle), content
    // always mounted…
    <div
      classList={{
        'flex overflow-hidden flex-none': true,
        'flex-row': isVertical(),
        'flex-col': !isVertical(),
      }}
      style={frameStyle()}
    >
      {/* …while the panel itself slides within it, at full opacity. */}
      <div
        classList={{
          'flex flex-none': true,
          'flex-row': isVertical(),
          'flex-col': !isVertical(),
        }}
        style={innerStyle()}
      >
        {expandedPanel()}
      </div>
    </div>
  );
};
