import { render } from 'solid-js/web';
import { OilView } from './OilView';

/**
 * Mount a live `OilView` into every `.oil-mount` placeholder in `host`.
 *
 * The reading view is an HTML string set through `innerHTML`, not a component
 * tree, so an interactive block cannot be produced by the string pipeline the
 * way a mermaid SVG can. These are islands: the markdown renderer emits an
 * empty div carrying three data attributes, and this puts a real component
 * inside it after the HTML lands and after DOMPurify has run.
 *
 * Returns a disposer that tears every island down. A caller that re-renders
 * the document MUST call it — Solid roots created here are not owned by the
 * calling component's lifecycle, so dropping the reference leaks the effect
 * and its pending fetches.
 */
export function mountOilViews(host: HTMLElement): () => void {
  const disposers: Array<() => void> = [];
  const mounts = host.querySelectorAll<HTMLElement>('.oil-mount');

  for (const el of mounts) {
    // Idempotent: a second pass over the same DOM (a re-render that reused
    // nodes) must not stack two views in one placeholder.
    if (el.dataset.oilMounted === 'true') continue;
    const plugin = el.getAttribute('data-oil-plugin');
    const view = el.getAttribute('data-oil-view');
    if (!plugin || !view) continue;

    let params: Record<string, unknown> = {};
    try {
      params = JSON.parse(el.getAttribute('data-oil-params') || '{}');
    } catch {
      // The fence parser already rejected malformed JSON, so reaching here
      // means the attribute was rewritten. An empty table is the safe read.
      params = {};
    }

    el.dataset.oilMounted = 'true';
    disposers.push(render(() => OilView({ plugin, view, params }), el));
  }

  return () => {
    for (const dispose of disposers) dispose();
  };
}
