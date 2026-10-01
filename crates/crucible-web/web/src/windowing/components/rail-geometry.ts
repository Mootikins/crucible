import { createEffect, onCleanup, onMount } from 'solid-js';
import { windowStore } from '@/windowing/store';
import { collectPanes } from '@/windowing/model/tree';
import type { EdgePanelPosition } from '@/windowing/model/types';
import { isEdgeCollapsed } from '@/windowing/model/types';

/** The body of a rail: the element that holds the panes of the rail. */
export function railBodyEl(position: EdgePanelPosition): HTMLElement | undefined {
  return (
    document.querySelector<HTMLElement>(`[data-edge-panel-body="${position}"]`) ?? undefined
  );
}

/**
 * The top edge of a pane of the rail, as an offset from `originTop` in px.
 * Null when the pane has no element.
 */
export function paneTopIn(
  body: HTMLElement,
  paneId: string,
  originTop: number,
): number | null {
  const el = body.querySelector<HTMLElement>(`[data-pane-id="${paneId}"]`);
  return el ? el.getBoundingClientRect().top - originTop : null;
}

/**
 * Run `measure` each time the geometry of a rail can change.
 *
 * The ribbon and the body of a rail are siblings with the same top. A part of
 * the ribbon that points at a pane thus reads the position of the pane from
 * the DOM. It does not compute the position again from the split ratios.
 *
 * - A ResizeObserver watches the body, each pane, and each element that
 *   `extra` gives. A split drag resizes the panes, so the observer sees
 *   each step of the drag.
 * - An effect watches the shape of the tree (a pane that comes, goes or
 *   collapses) and the value of `key`. The observer cannot see a pane that
 *   does not exist yet, so the effect observes again and measures again.
 * - A window resize measures again.
 */
export function watchRailGeometry(
  position: EdgePanelPosition,
  measure: () => void,
  opts: {
    /** More elements whose size changes the measurement. */
    extra?: () => readonly (Element | undefined)[];
    /** A reactive value. Each change measures again. */
    key?: () => unknown;
  } = {},
): void {
  const panel = () => windowStore.edgePanels[position];
  const panes = () => collectPanes(panel().layout);

  let observer: ResizeObserver | undefined;
  const observe = () => {
    observer?.disconnect();
    const body = railBodyEl(position);
    if (!body) return;
    observer = new ResizeObserver(() => measure());
    observer.observe(body);
    for (const pane of panes()) {
      const el = body.querySelector(`[data-pane-id="${pane.id}"]`);
      if (el) observer.observe(el);
    }
    for (const el of opts.extra?.() ?? []) if (el) observer.observe(el);
  };

  createEffect(() => {
    panes()
      .map((p) => `${p.id}:${p.collapsed === true}`)
      .join('|');
    isEdgeCollapsed(panel());
    opts.key?.();
    queueMicrotask(() => {
      observe();
      measure();
    });
  });

  onMount(() => {
    requestAnimationFrame(measure);
    window.addEventListener('resize', measure);
  });

  onCleanup(() => {
    observer?.disconnect();
    window.removeEventListener('resize', measure);
  });
}

/** Publish actual tab/pane alignment; tab order alone cannot establish an edge. */
export function measureRailTabEdges(ribbon: HTMLElement, body: HTMLElement): void {
  const panes = Array.from(body.querySelectorAll<HTMLElement>('[data-pane-id]'));
  for (const pane of panes) delete pane.dataset.activeTabEdge;
  for (const tab of ribbon.querySelectorAll<HTMLElement>('[data-ribbon-tab-id]')) {
    delete tab.dataset.paneEdge;
    if (!tab.hasAttribute('data-highlighted')) continue;
    const pane = panes.find(el => el.dataset.paneId === tab.dataset.ribbonPaneId);
    if (!pane) continue;
    const tabBox = tab.getBoundingClientRect();
    const paneBox = pane.getBoundingClientRect();
    if (!tabBox.height || !paneBox.height) continue;
    const edge = Math.abs(tabBox.top - paneBox.top) <= 1 ? 'top'
      : Math.abs(tabBox.bottom - paneBox.bottom) <= 1 ? 'bottom' : undefined;
    if (edge) {
      tab.dataset.paneEdge = edge;
      pane.dataset.activeTabEdge = edge;
    }
  }
}
