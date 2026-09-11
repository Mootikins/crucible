import { describe, it, expect, vi } from 'vitest';
import { COMPACT_QUERY, detectCompact } from '@/stores/deviceStore';

/** A `matchMedia` that answers one fixed value and records what it was asked. */
function mediaAnswering(matches: boolean) {
  return vi.fn((query: string) => ({ matches, media: query }) as unknown as MediaQueryList);
}

describe('detectCompact', () => {
  it('asks exactly the compact query', () => {
    const mm = mediaAnswering(true);
    detectCompact(mm);
    expect(mm).toHaveBeenCalledWith(COMPACT_QUERY);
  });

  it('is compact when the viewport matches the query', () => {
    expect(detectCompact(mediaAnswering(true))).toBe(true);
  });

  it('is not compact when the viewport does not match', () => {
    expect(detectCompact(mediaAnswering(false))).toBe(false);
  });

  it('falls back to the desktop shell where matchMedia does not exist', () => {
    expect(detectCompact(undefined)).toBe(false);
  });

  it('falls back to the desktop shell when matchMedia throws', () => {
    const throwing = vi.fn(() => {
      throw new Error('not supported');
    });
    expect(detectCompact(throwing)).toBe(false);
  });
});
