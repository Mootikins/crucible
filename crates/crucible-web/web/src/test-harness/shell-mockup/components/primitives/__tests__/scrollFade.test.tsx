import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import { scrollFade } from '../scrollFade';

/** A scroller with fixed metrics: jsdom lays nothing out. */
function scroller(axis: 'x' | 'y', total: number, view: number) {
  let el!: HTMLDivElement;
  render(() => <div ref={(e) => { el = e; scrollFade(axis)(e); }} />);
  const [size, client, pos] = axis === 'y' ? ['scrollHeight', 'clientHeight', 'scrollTop'] : ['scrollWidth', 'clientWidth', 'scrollLeft'];
  Object.defineProperty(el, size, { value: total, configurable: true });
  Object.defineProperty(el, client, { value: view, configurable: true });
  const scrollTo = (at: number) => {
    Object.defineProperty(el, pos, { value: at, configurable: true });
    el.dispatchEvent(new Event('scroll'));
  };
  return { el, scrollTo };
}

describe('scrollFade', () => {
  it('names only the edges that hide content', () => {
    const { el, scrollTo } = scroller('y', 500, 100);
    scrollTo(0);
    expect(el.dataset.fadeY).toBe('end');
    scrollTo(200);
    expect(el.dataset.fadeY).toBe('both');
    scrollTo(400);
    expect(el.dataset.fadeY).toBe('start');
  });

  it('marks nothing when all the content shows', () => {
    const { el, scrollTo } = scroller('y', 100, 100);
    scrollTo(0);
    expect(el.dataset.fadeY).toBeUndefined();
  });

  it('reads the horizontal axis for a line that clips', () => {
    const { el, scrollTo } = scroller('x', 300, 120);
    scrollTo(0);
    expect(el.dataset.fadeX).toBe('end');
    expect(el.dataset.fadeY).toBeUndefined();
  });
});
