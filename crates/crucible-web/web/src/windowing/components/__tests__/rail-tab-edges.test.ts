import { afterEach, expect, it } from 'vitest';
import { measureRailTabEdges } from '../rail-geometry';

afterEach(() => { document.body.replaceChildren(); });

it('uses pane geometry, not tab order, and clears stale edge joins', () => {
  const ribbon = document.createElement('div');
  const body = document.createElement('div');
  body.innerHTML = '<div data-pane-id="pane"></div>';
  ribbon.innerHTML = '<button data-ribbon-tab-id="a" data-ribbon-pane-id="pane"></button><button data-ribbon-tab-id="b" data-ribbon-pane-id="pane" data-highlighted></button>';
  const pane = body.firstElementChild as HTMLElement;
  const tab = ribbon.lastElementChild as HTMLElement;
  pane.getBoundingClientRect = () => ({ top: 100, bottom: 300, height: 200 }) as DOMRect;
  let top = 100;
  tab.getBoundingClientRect = () => ({ top, bottom: top + 40, height: 40 }) as DOMRect;
  measureRailTabEdges(ribbon, body);
  expect(tab.dataset.paneEdge).toBe('top');
  expect(pane.dataset.activeTabEdge).toBe('top');
  top = 170;
  measureRailTabEdges(ribbon, body);
  expect(tab.dataset.paneEdge).toBeUndefined();
  expect(pane.dataset.activeTabEdge).toBeUndefined();
  top = 260;
  measureRailTabEdges(ribbon, body);
  expect(tab.dataset.paneEdge).toBe('bottom');
  expect(pane.dataset.activeTabEdge).toBe('bottom');
  tab.removeAttribute('data-highlighted');
  measureRailTabEdges(ribbon, body);
  expect(tab.dataset.paneEdge).toBeUndefined();
  expect(pane.dataset.activeTabEdge).toBeUndefined();
});
