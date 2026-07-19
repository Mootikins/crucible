import { describe, it, expect } from 'vitest';
import { fuzzyScore } from '../fuzzy';

describe('fuzzyScore', () => {
  it('empty query matches everything with a neutral score', () => {
    expect(fuzzyScore('anything', '')).not.toBeNull();
  });

  it('returns null when the query is not a subsequence', () => {
    expect(fuzzyScore('clear chat', 'xyz')).toBeNull();
    expect(fuzzyScore('home', 'homes')).toBeNull();
  });

  it('is case-insensitive', () => {
    expect(fuzzyScore('Clear Chat', 'clear')).not.toBeNull();
    expect(fuzzyScore('clear chat', 'CLEAR')).not.toBeNull();
  });

  it('matches non-contiguous subsequences', () => {
    // "sst" is not a substring of "session settings" prefix run but is a subsequence
    expect(fuzzyScore('switch session', 'swse')).not.toBeNull();
    expect(fuzzyScore('New Session', 'nsn')).not.toBeNull();
  });

  it('ranks substring matches above scattered subsequences', () => {
    const substring = fuzzyScore('clear chat', 'chat')!;
    const scattered = fuzzyScore('change theme about', 'chat')!;
    expect(substring).toBeGreaterThan(scattered);
  });

  it('ranks word-start matches above mid-word matches', () => {
    const wordStart = fuzzyScore('open settings', 'set')!;
    const midWord = fuzzyScore('reset counter', 'set')!;
    expect(wordStart).toBeGreaterThan(midWord);
  });

  it('ranks earlier matches above later ones', () => {
    const early = fuzzyScore('home page', 'home')!;
    const late = fuzzyScore('go back home', 'home')!;
    expect(early).toBeGreaterThan(late);
  });
});
