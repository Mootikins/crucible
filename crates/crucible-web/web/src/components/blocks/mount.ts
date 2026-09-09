import { render } from 'solid-js/web';
import { PluginBlock } from './PluginBlock';

/**
 * Mount a live component into every `.plugin-mount` placeholder in `host`.
 *
 * The reading view is an HTML string set through `innerHTML`, not a component
 * tree, so an interactive block cannot come out of the string pipeline the way
 * a mermaid SVG can. These are islands: the markdown renderer emits an empty
 * div carrying three data attributes, and this puts a real component inside it
 * after the HTML lands and after DOMPurify has run.
 *
 * Returns a disposer that tears every island down. A caller that re-renders
 * the document MUST call it — these Solid roots are not owned by the calling
 * component's lifecycle, so dropping the reference leaks the effect, its
 * pending fetches and its event-stream subscription.
 */
export function mountPluginBlocks(host: HTMLElement): () => void {
  const disposers: Array<() => void> = [];

  for (const el of host.querySelectorAll<HTMLElement>('.plugin-mount')) {
    // Idempotent: a second pass over the same DOM (a re-render that reused
    // nodes) must not stack two blocks in one placeholder.
    if (el.dataset.pluginMounted === 'true') continue;
    const plugin = el.getAttribute('data-plugin-name');
    const block = el.getAttribute('data-plugin-block');
    if (!plugin || !block) continue;

    let params: Record<string, unknown> = {};
    try {
      params = JSON.parse(el.getAttribute('data-plugin-params') || '{}');
    } catch {
      // The fence parser already rejected malformed JSON, so reaching here
      // means the attribute was rewritten. An empty table is the safe read.
      params = {};
    }

    el.dataset.pluginMounted = 'true';
    disposers.push(render(() => PluginBlock({ plugin, block, params }), el));
  }

  return () => {
    for (const dispose of disposers) dispose();
  };
}
