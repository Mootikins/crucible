import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createRoot } from 'solid-js';
import { createRevealController, type RevealController } from '@/windowing/reveal/RevealController';

/** Make a controller inside a Solid root, and return the root's disposer too. */
function make(opts: { enterDelay: number; leaveDelay: number }): [RevealController, () => void] {
  return createRoot((dispose) => [createRevealController(opts), dispose]);
}

describe('RevealController', () => {
  const roots: Array<() => void> = [];
  const controller = (opts: { enterDelay: number; leaveDelay: number }) => {
    const [c, dispose] = make(opts);
    roots.push(() => { c.dispose(); dispose(); });
    return c;
  };

  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    roots.splice(0).forEach((d) => d());
    vi.useRealTimers();
  });

  it('reveals after the enter delay, and closes after the leave delay', () => {
    const c = controller({ enterDelay: 150, leaveDelay: 300 });
    c.pointerEnter();
    expect(c.state()).toBe('closed');
    vi.advanceTimersByTime(150);
    expect(c.state()).toBe('revealed');
    c.pointerLeave();
    vi.advanceTimersByTime(299);
    expect(c.state()).toBe('revealed');
    vi.advanceTimersByTime(1);
    expect(c.state()).toBe('closed');
  });

  it('a leave before the enter delay cancels the reveal', () => {
    const c = controller({ enterDelay: 150, leaveDelay: 300 });
    c.pointerEnter();
    vi.advanceTimersByTime(100);
    c.pointerLeave();
    vi.advanceTimersByTime(1000);
    expect(c.state()).toBe('closed');
  });

  it('an enter while revealed cancels a pending close', () => {
    const c = controller({ enterDelay: 150, leaveDelay: 300 });
    c.pointerEnter();
    vi.advanceTimersByTime(150);
    c.pointerLeave();
    vi.advanceTimersByTime(200);
    c.pointerEnter();
    vi.advanceTimersByTime(1000);
    expect(c.state()).toBe('revealed');
    // The next leave arms a full close delay again.
    c.pointerLeave();
    vi.advanceTimersByTime(299);
    expect(c.state()).toBe('revealed');
    vi.advanceTimersByTime(1);
    expect(c.state()).toBe('closed');
  });

  it('a pin holds the reveal open through a leave; unpin closes', () => {
    const c = controller({ enterDelay: 0, leaveDelay: 0 });
    c.pointerEnter();
    vi.advanceTimersByTime(0);
    c.pin();
    c.pointerLeave();
    vi.advanceTimersByTime(1000);
    expect(c.state()).toBe('pinned');
    c.unpin();
    expect(c.state()).toBe('closed');
  });

  it('a pin cancels a pending close', () => {
    const c = controller({ enterDelay: 0, leaveDelay: 300 });
    c.pointerEnter();
    vi.advanceTimersByTime(0);
    c.pointerLeave();
    c.pin();
    vi.advanceTimersByTime(1000);
    expect(c.state()).toBe('pinned');
  });

  it('a tap toggles, for touch where hover does not exist', () => {
    const c = controller({ enterDelay: 150, leaveDelay: 300 });
    c.tap();
    expect(c.state()).toBe('pinned');
    c.tap();
    expect(c.state()).toBe('closed');
  });

  it('a tap while revealed pins, and cancels a pending close', () => {
    const c = controller({ enterDelay: 0, leaveDelay: 300 });
    c.pointerEnter();
    vi.advanceTimersByTime(0);
    expect(c.state()).toBe('revealed');
    c.pointerLeave();
    c.tap();
    expect(c.state()).toBe('pinned');
    vi.advanceTimersByTime(1000);
    expect(c.state()).toBe('pinned');
  });

  it('dispose clears a pending reveal', () => {
    const c = controller({ enterDelay: 150, leaveDelay: 300 });
    c.pointerEnter();
    c.dispose();
    vi.advanceTimersByTime(1000);
    expect(c.state()).toBe('closed');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('dispose clears a pending close', () => {
    const c = controller({ enterDelay: 0, leaveDelay: 300 });
    c.pointerEnter();
    vi.advanceTimersByTime(0);
    c.pointerLeave();
    expect(vi.getTimerCount()).toBe(1);
    c.dispose();
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(1000);
    expect(c.state()).toBe('revealed');
  });
});
