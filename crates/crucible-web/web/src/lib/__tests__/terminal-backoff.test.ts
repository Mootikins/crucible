import { describe, it, expect } from 'vitest';
import { nextReconnectDelay, RECONNECT_CAP_MS } from '../terminal-backoff';

describe('nextReconnectDelay', () => {
  /** Jitter pinned to its extremes so the arithmetic is exact. */
  const shortest = () => 0;
  const longest = () => 1 - Number.EPSILON;

  it('doubles per attempt', () => {
    // 500, 1000, 2000, 4000 — read at full jitter so the base shows through.
    expect(nextReconnectDelay(0, longest)).toBe(500);
    expect(nextReconnectDelay(1, longest)).toBe(1000);
    expect(nextReconnectDelay(2, longest)).toBe(2000);
    expect(nextReconnectDelay(3, longest)).toBe(4000);
  });

  it('never exceeds the cap, however many attempts have failed', () => {
    for (const attempt of [5, 10, 50, 1000]) {
      expect(nextReconnectDelay(attempt, longest)).toBeLessThanOrEqual(RECONNECT_CAP_MS);
    }
    expect(nextReconnectDelay(1000, longest)).toBe(RECONNECT_CAP_MS);
  });

  it('jitters downward only, so the cap stays a real ceiling', () => {
    // Half of the capped exponential at the low extreme, full at the high one.
    expect(nextReconnectDelay(3, shortest)).toBe(2000);
    expect(nextReconnectDelay(3, longest)).toBe(4000);
    expect(nextReconnectDelay(99, shortest)).toBe(RECONNECT_CAP_MS / 2);
  });

  it('treats a negative attempt as the first one rather than shrinking below base', () => {
    // 2 ** -1 would be 250ms, i.e. a retry storm from an accounting slip.
    expect(nextReconnectDelay(-3, longest)).toBe(500);
  });

  it('stays inside the jitter window for real randomness', () => {
    for (let attempt = 0; attempt < 8; attempt++) {
      const base = Math.min(500 * 2 ** attempt, RECONNECT_CAP_MS);
      for (let i = 0; i < 50; i++) {
        const delay = nextReconnectDelay(attempt);
        expect(delay).toBeGreaterThanOrEqual(Math.round(base * 0.5));
        expect(delay).toBeLessThanOrEqual(base);
      }
    }
  });
});
