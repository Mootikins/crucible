/**
 * Fade the edges of an element where its content runs past them, in place of
 * a hard cut. Use it as a ref: `<div class="mk-scroll" ref={scrollFade('y')}>`.
 *
 * It sets `data-fade-y` (a scroller) or `data-fade-x` (a line that clips) to
 * `start`, `end` or `both`, and mockup.css masks those edges. It reads the
 * scroll position, so a list at its top shows its first row at full strength.
 */
import { onCleanup } from 'solid-js';

export type FadeAxis = 'x' | 'y';

/** The edges that hide content, or null when all of it shows. */
function hiddenEdges(el: HTMLElement, axis: FadeAxis): 'start' | 'end' | 'both' | null {
  const [pos, view, total] =
    axis === 'y' ? [el.scrollTop, el.clientHeight, el.scrollHeight] : [el.scrollLeft, el.clientWidth, el.scrollWidth];
  // One pixel of slack: a fractional layout can leave a sub-pixel overflow.
  const start = pos > 1;
  const end = pos + view < total - 1;
  return start && end ? 'both' : start ? 'start' : end ? 'end' : null;
}

export const scrollFade = (axis: FadeAxis) => (el: HTMLElement) => {
  const key = axis === 'y' ? 'fadeY' : 'fadeX';
  const update = () => {
    const edges = hiddenEdges(el, axis);
    if (edges) el.dataset[key] = edges;
    else delete el.dataset[key];
  };
  el.addEventListener('scroll', update, { passive: true });
  // A resize changes the view, and new content changes the total.
  const resize = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
  resize?.observe(el);
  const mutation = new MutationObserver(update);
  mutation.observe(el, { childList: true, subtree: true, characterData: true });
  onCleanup(() => {
    el.removeEventListener('scroll', update);
    resize?.disconnect();
    mutation.disconnect();
  });
  requestAnimationFrame(update);
};
