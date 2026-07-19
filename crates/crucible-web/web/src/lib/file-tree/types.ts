/**
 * The one canonical frontend node type for the file-tree explorer. camelCase
 * (TS idiom) and shaped to satisfy ark-ui's `TreeCollection` (via `children`
 * + a `nodeToValue` that reads `relPath`). Wire types (`FsEntry`, `NoteEntry`)
 * stay snake_case; the kiln/project builders map wire -> node.
 */

/** PHASE-2/3 SEAM — git/diff decoration. No producer in Phase 1 (always undefined). */
export type FileNodeStatus = 'modified' | 'added' | 'deleted' | 'untracked' | 'conflicted';

export interface FileTreeNode {
  /**
   * Stable within-tree identity AND the ark-ui `value` (via `nodeToValue`).
   * Root-relative POSIX path: `''` for the synthetic root; `'Meta'`;
   * `'Meta/Systems.md'`.
   */
  relPath: string;
  /** Last path segment. */
  name: string;
  isDir: boolean;
  /**
   * ABSOLUTE fs path. Leaf identity handed to `openFileInEditor()`. `''` for
   * the synthetic root.
   */
  absPath: string;
  /**
   * Lazy children. `undefined` = NOT loaded (project dirs, so ark-ui
   * `loadChildren` fires on first expand); `[]` = loaded-but-empty; kiln trees
   * are built whole, so every kiln dir has a concrete array.
   */
  children?: FileTreeNode[];
  /**
   * Epoch seconds — the "modified" sort axis. Kiln leaf:
   * `Date.parse(updated_at)/1000`; dir: `max(child modified)`; project:
   * `fs.list_dir.modified`. `undefined` when unknown.
   */
  modified?: number;
  /**
   * PHASE-2/3 SEAM. Always `undefined` in Phase 1 — rows may READ it (render
   * nothing when undefined) but no P1 code SETS it.
   */
  status?: FileNodeStatus;
}

/** `'created'` is NOT available in Phase 1 (neither list_notes nor fs.list_dir report it). */
export type SortKey = 'name' | 'modified';
export type SortDir = 'asc' | 'desc';
export interface SortSpec {
  key: SortKey;
  dir: SortDir;
}
