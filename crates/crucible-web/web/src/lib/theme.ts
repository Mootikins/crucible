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

/** The shell's own identity. Light is opt-in. */
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

export function readTheme(): Theme {
  try {
    return localStorage.getItem(KEY) === 'light' ? 'light' : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

/**
 * Paint the theme and remember it.
 *
 * Dark writes NO attribute rather than `data-theme="dark"`: the dark palette
 * is the bare `:root` declaration, so an absent attribute and a dark one must
 * mean the same thing — and a page that has not run this yet is already dark.
 */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  if (theme === 'light') root.setAttribute('data-theme', 'light');
  else root.removeAttribute('data-theme');
  setThemeSignal(theme);
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    /* private mode */
  }
}
