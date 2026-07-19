import { describe, it, expect } from 'vitest';
import { initializeHighlighter, highlighter, SHIKI_THEME } from '../shiki';

describe('lib/shiki — singleton', () => {
  it('exposes a constant theme name', () => {
    expect(SHIKI_THEME).toBe('github-dark');
  });

  it('returns null from highlighter() before initialization', () => {
    // Vitest isolates modules per test file, and this test runs before the
    // init tests below, so the singleton is guaranteed uninitialized here.
    // Assert that strictly — the old `h === null || typeof ... === 'function'`
    // was a tautology that passed in every state.
    expect(highlighter()).toBeNull();
  });

  it('initializeHighlighter resolves and makes highlighter() return a Highlighter', async () => {
    await initializeHighlighter();
    const h = highlighter();
    expect(h).not.toBeNull();
    expect(typeof h!.codeToTokens).toBe('function');
  });

  it('initializeHighlighter is idempotent — repeated calls return the same instance', async () => {
    await initializeHighlighter();
    const first = highlighter();
    await initializeHighlighter();
    const second = highlighter();
    expect(first).toBe(second);
  });
});
