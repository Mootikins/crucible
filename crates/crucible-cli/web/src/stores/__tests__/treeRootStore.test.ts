import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import type { TreeRoot } from '@/lib/tree-root';
import { selectedRoot, selectedRootKey, treeRootActions, TREE_ROOT_STORAGE_KEY } from '../treeRootStore';

const KILN: TreeRoot = { kind: 'kiln', path: '/vault', name: 'Vault' };

describe('treeRootStore', () => {
  beforeEach(() => {
    localStorage.clear();
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('selectRoot persists the rootKey and updates the signals', () => {
    treeRootActions.selectRoot(KILN);
    expect(selectedRoot()).toEqual(KILN);
    expect(selectedRootKey()).toBe('kiln:/vault');
    expect(localStorage.getItem(TREE_ROOT_STORAGE_KEY)).toBe('kiln:/vault');
  });

  it('setSelectedRootKey updates the key WITHOUT persisting (silent restore)', () => {
    // Assert "does not write" by comparing before/after rather than assuming a
    // pristine store — robust under shared-worker localStorage.
    const before = localStorage.getItem(TREE_ROOT_STORAGE_KEY);
    treeRootActions.setSelectedRootKey('project:/p');
    expect(selectedRootKey()).toBe('project:/p');
    expect(localStorage.getItem(TREE_ROOT_STORAGE_KEY)).toBe(before);
  });

  it('survives a throwing localStorage (private mode) and still updates in-memory', () => {
    const setSpy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('QuotaExceeded');
    });
    expect(() => treeRootActions.selectRoot(KILN)).not.toThrow();
    expect(selectedRoot()).toEqual(KILN);
    expect(selectedRootKey()).toBe('kiln:/vault');
    setSpy.mockRestore();
  });

  // Only this test needs a fresh module load (module-level `load()` reads
  // localStorage once at import). Isolate the reset here to avoid a
  // per-test re-transform that starves under concurrent load.
  it('seeds the selected key from pre-existing localStorage on import', async () => {
    localStorage.setItem(TREE_ROOT_STORAGE_KEY, 'kiln:/seeded');
    vi.resetModules();
    const fresh = await import('../treeRootStore');
    expect(fresh.selectedRootKey()).toBe('kiln:/seeded');
  });
});
