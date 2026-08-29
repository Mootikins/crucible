import { describe, it, expect } from 'vitest';
import { terseAge } from '@/lib/format-time';

// A fixed clock, injected — the reason `terseAge` takes `now` at all. Without
// it every threshold test would be a race against the wall clock.
const NOW = Date.parse('2026-08-29T12:00:00Z');
const ago = (ms: number) => new Date(NOW - ms).toISOString();

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

describe('terseAge', () => {
  it('collapses anything under a minute to "now"', () => {
    expect(terseAge(ago(0), NOW)).toBe('now');
    expect(terseAge(ago(59_000), NOW)).toBe('now');
  });

  it('counts minutes, then hours, then days', () => {
    expect(terseAge(ago(MINUTE), NOW)).toBe('1m');
    expect(terseAge(ago(59 * MINUTE), NOW)).toBe('59m');
    expect(terseAge(ago(HOUR), NOW)).toBe('1h');
    expect(terseAge(ago(23 * HOUR), NOW)).toBe('23h');
    expect(terseAge(ago(DAY), NOW)).toBe('1d');
    expect(terseAge(ago(400 * DAY), NOW)).toBe('400d');
  });

  it('answers null for a timestamp it cannot use', () => {
    // Null, not '': a caller renders nothing rather than an empty chip.
    expect(terseAge(null, NOW)).toBeNull();
    expect(terseAge(undefined, NOW)).toBeNull();
    expect(terseAge('', NOW)).toBeNull();
    expect(terseAge('not a date', NOW)).toBeNull();
  });

  it('does not run backwards for a clock-skewed future timestamp', () => {
    // A daemon a few seconds ahead must not print "-1m".
    expect(terseAge(new Date(NOW + 30_000).toISOString(), NOW)).toBe('now');
  });
});
