/**
 * Shared visual vocabulary for every Ark menu — ONE set of classes so the file
 * tree's context menu, the sessions tree's and the titlebar's project menu read
 * identically. The strings were byte-identical in three files before this;
 * `tree-style.ts` is the same idea for tree-ish surfaces.
 */

/** Flat floating surface above phone drawers; groups separate with spacing. */
export const menuContent =
  'z-[60] focus-ring min-w-[10rem] rounded-card shell-popup p-1 text-xs text-shell-ink';

/** One row. Ark stamps `data-highlighted` on the keyboard/pointer cursor. */
export const menuItem =
  'rounded-control flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash';

/**
 * A kebab trigger that opens one of these menus. No border: the icon alone
 * marks the hit target, so the trigger stays quiet until hover or focus.
 */
export const menuTrigger =
  'inline-flex items-center justify-center h-6 w-6 rounded-control text-muted hover:text-shell-ink hover:bg-hover-wash transition-colors';

/** Whitespace between groups of rows. */
export const menuSeparator = 'my-1 border-0';
