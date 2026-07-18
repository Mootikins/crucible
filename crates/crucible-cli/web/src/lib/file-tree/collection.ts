import { createTreeCollection, type TreeCollection } from '@ark-ui/solid';
import type { FileTreeNode, SortSpec } from './types';

/**
 * Wrap a synthetic root node (`relPath: ''`, `isDir: true`, `children: [...]`)
 * in an ark-ui `TreeCollection`. `TreeView` renders `root.children`; the
 * synthetic root is never shown.
 *
 * `nodeToValue` returns `relPath` — unique within one root's tree and the
 * identity everything (selection, focus, reveal, reconcile) keys on.
 * `nodeToString` drives type-ahead. Children are read from `node.children`
 * (the collection's default), so lazy project dirs (`children: undefined`)
 * report as branches with no loaded children.
 */
export function makeFileCollection(root: FileTreeNode): TreeCollection<FileTreeNode> {
  return createTreeCollection<FileTreeNode>({
    rootNode: root,
    nodeToValue: (n) => n.relPath,
    nodeToString: (n) => n.name,
    // Branch detection: zag treats a node as a branch when it has loaded
    // children OR a non-null children count. A lazily-loaded project dir has
    // `children: undefined` (0 loaded), so without this it would render as a
    // leaf and never fire `loadChildren`. Report a count for every dir (0 for
    // loaded-empty, 1 as an "unknown, has children" hint for unloaded);
    // `undefined` for files keeps them leaves.
    nodeToChildrenCount: (n) => (n.isDir ? (n.children?.length ?? 1) : undefined),
  });
}

/**
 * Pure, recursive sort. Folders ALWAYS precede files at every level; within a
 * group, sort by the chosen axis and break ties by name-asc for determinism.
 * Returns a NEW tree (input is never mutated). Lazy dirs (`children:
 * undefined`) pass through untouched so expansion still triggers a fetch.
 */
export function sortTree(root: FileTreeNode, s: SortSpec): FileTreeNode {
  const cmpName = (a: FileTreeNode, b: FileTreeNode) =>
    a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });

  const cmp = (a: FileTreeNode, b: FileTreeNode): number => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1; // folders first, always
    let primary =
      s.key === 'name' ? cmpName(a, b) : (a.modified ?? -1) - (b.modified ?? -1);
    if (s.dir === 'desc') primary = -primary;
    return primary !== 0 ? primary : cmpName(a, b); // stable tiebreak
  };

  const rec = (n: FileTreeNode): FileTreeNode =>
    n.children ? { ...n, children: [...n.children].sort(cmp).map(rec) } : n;

  return rec(root);
}
