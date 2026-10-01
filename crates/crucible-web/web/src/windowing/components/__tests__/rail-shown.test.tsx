import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
import { WindowManager } from '../WindowManager';
import { windowActions, windowStore } from '@/windowing/store';
import { configureRails, neutralRenderer } from './fixtures';

/**
 * A rail counts as shown while any of its body is on screen. Its icon then
 * shows the open pane: at the START of the slide when the rail opens, and
 * until the END of the slide when it closes.
 */
// A fake clock drives the slide, so the test does not depend on the speed of
// real animation frames under a loaded run.
beforeEach(() => {
  vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame', 'performance'] });
  configureRails();
});
afterEach(() => vi.useRealTimers());
/** Runs the slide to its end. */
const finishSlide = () => vi.advanceTimersByTime(500);

const mount = () => render(() => <WindowManager renderContent={neutralRenderer} slots={{}} />);
const alphaIcon = () =>
  Array.from(document.querySelectorAll<HTMLElement>('.wm-ribbon-tab')).find((b) => b.title === 'Alpha')!;
const host = (side: 'left' | 'right') => document.querySelector<HTMLElement>(`[data-testid='edge-host-${side}']`)!;

describe('the shown state of a rail', () => {
  it('stays on while a closing rail slides, and goes off when it is shut', () => {
    mount();
    expect(alphaIcon().hasAttribute('data-highlighted')).toBe(true);
    windowActions.setEdgePanelCollapsed('left', true);
    // The slide has not run yet: the body is still on screen.
    expect(alphaIcon().hasAttribute('data-highlighted')).toBe(true);
    expect(host('left').hasAttribute('data-edge-shown')).toBe(true);
    vi.advanceTimersByTime(100);
    // Half-way: still shown.
    expect(alphaIcon().hasAttribute('data-highlighted')).toBe(true);
    finishSlide();
    expect(alphaIcon().hasAttribute('data-highlighted')).toBe(false);
    expect(host('left').hasAttribute('data-edge-shown')).toBe(false);
  });

  it('comes on at the start of an opening slide', () => {
    mount();
    windowActions.setEdgePanelCollapsed('left', true);
    finishSlide();
    expect(host('left').hasAttribute('data-edge-shown')).toBe(false);
    windowActions.setEdgePanelCollapsed('left', false);
    expect(host('left').hasAttribute('data-edge-shown')).toBe(true);
    expect(alphaIcon().hasAttribute('data-highlighted')).toBe(true);
  });

  it('publishes the slide progress, from 1 when open to 0 when shut', () => {
    mount();
    const progress = () => Number(host('left').style.getPropertyValue('--wm-edge-progress'));
    expect(progress()).toBe(1);
    windowActions.setEdgePanelCollapsed('left', true);
    vi.advanceTimersByTime(100);
    expect(progress()).toBeGreaterThan(0);
    expect(progress()).toBeLessThan(1);
    finishSlide();
    expect(progress()).toBe(0);
  });
});


it('a tiled rail icon collapses only its own pane, and reopens it', () => {
  mount();
  windowActions.setEdgePanelCollapsed('right', false);
  windowActions.setPaneCollapsed('right-term-pane', false);
  finishSlide();
  const omega = document.querySelector<HTMLButtonElement>('.wm-ribbon-tab[title="Omega"]')!;
  fireEvent.click(omega);
  expect(windowStore.edgePanels.right.mode).toBe('docked');
  const layout = windowStore.edgePanels.right.layout;
  if (layout.type !== 'split' || layout.second.type !== 'pane') throw new Error('expected stacked panes');
  expect(layout.second.collapsed).toBe(true);
  fireEvent.click(omega);
  expect(layout.second.collapsed).toBe(false);
});
