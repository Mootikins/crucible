/**
 * Which shell this page draws: the window manager, or the compact shell.
 *
 * The page decides ONCE, at load, and never again. Two facts force this. The
 * desktop layout loads once, when the window manager mounts, so a swap on a
 * resize would mount it with no layout. And the compact shell must never touch
 * that layout, because it lives on the daemon and every desktop shares it. A
 * reload decides again. See `docs/Meta/Architecture/Mobile Shell.md`, section 3.
 */

/** The width below which a phone-shaped viewport gets the compact shell. */
export const COMPACT_QUERY = '(max-width: 767px)';

type MatchMedia = (query: string) => MediaQueryList;

/**
 * Whether `matchMedia` reports a compact viewport.
 *
 * Some test and embedded runtimes have no `matchMedia`, and some throw on an
 * unknown query. Both fall back to the desktop shell, which is the shell every
 * such runtime was built against.
 */
export function detectCompact(
  matchMedia: MatchMedia | undefined = typeof window !== 'undefined'
    ? window.matchMedia?.bind(window)
    : undefined,
): boolean {
  if (typeof matchMedia !== 'function') return false;
  try {
    return matchMedia(COMPACT_QUERY).matches;
  } catch {
    return false;
  }
}

const compact = detectCompact();

/** The shell this page chose at load. Constant for the life of the page. */
export function isCompact(): boolean {
  return compact;
}
