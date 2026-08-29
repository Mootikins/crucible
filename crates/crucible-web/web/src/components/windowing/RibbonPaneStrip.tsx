import { Component, Show, createSignal, createEffect, onCleanup, onMount } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import { Dynamic } from 'solid-js/web';
import { windowStore, windowActions } from '@/stores/windowStore';
import { collectPanes } from '@/stores/windowStoreInternals';
import { isCollapsedLeaf } from '@/lib/pane-collapse';
import { paneBoundaries, findSplitInLayout } from '@/lib/pane-boundaries';
import { startSplitDrag } from '@/lib/split-drag';
import type { EdgePanelPosition, PaneNode } from '@/types/windowTypes';

/** A pane's marker band is exactly a tab bar: TabBar is `h-9`. */
const MARKER_PX = 36;

/** One pane's marker, placed at the pane's own top edge. */
interface Band {
  paneId: string;
  /** Offset from the ribbon's top, in px — MEASURED, never recomputed. */
  top: number;
  /** The split this pane's top edge belongs to; null for the topmost pane. */
  boundarySplitId: string | null;
  collapsed: boolean;
}

const ribbonBtn =
  'flex items-center justify-center text-muted-dark hover:text-shell-body hover:bg-hover-wash transition-colors';

/**
 * The ribbon's markers, each sitting on its pane's own top edge.
 *
 * MEASURED, NOT MIRRORED. The first version rebuilt the panel's geometry in
 * the ribbon from the same split ratios and drew the bands in the ribbon's
 * leftover flow space. The proportions matched and the pixels did not: the
 * fixed clusters above the strip pushed every band down by their own height,
 * so the terminal's marker sat 56px below the terminal's tab bar and the file
 * tree's sat 362px below its own. A marker that points at the wrong pane is
 * worse than no marker.
 *
 * The ribbon and the panel are siblings of equal height and equal top, so the
 * fix is to ask the DOM where each pane actually is and place the marker
 * there. That is exact by construction, and it stays exact when SplitPane's
 * own geometry changes — splitter thickness, min sizes, flex rounding — none
 * of which this file now knows or needs to know.
 *
 * Each band draws the pane's header across the rail: a HEAVY rule on the
 * boundary above it, the icon, then the tab bar's own light rule below. The
 * heavy rule is the boundary, so it drags the split — the same drag the
 * splitter between the panes runs, through the same `startSplitDrag`.
 */
export const RibbonPaneStrip: Component<{
  position: EdgePanelPosition;
  /** The ribbon element the bands are positioned within. */
  ribbonEl: () => HTMLElement | undefined;
}> = (props) => {
  const panel = () => windowStore.edgePanels[props.position];
  const panes = (): PaneNode[] => collectPanes(panel().layout);
  const [bands, setBands] = createSignal<Band[]>([]);
  /** The band of ribbon the markers may occupy — see `measure`. */
  const [floor, setFloor] = createSignal(Number.POSITIVE_INFINITY);
  const [ceiling, setCeiling] = createSignal(0);

  const bodyEl = () =>
    document.querySelector<HTMLElement>(`[data-edge-panel-body="${props.position}"]`) ?? undefined;

  const measure = () => {
    const ribbon = props.ribbonEl();
    const body = bodyEl();
    if (!ribbon || !body) {
      setBands([]);
      return;
    }
    const ribbonTop = ribbon.getBoundingClientRect().top;
    const boundaries = paneBoundaries(panel().layout);
    const next: Band[] = [];
    for (const pane of panes()) {
      const el = body.querySelector<HTMLElement>(`[data-pane-id="${pane.id}"]`);
      if (!el) continue;
      next.push({
        paneId: pane.id,
        top: el.getBoundingClientRect().top - ribbonTop,
        boundarySplitId: boundaries.get(pane.id) ?? null,
        collapsed: pane.collapsed === true,
      });
    }
    setBands(next);

    // Two pinned clusters own ribbon pixels the strip may not draw on: the
    // rail toggle at the top, and the bell (right) or the settings pair (left)
    // at the bottom. A band that reached either would put a marker under a
    // button that already owns those pixels.
    //
    // The band is DROPPED, never nudged. Nudging is exactly what the mirrored
    // strip did, and a marker one cluster-height away from its pane is the bug
    // this file exists to remove — a marker that is absent is honest, a marker
    // that lies is not.
    //
    // The topmost pane always loses this way: its top edge is y=0, which is the
    // rail toggle's own 36px. That pane has no boundary above it to drag
    // either, so the two facts agree — the rail toggle IS the control at that
    // height.
    const top = ribbon.querySelector<HTMLElement>('[data-ribbon-ceiling]');
    setCeiling(top ? top.getBoundingClientRect().bottom - ribbonTop : 0);
    const pinned = ribbon.querySelector<HTMLElement>('[data-ribbon-floor]');
    setFloor(pinned ? pinned.getBoundingClientRect().top - ribbonTop : Number.POSITIVE_INFINITY);
  };

  // Re-measure whenever the tree's SHAPE changes (a pane added, removed or
  // collapsed) — the ResizeObserver below covers every size change, including
  // a ratio drag, but it cannot fire for a pane that does not exist yet.
  createEffect(() => {
    panes()
      .map((p) => `${p.id}:${p.collapsed === true}`)
      .join('|');
    panel().isCollapsed;
    queueMicrotask(measure);
  });

  let observer: ResizeObserver | undefined;
  const observe = () => {
    observer?.disconnect();
    const body = bodyEl();
    if (!body) return;
    observer = new ResizeObserver(() => measure());
    observer.observe(body);
    for (const pane of panes()) {
      const el = body.querySelector(`[data-pane-id="${pane.id}"]`);
      if (el) observer.observe(el);
    }
  };

  createEffect(() => {
    panes()
      .map((p) => p.id)
      .join('|');
    queueMicrotask(observe);
  });

  onMount(() => {
    requestAnimationFrame(measure);
    window.addEventListener('resize', measure);
  });

  onCleanup(() => {
    observer?.disconnect();
    window.removeEventListener('resize', measure);
  });

  return (
    <div
      data-testid={`ribbon-pane-strip-${props.position}`}
      class="absolute inset-x-0 top-0 bottom-0 z-10 pointer-events-none"
    >
      {/* KEYED BY PANE. `measure` builds fresh band objects, so a plain For
          rebuilds every row on every measurement — and a drag re-measures on
          each pointermove, which unmounted the handle under the pointer and
          ran its cleanup. The drag died after about one move: a 300px gesture
          resized the pane by 25px. Keying holds the row across a re-measure. */}
      <Key each={bands()} by={(b) => b.paneId}>
        {(band) => (
          <Show when={band().top >= ceiling() && band().top + MARKER_PX <= floor()}>
            <RibbonPaneBand position={props.position} band={band()} />
          </Show>
        )}
      </Key>
    </div>
  );
};

