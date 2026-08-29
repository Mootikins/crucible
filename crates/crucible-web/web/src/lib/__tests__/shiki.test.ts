import { describe, it, expect } from 'vitest';
import { initializeHighlighter, highlighter, SHIKI_THEMES } from '../shiki';

describe('lib/shiki — singleton', () => {
  it('exposes one theme name per shell theme', () => {
    // Must match the CodeMirror themes (editor + live preview) so code blocks
    // read the same in every surface, in both themes.
    expect(SHIKI_THEMES).toEqual({ dark: 'one-dark-pro', light: 'one-light' });
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

  it('loads BOTH themes into the highlighter', async () => {
    // Markdown renders to a cached HTML string and cannot re-highlight on a
    // theme toggle, so it asks for both palettes in one pass. A highlighter
    // that loaded only the dark theme throws on that call.
    const h = await initializeHighlighter();
    expect(h.getLoadedThemes().sort()).toEqual(['one-dark-pro', 'one-light']);
  });
});
