import { describe, it, expect, vi } from 'vitest';
import { COMPACT_QUERY, detectCompact } from '@/stores/deviceStore';

/** A `matchMedia` that answers one fixed value and records what it was asked. */
function mediaAnswering(matches: boolean) {
  return vi.fn((query: string) => ({ matches, media: query }) as unknown as MediaQueryList);
}

describe('detectCompact', () => {
  // The LITERAL, not the constant. Asserting `COMPACT_QUERY` against itself
  // is satisfied by any value, and this 767 px decides which shell every user
  // gets. It is also written a second time in `index.css`, which is why it is
  // pinned here rather than left to whatever the module happens to hold.
  it('asks for the 767 px breakpoint the stylesheet also uses', () => {
    const mm = mediaAnswering(true);
    detectCompact(mm);
    expect(mm).toHaveBeenCalledWith('(max-width: 767px)');
    expect(COMPACT_QUERY).toBe('(max-width: 767px)');
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
