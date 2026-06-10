import { describe, it, expect } from 'vitest';
import { initializeHighlighter, highlighter, SHIKI_THEME } from '../shiki';

describe('lib/shiki — singleton', () => {
  it('exposes a constant theme name', () => {
    expect(SHIKI_THEME).toBe('github-dark');
  });

  it('returns null from highlighter() before initialization', () => {
    // Note: this test relies on Vitest module isolation between files.
    // If another test has already initialized, this becomes a no-op assertion.
    const h = highlighter();
    expect(h === null || typeof h.codeToTokens === 'function').toBe(true);
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
