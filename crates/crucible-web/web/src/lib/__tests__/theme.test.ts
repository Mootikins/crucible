import { describe, it, expect, beforeEach } from 'vitest';
import { DEFAULT_THEME, applyTheme, readTheme } from '@/lib/theme';

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

  it('falls back to dark for a value it does not know', () => {
    localStorage.setItem('crucible:theme', 'solarized');
    expect(readTheme()).toBe('dark');
  });
});
