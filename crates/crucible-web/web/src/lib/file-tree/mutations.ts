/**
 * Pure helpers for tree mutations (rename / new note), kept out of the
 * component so the naming rules are unit-testable.
 */

/** Parent rel-path of a rel-path ('' for root-level entries). */
export function parentRel(rel: string): string {
  const i = rel.lastIndexOf('/');
  return i === -1 ? '' : rel.slice(0, i);
}

/**
 * The rel-path a rename produces. Markdown notes keep their extension when
 * the typed label has none — renaming `a.md` to `b` must not silently turn a
 * note into a non-note (it would fall out of the index and break links).
 */
export function renamedRel(oldRel: string, newLabel: string): string {
  const label = newLabel.trim();
  const keepMd = oldRel.endsWith('.md') && !label.includes('.');
  const name = keepMd ? `${label}.md` : label;
  const parent = parentRel(oldRel);
  return parent ? `${parent}/${name}` : name;
}

/** True when a rename label is usable as a single path segment. */
export function isValidName(label: string): boolean {
  const l = label.trim();
  return l.length > 0 && !l.includes('/') && !l.includes('\\') && l !== '.' && l !== '..';
}
