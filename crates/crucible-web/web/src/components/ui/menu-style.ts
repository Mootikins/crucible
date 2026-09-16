/**
 * Shared visual vocabulary for every Ark menu — ONE set of classes so the file
 * tree's context menu, the sessions tree's and the titlebar's project menu read
 * identically. The strings were byte-identical in three files before this;
 * `tree-style.ts` is the same idea for tree-ish surfaces.
 */

/**
 * The floating panel.
 *
 * Border and shadow are deliberately in proportion. This was a `border-hairline`
 * (the FAINTEST rule in the palette) under `shadow-lg`, whose blur is 20px — a
 * 20:1 ratio of soft to hard, which is the shape of a panel that has been given
 * a big drop shadow to make up for an edge that does not read. On a near-black
 * canvas the shadow did nothing anyway; on the light theme it did too much.
 *
 * The edge does the separating now (`hairline-strong`, the same rule the rest
 * of the shell uses where an edge must be seen) and the shadow only lifts:
 * `shadow-md` is 8px, so the ratio is 8:1.
 */
export const menuContent =
  'focus-ring min-w-[10rem] rounded border border-hairline-strong bg-surface-elevated py-1 text-xs text-shell-ink shadow-md';

/** One row. Ark stamps `data-highlighted` on the keyboard/pointer cursor. */
export const menuItem =
  'flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash';

/**
 * A kebab trigger that opens one of these menus. No border: the icon alone
 * marks the hit target, so the trigger stays quiet until hover or focus.
 */
export const menuTrigger =
  'inline-flex items-center justify-center h-6 w-6 rounded text-muted hover:text-shell-ink hover:bg-hover-wash transition-colors';

/** Hairline between groups of rows. */
export const menuSeparator = 'my-1 border-t border-hairline';