/**
 * One band: the boundary rule, the marker, and the tab bar's own underline.
 *
 * The marker wears the pane's ACTIVE TAB icon, so the terminal's marker is a
 * terminal — the rail reads as "this icon controls that pane" rather than as a
 * column of anonymous chevrons.
 */
const RibbonPaneBand: Component<{ position: EdgePanelPosition; band: Band }> = (props) => {
  const panel = () => windowStore.edgePanels[props.position];
  const pane = () => collectPanes(panel().layout).find((p) => p.id === props.band.paneId);
  const group = () => {
    const id = pane()?.tabGroupId;
    return id ? windowStore.tabGroups[id] : undefined;
  };
  // The active tab, or the first — a group whose active tab was just closed
  // still has a pane to control.
  const tab = () => {
    const g = group();
    if (!g) return undefined;
    return g.tabs.find((t) => t.id === g.activeTabId) ?? g.tabs[0];
  };
  const label = () => tab()?.title ?? 'pane';
  const collapsed = () => props.band.collapsed;

  const split = () => {
    const id = props.band.boundarySplitId;
    return id ? findSplitInLayout(panel().layout, id) : null;
  };
  // Same rule as the splitter between the panes: a collapsed side pins the
  // boundary, because `splitRatio` is the size the pane opens back to and a
  // drag would silently rewrite it.
  const locked = () => {
    const s = split();
    if (!s) return true;
    return isCollapsedLeaf(s.first) || isCollapsedLeaf(s.second);
  };

  const [dragging, setDragging] = createSignal(false);
  let cleanup: (() => void) | null = null;
  onCleanup(() => cleanup?.());

  const onHandleDown = (e: PointerEvent) => {
    const s = split();
    if (e.button !== 0 || !s || locked()) return;
    e.preventDefault();
    e.stopPropagation();
    setDragging(true);
    cleanup = startSplitDrag({
      event: e,
      splitId: s.id,
      direction: s.direction === 'horizontal' ? 'horizontal' : 'vertical',
      startRatio: s.splitRatio,
      // The split's own container in the PANEL is the box the ratio means
      // something against — the ribbon is only where the pointer happens to be.
      //
      // The testid is part of the selector, not decoration. This handle used to
      // carry `data-split-id` as well, so the lookup matched THIS band — 36px
      // tall — and every drag divided by 36 instead of by the split's height,
      // which pinned the ratio to its 0.1 clamp on the first pixel of movement.
      getContainerRect: () =>
        document
          .querySelector(`[data-testid="resize-splitter"][data-split-id="${s.id}"]`)
          ?.parentElement?.getBoundingClientRect() ?? null,
      onPreview: (ratio) => windowActions.commitSplitRatio(s.id, ratio),
      onEnd: () => {
        setDragging(false);
        cleanup = null;
      },
    });
  };

  return (
    <div class="absolute inset-x-0" style={{ top: `${props.band.top}px` }}>
      {/* The boundary itself: heavier than the tab bar's underline because it
          separates two panes rather than a header from its body. The after:
          pseudo widens the grab target to ±4px without thickening the line. */}
      <Show when={props.band.boundarySplitId}>
        <div
          data-testid={`ribbon-split-handle-${props.position}`}
          data-boundary-split-id={props.band.boundarySplitId}
          data-locked={locked() ? 'true' : undefined}
          classList={{
            'absolute inset-x-0 -top-0.5 h-0.5 pointer-events-auto transition-colors': true,
            "after:content-[''] after:absolute after:inset-x-0 after:-inset-y-1": !locked(),
            'cursor-row-resize': !locked(),
            'bg-primary': dragging(),
            'bg-hairline-strong': !dragging(),
          }}
          on:pointerdown={onHandleDown}
        />
      </Show>
      <button
        type="button"
        data-testid={`ribbon-pane-marker-${props.position}`}
        data-pane-id={props.band.paneId}
        data-collapsed={collapsed() ? 'true' : 'false'}
        aria-expanded={!collapsed()}
        class={`${ribbonBtn} w-full border-b border-hairline pointer-events-auto`}
        style={{ height: `${MARKER_PX}px` }}
        classList={{ 'text-shell-body': !collapsed() }}
        title={`${collapsed() ? 'Expand' : 'Collapse'} ${label()}`}
        onClick={() => windowActions.togglePaneCollapsed(props.band.paneId)}
      >
        <Show when={tab()?.icon} fallback={<span class="text-xs">{label()[0]}</span>}>
          {(icon) => <Dynamic component={icon()} class="w-4 h-4" />}
        </Show>
      </button>
    </div>
  );
};
