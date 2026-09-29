/**
 * A folder tree built from flat note paths. The real app does not build it
 * whole: it lists one folder at a time with `useListDir` (`lib/query/fs.ts`),
 * when the user opens that folder.
 */
export interface TreeNode {
  name: string;
  path: string;
  dirs: Map<string, TreeNode>;
  /** The full paths of the notes directly in this folder. */
  files: string[];
}

export function buildTree(paths: readonly string[]): TreeNode {
  const root: TreeNode = { name: '', path: '', dirs: new Map(), files: [] };
  for (const p of paths) {
    const parts = p.split('/');
    let n = root;
    parts.slice(0, -1).forEach((part, k) => {
      if (!n.dirs.has(part)) n.dirs.set(part, { name: part, path: parts.slice(0, k + 1).join('/'), dirs: new Map(), files: [] });
      n = n.dirs.get(part)!;
    });
    n.files.push(p);
  }
  return root;
}
