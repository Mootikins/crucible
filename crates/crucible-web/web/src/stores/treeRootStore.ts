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
const [selectedRoot, setSelectedRoot] = createSignal<TreeRoot | null>(null);

export { selectedRootKey, selectedRoot };

export const treeRootActions = {
  /** User picked a root: update signals AND persist the key. */
  selectRoot(root: TreeRoot) {
    setSelectedRoot(root);
    setKey(rootKey(root));
    try {
      localStorage.setItem(TREE_ROOT_STORAGE_KEY, rootKey(root));
    } catch {
      /* private mode: in-memory only */
    }
  },
  /** Silent restore of a derived/fallback key — does NOT persist. */
  setSelectedRootKey(key: string | null) {
    setKey(key);
  },
};
