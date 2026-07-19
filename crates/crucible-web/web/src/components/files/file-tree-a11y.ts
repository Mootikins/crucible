/**
 * Open-note highlight + lazy reveal helpers for the file tree. The "open note"
 * state is DERIVED from the window store (which file tab is active), not stored
 * on the tree — reading it reactively live-updates the accent with no tree
 * rebuild.
 */
import { windowStore } from '@/stores/windowStore';
import type { TreeCollection } from '@ark-ui/solid';
import type { FileTreeNode } from '@/lib/file-tree/types';

/**
 * The absolute path of the file open in the active tab of any group, or `null`.
 * Matches `FileTreeNode.absPath`. A tab is a file tab when
 * `contentType === 'file'` and it carries `metadata.filePath`.
 */
export function currentOpenFilePath(): string | null {
  for (const group of Object.values(windowStore.tabGroups)) {
    const active = group.tabs.find((t) => t.id === group.activeTabId);
    if (active?.contentType === 'file') {
      const filePath = active.metadata?.filePath;
      if (typeof filePath === 'string') return filePath;
    }
  }
  return null;
}

/** Minimal slice of the ark-ui TreeView api this helper drives. */
export interface RevealApi {
  expand(value?: string[]): void;
  focus(value: string): void;
}

/**
 * Reveal a fully-loaded path (kilns): expand every ancestor then focus the
 * target. `getIndexPath`/`getValuePath` come from the collection; ancestors are
 * `valuePath.slice(0, -1)`. No-op when the value is absent from the collection.
 */
export function revealLoadedPath(
  api: RevealApi,
  collection: TreeCollection<FileTreeNode>,
  targetRelPath: string,
): boolean {
  const indexPath = collection.getIndexPath(targetRelPath);
  if (!indexPath) return false;
  const valuePath = collection.getValuePath(indexPath);
  const ancestors = valuePath.slice(0, -1);
  if (ancestors.length > 0) api.expand(ancestors);
  api.focus(targetRelPath);
  return true;
}

/** Lazy-reveal api: expand awaits child load before descending. */
export interface LazyRevealApi extends RevealApi {
  /** Resolves once the node's children have loaded (or immediately if already loaded). */
  onLoaded(value: string): Promise<void>;
}

/**
 * Reveal a path in a lazily-loaded tree (projects): walk ancestors root->leaf,
 * expanding each and awaiting its children before descending. Stops silently on
 * a load failure. Wired in Phase 1, exercised only for project roots.
 */
export async function revealLazyPath(
  api: LazyRevealApi,
  targetRelPath: string,
): Promise<boolean> {
  const parts = targetRelPath.split('/').filter(Boolean);
  if (parts.length === 0) return false;

  let prefix = '';
  // Expand every ancestor (all but the final leaf segment).
  for (let i = 0; i < parts.length - 1; i++) {
    prefix = prefix ? `${prefix}/${parts[i]}` : parts[i];
    try {
      api.expand([prefix]);
      await api.onLoaded(prefix);
    } catch {
      return false;
    }
  }
  api.focus(targetRelPath);
  return true;
}
