/**
 * The mockup's answer to a wikilink: resolve the target in the docs kiln,
 * then a click opens the note and a hover floats it.
 */
import { KILN_PATHS } from '../data';
import { hoverEnd, hoverStart, openNote, whereFor } from '../actions';
import { basename } from '../components/path';
import { wikilinkEvents, type WikilinkEvents } from '../components/primitives/wikilinks';

/** A link target to a note path: an exact path first, else the first note with that name. */
export function resolveNote(target: string): string | null {
  const t = target.split('#')[0]!.trim().replace(/\.md$/, '');
  if (KILN_PATHS.includes(t)) return t;
  return KILN_PATHS.find((p) => basename(p).toLowerCase() === basename(t).toLowerCase()) ?? null;
}

/** The link handlers for a surface. A session surface opens a note as a peek while it covers the centre. */
export function noteLinks(fromSession: boolean): WikilinkEvents {
  return wikilinkEvents({
    open: (target, e) => {
      const path = resolveNote(target);
      if (path) openNote(path, { fromSession, where: whereFor(e) });
    },
    hover: (target, anchor) => {
      const path = resolveNote(target);
      if (!path) return;
      hoverStart(anchor, path);
      anchor.addEventListener('mouseleave', hoverEnd, { once: true });
    },
  });
}
