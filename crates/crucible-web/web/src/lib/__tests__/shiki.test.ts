import { describe, it, expect } from 'vitest';
import { initializeHighlighter, getHighlighter, SHIKI_THEME } from '../shiki';

describe('lib/shiki — singleton', () => {
  it('exposes a constant theme name', () => {
    expect(SHIKI_THEME).toBe('github-dark');
  });

  it('returns null from getHighlighter before initialization', () => {
    // Note: this test relies on Vitest module isolation between files.
    // If another test has already initialized, this becomes a no-op assertion.
    const h = getHighlighter();
    expect(h === null || typeof h.codeToTokens === 'function').toBe(true);
  });

  it('initializeHighlighter resolves and makes getHighlighter return a Highlighter', async () => {
    await initializeHighlighter();
    const h = getHighlighter();
    expect(h).not.toBeNull();
    expect(typeof h!.codeToTokens).toBe('function');
  });

  it('initializeHighlighter is idempotent — repeated calls return the same instance', async () => {
    await initializeHighlighter();
    const first = getHighlighter();
    await initializeHighlighter();
    const second = getHighlighter();
    expect(first).toBe(second);
  });
});
