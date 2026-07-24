/**
 * Shared tree/list visual vocabulary — ONE set of classes for every tree-ish
 * surface (files tree, sessions tree, picker popouts), so rows, section
 * headers, and chips read identically across panels and match the composer
 * splash idiom. Components stay free to append surface-specific classes
 * (selection tints, drop highlights) on top.
 */

/** Uppercase section/group header (popout groups, tree group labels). */
export const treeSectionHeader =
  'px-3 pt-2 pb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-dark';

/** Base interactive row: quiet, rounded, hover-washed. */
export const treeRow =
  'flex items-center pr-2 py-1 rounded cursor-pointer hover:bg-hover-wash text-shell-body text-sm';

/** Collapsible group header row (chevron + name), one step quieter than rows. */
export const treeGroupRow =
  'w-full flex items-center gap-1 px-2 py-1 rounded cursor-pointer hover:bg-hover-wash text-muted text-xs font-medium';

/** Chevron that rotates open — pair with data-[state=open]:rotate-90 stamps
 * or an explicit rotate class toggle. */
export const treeChevron = 'w-3.5 h-3.5 shrink-0 transition-transform duration-150';

/** Small metadata chip on a row (branch, kiln, model). */
export const treeChip =
  'inline-flex items-center gap-1 text-[10px] text-muted px-1.5 py-0.5 rounded bg-surface-elevated border border-hairline max-w-[160px]';
