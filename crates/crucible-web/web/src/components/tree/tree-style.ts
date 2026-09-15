/**
 * Shared tree/list visual vocabulary — ONE set of classes for every tree-ish
 * surface (files tree, sessions tree, picker popouts), so rows, section
 * headers, and chips read identically across panels and match the composer
 * splash idiom. Components stay free to append surface-specific classes
 * (selection tints, drop highlights) on top.
 */

/** Uppercase section/group header (popout groups, tree group labels). */
export const treeSectionHeader =
  'px-3 pt-2 pb-1 text-floor font-semibold uppercase tracking-wide text-muted-dark';

/** Collapsible group header row (chevron + name), one step quieter than rows. */
export const treeGroupRow =
  'w-full flex items-center gap-1 px-2 py-1 rounded cursor-pointer hover:bg-hover-wash text-muted text-xs font-medium';

/** Chevron that rotates open — pair with data-[state=open]:rotate-90 stamps
 * or an explicit rotate class toggle. */
export const treeChevron = 'w-3.5 h-3.5 shrink-0 transition-transform duration-150';

/**
 * A clickable tree/list row. It carries NO metrics of its own: row height,
 * text size, icon slot and indent step come from `styles/refine-touch.css`,
 * which reads the nearest `data-density` ancestor. A surface therefore
 * changes every row it owns with one attribute instead of a second class set.
 *
 * Do not put a Tailwind `text-*` class on the row or on the text inside it.
 * Such a class pins the size and the density attribute stops reaching it.
 */
export const treeRow = 'tree-row';

/** The fixed icon column at the head of a row (chevron or filetype icon). */
export const treeIconSlot = 'tree-icon-slot';

