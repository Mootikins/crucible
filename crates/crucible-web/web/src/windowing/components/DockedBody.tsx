import { Component, Show, createEffect, createSignal, on, onCleanup } from 'solid-js';
import { windowStore, windowActions } from '@/windowing/store';
import type { EdgePanelPosition } from '@/windowing/model/types';
import { isEdgeCollapsed } from '@/windowing/model/types';
import { isRestoringLayout } from '@/windowing/model/layout-restore';
import { SplitPane } from './SplitPane';
import { Ribbon, RIBBON_WIDTH_PX } from './Ribbon';
import { setRailProgress, setRailShown } from './rail-shown';

const EDGE_PANEL_MIN_WIDTH = 120;
// No fixed max: a rail that holds a wide panel may take most of the viewport.
// The limit keeps a narrow strip of the centre pane usable.
const edgePanelMaxWidth = () => Math.max(600, window.innerWidth - 320);

function EdgePanelResizeHandle(props: { position: EdgePanelPosition }) {
  const panel = () => windowStore.edgePanels[props.position];
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
    const startSize = panel().width ?? 250;

    const handlePointerMove = (e: PointerEvent) => {
      const delta = props.position === 'left' ? e.clientX - startX : startX - e.clientX;
      windowActions.setEdgePanelSize(
        props.position,
        Math.max(EDGE_PANEL_MIN_WIDTH, Math.min(edgePanelMaxWidth(), startSize + delta))
      );
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
      aria-orientation="vertical"
      class="wm-edge-handle relative flex-shrink-0 z-10 after:content-[''] after:absolute w-px cursor-col-resize after:inset-y-0 after:-inset-x-1"
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
  /** The rail covers the centre: it fills the row, and its width does not apply. */
  const coversCentre = () => windowStore.expandedEdge === props.position;
  /** The ribbon rides inside the body, and slides with it. See `RibbonPlacement`. */
  const ribbonInside = () => windowStore.ribbonPlacement === 'panel';

  const handle = () => !coversCentre() && <EdgePanelResizeHandle position={props.position} />;
  // The panel body is a full layout tree rendered by the same SplitPane/Pane
  // stack as the center tiling, so edge panels split, host tab bars, and
  // accept drops exactly like center panes.
  const body = () => (
    <div
      data-edge-panel-body={props.position}
      class="wm-edge-body flex flex-col overflow-hidden"
      style={{
        ...(coversCentre()
          ? { flex: '1 1 auto' }
          : { width: panel().width ? `${panel().width}px` : '250px' }),
        'min-width': '0',
        // With the ribbon inside, the frame never closes to nothing: the
        // ribbon stays in view. The clipped body leaves paint, hit-testing
        // and the tab order on its own.
        visibility: ribbonInside() && progress() === 0 ? 'hidden' : undefined,
      }}
    >
      <div class="flex-1 min-h-0 min-w-0">
        <SplitPane node={panel().layout} />
      </div>
    </div>
  );

  const expandedPanel = () => (
    <Show
      when={ribbonInside()}
      fallback={
        // No border here — the ribbon and handle lines are the separators.
        <>
          {props.position === 'right' && handle()}
          {body()}
          {props.position === 'left' && handle()}
        </>
      }
    >
      {/* The card: the body, the handle, and the ribbon on the INSIDE edge,
          next to the centre. The card slides as one piece, and a closed rail
          shows the ribbon alone. A theme draws the card's edge. */}
      <div
        data-edge-card={props.position}
        class="wm-edge-card flex flex-row min-w-0 overflow-hidden"
        style={{ flex: coversCentre() ? '1 1 auto' : '0 0 auto' }}
      >
        {props.position === 'left' ? (
          <>
            {body()}
            {handle()}
            <Ribbon position="left" />
          </>
        ) : (
          <>
            <Ribbon position={props.position} />
            {handle()}
            {body()}
          </>
        )}
      </div>
    </Show>
  );

  // Slide with SYNCHRONIZED reflow, no remount: the panel content stays
  // MOUNTED while collapsed — clipped to zero size and visibility:hidden —
  // so a toggle never re-mounts the panel subtree (the old mount-on-expand
  // lifecycle spent ~500ms building a heavy panel exactly when the slide
  // should start; staying mounted also keeps the state that each panel
  // holds in its DOM). One rAF loop drives BOTH the clip frame's size
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
  // Shown while the rail is open, and while it still slides shut. See rail-shown.ts.
  createEffect(() => setRailShown(props.position, !isCollapsed() || progress() > 0));
  createEffect(() => setRailProgress(props.position, progress()));
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
        // largest moving thing the window manager draws.
        //
        // Queried here rather than cached at module load so a preference
        // that the user changes later takes effect on the next toggle.
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

  // Panel size + the 1px resize handle that lives inside the wrapper: the
  // part of the rail that slides. With the ribbon inside, the ribbon is the
  // part that stays.
  const slideSize = () => (panel().width || 250) + 1;
  const stay = () => (ribbonInside() ? RIBBON_WIDTH_PX : 0);
  const frameStyle = () => ({
    // An expanded rail takes the row. The slide still runs on a collapse,
    // because a collapse ends the expand first (see toggleEdgePanel).
    ...(coversCentre()
      ? { flex: '1 1 auto', 'min-width': '0' }
      : { width: `${Math.round(stay() + slideSize() * progress())}px` }),
    // Fully closed panels leave paint, hit-testing, and the tab order —
    // clipped-but-visible content is still keyboard-reachable otherwise.
    // A ribbon inside the rail stays; the body hides itself instead.
    visibility: progress() > 0 || ribbonInside() ? ('visible' as const) : ('hidden' as const),
  });
  const innerStyle = () => {
    if (coversCentre()) return { width: '100%' };
    const off = (1 - progress()) * slideSize();
    // Left: the piece is anchored at the frame's inside edge, so it slides
    // out past the window edge. Right: the frame's inside edge is its left
    // edge, where the piece already starts, so it moves with the frame.
    const translate =
      props.position === 'left' ? `${-off}px 0` : ribbonInside() ? '0 0' : `${off}px 0`;
    return { width: `${stay() + slideSize()}px`, translate };
  };

  return (
    // Clip frame: snaps 0 ↔ full size (one reflow per toggle), content
    // always mounted…
    <div class="flex flex-row overflow-hidden flex-none" style={frameStyle()}>
      {/* …while the panel itself slides within it, at full opacity. */}
      <div class="flex flex-row flex-none" style={innerStyle()}>
        {expandedPanel()}
      </div>
    </div>
  );
};
