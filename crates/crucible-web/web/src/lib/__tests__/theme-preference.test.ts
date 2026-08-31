import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { DEFAULT_THEME, applyTheme, initTheme, readTheme, storedTheme, systemTheme, theme, watchSystemTheme } from '@/lib/theme';

/**
 * Nothing read the OS colour scheme. `data-theme` is absent for dark, so dark
 * was the attribute-absent default and a visitor whose machine is set to light
 * got dark until they found the ribbon toggle.
 *
 * The rule that matters, and the one these tests exist to hold: an explicit
 * stored choice ALWAYS beats the OS. A user who deliberately picked dark on a
 * light-set machine keeps dark.
 */

type Listener = () => void;

/** A `matchMedia` whose answer we can move, with the listeners it handed out. */
function installMatchMedia(prefersLight: boolean) {
  const listeners = new Set<Listener>();
  let matches = prefersLight;
  const query = {
    get matches() {
      return matches;
    },
    media: '(prefers-color-scheme: light)',
    addEventListener: (_: string, fn: Listener) => void listeners.add(fn),
    removeEventListener: (_: string, fn: Listener) => void listeners.delete(fn),
  };
  const impl = vi.fn((media: string) => {
    // The app must ask for LIGHT. `prefers-color-scheme: dark` also matches
    // `no-preference` on some engines, which is a different question.
    expect(media).toBe('(prefers-color-scheme: light)');
    return query as unknown as MediaQueryList;
  });
  vi.stubGlobal('matchMedia', impl);
  return {
    impl,
    set(next: boolean) {
      matches = next;
      for (const fn of [...listeners]) fn();
    },
    listenerCount: () => listeners.size,
  };
}

describe('theme preference resolution', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
    applyTheme(DEFAULT_THEME);
    localStorage.clear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('reads the OS when the user has chosen nothing', () => {
    installMatchMedia(true);
    expect(storedTheme()).toBeNull();
    expect(systemTheme()).toBe('light');
    expect(readTheme()).toBe('light');
  });

  it('stays on the shell’s own identity when the OS asks for dark', () => {
    installMatchMedia(false);
    expect(readTheme()).toBe(DEFAULT_THEME);
  });

  it('an explicit stored choice BEATS the OS, both ways', () => {
    // The regression that actually bites: pick dark on a light machine and the
    // choice must survive every reload.
    installMatchMedia(true);
    applyTheme('dark');
    expect(storedTheme()).toBe('dark');
    expect(systemTheme()).toBe('light');
    expect(readTheme()).toBe('dark');

    // And the mirror: pick light on a dark machine.
    localStorage.clear();
    installMatchMedia(false);
    applyTheme('light');
    expect(readTheme()).toBe('light');
  });

  it('treats a value it does not know as NO choice, so the OS gets the vote', () => {
    localStorage.setItem('crucible:theme', 'solarized');
    installMatchMedia(true);
    expect(storedTheme()).toBeNull();
    expect(readTheme()).toBe('light');
  });

  it('falls back to dark where matchMedia does not exist', () => {
    vi.stubGlobal('matchMedia', undefined);
    expect(systemTheme()).toBe(DEFAULT_THEME);
    expect(readTheme()).toBe(DEFAULT_THEME);
  });

  it('boot PAINTS the resolved theme without recording it as a choice', () => {
    // `applyTheme(readTheme())` is the obvious spelling and it is wrong: the
    // first load would write its own fallback into storage, and from then on
    // the OS is never consulted again.
    const media = installMatchMedia(true);
    const dispose = initTheme();
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    expect(theme()).toBe('light');
    expect(localStorage.getItem('crucible:theme')).toBeNull();
    expect(media.listenerCount()).toBe(1);
    dispose();
    expect(media.listenerCount()).toBe(0);
  });

  it('follows the OS while no choice is stored', () => {
    const media = installMatchMedia(false);
    const dispose = initTheme();
    expect(theme()).toBe('dark');

    media.set(true);
    expect(theme()).toBe('light');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    // Following is not choosing.
    expect(localStorage.getItem('crucible:theme')).toBeNull();
    dispose();
  });

  it('STOPS following the OS the moment the user chooses', () => {
    const media = installMatchMedia(false);
    const dispose = watchSystemTheme();

    applyTheme('dark');
    media.set(true);

    expect(theme()).toBe('dark');
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
    dispose();
  });
});
