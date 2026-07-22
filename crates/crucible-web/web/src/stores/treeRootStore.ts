/**
 * App-global selected file-tree root, mirroring the module-level signal-store
 * idiom (notificationStore / statusBarStore). The PERSISTED value is the
 * `rootKey` string; the resolved `TreeRoot` is derived once the roster loads.
 */
import { createSignal } from 'solid-js';
import type { TreeRoot } from '@/lib/tree-root';
import { rootKey } from '@/lib/tree-root';

export const TREE_ROOT_STORAGE_KEY = 'crucible:treeRoot';

function load(): string | null {
  try {
    return localStorage.getItem(TREE_ROOT_STORAGE_KEY);
  } catch {
    return null; // private mode / storage disabled
  }
}

const [selectedRootKey, setKey] = createSignal<string | null>(load());

export { selectedRootKey };

export const treeRootActions = {
  /** User picked a root: update the signal AND persist the key. The key is a
   * PREFERENCE, not the display state — FilesPanel resolves it against the
   * loaded roster (with fallback) and binds the dropdown to the RESOLVED
   * root, so a stale key can never show a root the tree isn't rendering. */
  selectRoot(root: TreeRoot) {
    setKey(rootKey(root));
    try {
      localStorage.setItem(TREE_ROOT_STORAGE_KEY, rootKey(root));
    } catch {
      /* private mode: in-memory only */
    }
  },
};
