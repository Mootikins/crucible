/**
 * Shared visual vocabulary for every Ark menu — ONE set of classes so the file
 * tree's context menu, the sessions tree's and the titlebar's project menu read
 * identically. The strings were byte-identical in three files before this;
 * `tree-style.ts` is the same idea for tree-ish surfaces.
 */

/** The floating panel. */
export const menuContent =
  'min-w-[10rem] rounded border border-hairline bg-surface-elevated py-1 text-xs text-shell-ink shadow-lg focus:outline-none';

/** One row. Ark stamps `data-highlighted` on the keyboard/pointer cursor. */
export const menuItem =
  'flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash';

/** Hairline between groups of rows. */
export const menuSeparator = 'my-1 border-t border-hairline';
