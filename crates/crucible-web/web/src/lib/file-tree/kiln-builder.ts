import type { NoteEntry } from '@/lib/types';
import { noteAbsolutePath } from '@/lib/note-actions';
import type { FileTreeNode } from './types';

const toEpoch = (iso?: string): number | undefined => {
  if (!iso) return undefined;
  const t = Date.parse(iso);
  return Number.isNaN(t) ? undefined : Math.floor(t / 1000);
};

/**
 * Build the WHOLE kiln tree client-side from `list_notes` output — no lazy
 * loading, no per-folder endpoint. Each `NoteEntry.path` is kiln-relative with
 * folder segments (e.g. `Meta/Systems.md`), split on `/` to nest.
 *
 * Returns a synthetic ROOT node (`relPath: ''`) whose `children` are the nested
 * tree. Directory `modified` is the max of descendant leaf `modified`.
 *
 * Deterministic collision rules (unit-tested):
 *  - duplicate identical paths -> first wins;
 *  - a path appearing as both a file and a directory prefix -> directory wins
 *    (the leaf is dropped);
 *  - same-stem files in different folders -> distinct nodes under distinct
 *    parents.
 *
 * Documented limitation: empty kiln directories are invisible — `list_notes`
 * returns files only.
 */
export function notesToTree(notes: NoteEntry[], kilnAbsRoot: string): FileTreeNode {
  const rootChildren: FileTreeNode[] = [];
  const dirIndex = new Map<string, FileTreeNode>();

  for (const note of notes) {
    const segments = note.path.replace(/^\.?\//, '').split('/').filter(Boolean);
    if (segments.length === 0) continue;

    let parentChildren = rootChildren;
    let relPrefix = '';
    for (let i = 0; i < segments.length - 1; i++) {
      relPrefix = relPrefix ? `${relPrefix}/${segments[i]}` : segments[i];
      let dir = dirIndex.get(relPrefix);
      if (!dir) {
        // A path first recorded as a leaf now turns out to be a directory
        // prefix — directory wins, drop the stale leaf (order-independent).
        const staleLeaf = parentChildren.findIndex((n) => !n.isDir && n.relPath === relPrefix);
        if (staleLeaf >= 0) parentChildren.splice(staleLeaf, 1);
        dir = {
          relPath: relPrefix,
          name: segments[i],
          isDir: true,
          absPath: noteAbsolutePath(relPrefix, kilnAbsRoot),
          children: [],
        };
        dirIndex.set(relPrefix, dir);
        parentChildren.push(dir);
      }
      parentChildren = dir.children!;
    }

    const leafRel = segments.join('/');
    if (dirIndex.has(leafRel)) continue; // a directory already owns this path
    if (parentChildren.some((n) => !n.isDir && n.relPath === leafRel)) continue; // duplicate leaf

    parentChildren.push({
      relPath: leafRel,
      name: segments[segments.length - 1],
      isDir: false,
      absPath: noteAbsolutePath(note.path, kilnAbsRoot),
      modified: toEpoch(note.updated_at),
    });
  }

  aggregateDirModified(rootChildren);
  return { relPath: '', name: '', isDir: true, absPath: kilnAbsRoot, children: rootChildren };
}

/** Post-order: each dir's `modified` becomes the max of its descendants' leaf `modified`. */
function aggregateDirModified(nodes: FileTreeNode[]): number | undefined {
  let max: number | undefined;
  for (const n of nodes) {
    const t = n.isDir ? aggregateDirModified(n.children!) : n.modified;
    if (t !== undefined && (max === undefined || t > max)) max = t;
    if (n.isDir) n.modified = t;
  }
  return max;
}
