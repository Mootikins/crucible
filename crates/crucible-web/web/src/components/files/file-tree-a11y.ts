/**
 * Open-note highlight + lazy reveal helpers for the file tree. The "open note"
 * state is DERIVED from the window store (which file tab is active), not stored
 * on the tree — reading it reactively live-updates the accent with no tree
 * rebuild.
 */
import { windowStore } from '@/stores/windowStore';
import { tabHost } from '@/lib/tab-host';

/**
 * The absolute path of the file open in the active tab of any group, or `null`.
 * Matches `FileTreeNode.absPath`. A tab is a file tab when
 * `contentType === 'file'` and it carries `metadata.filePath`.
 */
export function currentOpenFilePath(): string | null {
  // The compact shell has one active tab and no groups, so the host answers
  // for it; the desktop scan below then finds nothing.
  const active = tabHost().activeTab();
  if (active?.contentType === 'file' && typeof active.metadata?.filePath === 'string') {
    return active.metadata.filePath;
  }
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
interface RevealApi {
  expand(value?: string[]): void;
  focus(value: string): void;
}

/** Lazy-reveal api: expand awaits child load before descending. */
export interface LazyRevealApi extends RevealApi {
  /** Resolves once the node's children have loaded (or immediately if already loaded). */
  onLoaded(value: string): Promise<void>;
}

/**
 * Reveal a path in a lazily-loaded filesystem tree: walk ancestors root->leaf,
 * expanding each and awaiting its children before descending. Stops silently on
 * a load failure.
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
