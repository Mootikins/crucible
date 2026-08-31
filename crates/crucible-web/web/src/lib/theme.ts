/**
 * Light and dark, as one attribute on the document root.
 *
 * The palette is a set of `--color-*` custom properties that Tailwind's
 * `@theme` block defines on `:root`; a light theme re-declares the same names
 * under `:root[data-theme='light']`. So every component that already says
 * `bg-shell-bg` or `text-muted` follows along with no change — the alternative,
 * a `dark:` variant on every class in the app, would need touching every file
 * and would still miss the inline `var(--color-…)` uses.
 */
import { createSignal } from 'solid-js';

export type Theme = 'dark' | 'light';

const KEY = 'crucible:theme';

/** The shell's own identity, and the answer when nothing else has one — no
 *  stored choice AND no OS preference. */
export const DEFAULT_THEME: Theme = 'dark';

/**
 * The live theme, as a signal.
 *
 * `document.documentElement` is not reactive, and four things cannot follow a
 * CSS custom property at all: CodeMirror compiles its syntax colors into a
 * StyleModule, xterm takes a color object, the graph paints literal colors to
 * a canvas, and the typography plugin's `prose-invert` is a CLASS, not a
 * variable. They read THIS instead, and reconfigure when it changes.
 */
const [theme, setThemeSignal] = createSignal<Theme>(DEFAULT_THEME);
export { theme };

/**
 * The theme the user chose, or null when the user has not chosen one.
 *
 * Null is the load-bearing case: it is the ONLY state in which the OS gets a
 * vote. Anything unrecognised in storage counts as no choice.
 */
export function storedTheme(): Theme | null {
  try {
    const value = localStorage.getItem(KEY);
    return value === 'light' || value === 'dark' ? value : null;
  } catch {
    // Private mode: reads throw as readily as writes.
    return null;
  }
}

/**
 * What the operating system asks for.
 *
 * The query is for LIGHT, not for dark. `prefers-color-scheme: dark` also
 * matches `no-preference` on some engines, and the two are not the same
 * question; asking for light and falling back keeps `no-preference` on the
 * shell's own identity, which is what `:root` already paints.
 */
export function systemTheme(): Theme {
  try {
    return window.matchMedia?.('(prefers-color-scheme: light)').matches
      ? 'light'
      : DEFAULT_THEME;
  } catch {
    // matchMedia is absent in some test and embedded runtimes.
    return DEFAULT_THEME;
  }
}

/**
 * The theme to paint: the user's choice, else the OS preference.
 *
 * The stored choice ALWAYS wins. A visitor who deliberately picked dark on a
 * light-set machine must keep dark across reloads, so the OS is consulted only
 * when `storedTheme()` is null.
 */
export function readTheme(): Theme {
  return storedTheme() ?? systemTheme();
}

/**
 * Paint the resolved theme at boot and keep following the OS after it.
 *
 * It PAINTS, it does not persist. `applyTheme(readTheme())` was the obvious
 * spelling and it is wrong: the first load would write its own fallback into
 * storage, which is an explicit choice the user never made — and from then on
 * `storedTheme()` is non-null, so the OS is never consulted again. The stored
 * value must be written by a deliberate toggle and by nothing else.
 *
 * Returns the watcher's disposer.
 */
export function initTheme(): () => void {
  paintTheme(readTheme());
  return watchSystemTheme();
}

/**
 * Follow the OS while the user has expressed no preference.
 *
 * The listener re-checks `storedTheme()` on every change rather than at
 * subscribe time: the user can pick a theme after this is armed, and from that
 * moment the OS must stop moving the app. Returns its own disposer.
 */
export function watchSystemTheme(): () => void {
  let query: MediaQueryList;
  try {
    query = window.matchMedia('(prefers-color-scheme: light)');
  } catch {
    return () => {};
  }
  const onChange = () => {
    if (storedTheme() !== null) return;
    // Paint WITHOUT persisting — writing here would turn a passive OS follow
    // into an explicit choice and freeze the app on it forever.
    paintTheme(systemTheme());
  };
  query.addEventListener('change', onChange);
  return () => query.removeEventListener('change', onChange);
}

/**
 * Paint the theme and remember it.
 *
 * Dark writes NO attribute rather than `data-theme="dark"`: the dark palette
 * is the bare `:root` declaration, so an absent attribute and a dark one must
 * mean the same thing — and a page that has not run this yet is already dark.
 */
export function applyTheme(theme: Theme): void {
  paintTheme(theme);
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    /* private mode */
  }
}

/** Paint without remembering. Split out for `watchSystemTheme`, which follows
 *  the OS and must NOT record that as the user's own choice. */
function paintTheme(theme: Theme): void {
  const root = document.documentElement;
  if (theme === 'light') root.setAttribute('data-theme', 'light');
  else root.removeAttribute('data-theme');
  setThemeSignal(theme);
}
