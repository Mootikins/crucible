import { describe, it, expect, beforeEach } from 'vitest';
import { createComputed, createRoot } from 'solid-js';
import { DEFAULT_THEME, applyTheme, readTheme, theme } from '@/lib/theme';

describe('theme', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
  });

  it('starts on the shell’s own identity', () => {
    expect(readTheme()).toBe(DEFAULT_THEME);
    expect(DEFAULT_THEME).toBe('dark');
  });

  it('marks the document root for light, and remembers it', () => {
    applyTheme('light');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    expect(readTheme()).toBe('light');
  });

  it('writes NO attribute for dark', () => {
    applyTheme('light');
    applyTheme('dark');
    // The dark palette is the bare `:root` declaration, so an absent attribute
    // and a dark one have to mean the same thing — a page that has not run
    // this yet is already dark.
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
    expect(readTheme()).toBe('dark');
  });

  it('survives a round trip either way', () => {
    for (const theme of ['light', 'dark', 'light'] as const) {
      applyTheme(theme);
      expect(readTheme()).toBe(theme);
    }
  });

  it('publishes the change as a signal', () => {
    // Three surfaces cannot follow a CSS custom property: CodeMirror compiles
    // its syntax colors into a StyleModule, xterm takes a color object, and the
    // graph paints to a canvas. They track THIS, so an attribute written
    // without it repaints nothing.
    applyTheme('light');
    expect(theme()).toBe('light');
    applyTheme('dark');
    expect(theme()).toBe('dark');
  });

  it('sets the root attribute BEFORE the signal', () => {
    // The order is load-bearing: every subscriber re-reads the tokens out of
    // the DOM, so a signal that fired first would hand them the OLD palette.
    let seen: string | null | undefined;
    createRoot((dispose) => {
      // createComputed, not createEffect: a pure computation re-runs
      // SYNCHRONOUSLY inside the write, so it observes the DOM exactly as
      // applyTheme left it. An effect is queued and would see the settled
      // state either way.
      createComputed(() => {
        theme();
        seen = document.documentElement.getAttribute('data-theme');
      });
      seen = undefined;
      applyTheme('light');
      dispose();
    });
    expect(seen).toBe('light');
  });

  it('falls back to dark for a value it does not know', () => {
    localStorage.setItem('crucible:theme', 'solarized');
    expect(readTheme()).toBe('dark');
  });
});
