import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import type { TreeRoot } from '@/lib/tree-root';
import { pinnedRootKey, treeRootActions, TREE_ROOT_STORAGE_KEY } from '../treeRootStore';

const KILN: TreeRoot = { kind: 'kiln', path: '/vault', name: 'Vault' };
const DOCS: TreeRoot = { kind: 'kiln', path: '/docs', name: 'Docs' };

describe('treeRootStore', () => {
  beforeEach(() => {
    localStorage.clear();
    treeRootActions.unpin('s-1');
    treeRootActions.unpin('s-2');
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('pins per session and persists the map', () => {
    treeRootActions.pin('s-1', KILN);
    expect(pinnedRootKey('s-1')).toBe('kiln:/vault');
    expect(JSON.parse(localStorage.getItem(TREE_ROOT_STORAGE_KEY)!)).toEqual({
      's-1': 'kiln:/vault',
    });
  });

  // The whole reason the pin is keyed by session: one global pin stops the
  // tree following for every session at once.
  it('keeps one session’s pin out of another’s', () => {
    treeRootActions.pin('s-1', KILN);
    treeRootActions.pin('s-2', DOCS);
    expect(pinnedRootKey('s-1')).toBe('kiln:/vault');
    expect(pinnedRootKey('s-2')).toBe('kiln:/docs');
  });

  it('reports no pin for an unpinned or unknown session', () => {
    treeRootActions.pin('s-1', KILN);
    expect(pinnedRootKey('s-2')).toBeNull();
    expect(pinnedRootKey(null)).toBeNull();
    expect(pinnedRootKey(undefined)).toBeNull();
  });

  it('unpin returns the session to following', () => {
    treeRootActions.pin('s-1', KILN);
    treeRootActions.unpin('s-1');
    expect(pinnedRootKey('s-1')).toBeNull();
  });

  it('prune forgets pins for sessions that no longer exist', () => {
    treeRootActions.pin('s-1', KILN);
    treeRootActions.pin('s-2', DOCS);
    treeRootActions.prune(['s-2']);
    expect(pinnedRootKey('s-1')).toBeNull();
    expect(pinnedRootKey('s-2')).toBe('kiln:/docs');
  });

  // A failed session fetch reports an empty list. Pruning against it would
  // wipe every pin the user has — treat empty as "not loaded", not "none".
  it('prune treats an empty session list as unknown and keeps every pin', () => {
    treeRootActions.pin('s-1', KILN);
    treeRootActions.prune([]);
    expect(pinnedRootKey('s-1')).toBe('kiln:/vault');
  });

  it('survives a throwing localStorage (private mode) and still updates in-memory', () => {
    const setSpy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('QuotaExceeded');
    });
    expect(() => treeRootActions.pin('s-1', KILN)).not.toThrow();
    expect(pinnedRootKey('s-1')).toBe('kiln:/vault');
    setSpy.mockRestore();
  });

  // Only these need a fresh module load (module-level `load()` reads
  // localStorage once at import).
  it('seeds pins from pre-existing localStorage on import', async () => {
    localStorage.setItem(TREE_ROOT_STORAGE_KEY, JSON.stringify({ 's-9': 'kiln:/seeded' }));
    vi.resetModules();
    const fresh = await import('../treeRootStore');
    expect(fresh.pinnedRootKey('s-9')).toBe('kiln:/seeded');
  });

  it('ignores malformed storage instead of letting a stray value reach a comparison', async () => {
    localStorage.setItem(TREE_ROOT_STORAGE_KEY, JSON.stringify({ 's-9': 42, 's-8': 'kiln:/ok' }));
    vi.resetModules();
    const fresh = await import('../treeRootStore');
    expect(fresh.pinnedRootKey('s-9')).toBeNull();
    expect(fresh.pinnedRootKey('s-8')).toBe('kiln:/ok');
  });

  it('drops the pre-session global preference on load', async () => {
    localStorage.setItem('crucible:treeRoot', 'kiln:/old-global');
    vi.resetModules();
    await import('../treeRootStore');
    expect(localStorage.getItem('crucible:treeRoot')).toBeNull();
  });
});
