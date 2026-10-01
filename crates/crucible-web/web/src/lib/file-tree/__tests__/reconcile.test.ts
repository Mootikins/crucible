import { describe, it, expect, vi } from 'vitest';
import {
  locate,
  reconcileMount,
  foldersToInvalidate,
  createFsEventBatcher,
  type RootMount,
} from '../reconcile';
import type { FileTreeNode } from '../types';

const leaf = (rel: string, base: string): FileTreeNode => ({
  relPath: rel,
  name: rel.split('/').pop()!,
  isDir: false,
  absPath: `${base}/${rel}`,
});
const dir = (rel: string, base: string, children: FileTreeNode[] | undefined): FileTreeNode => ({
  relPath: rel,
  name: rel.split('/').pop()!,
  isDir: true,
  absPath: `${base}/${rel}`,
  children,
});
const root = (base: string, children: FileTreeNode[]): FileTreeNode => ({
  relPath: '',
  name: '',
  isDir: true,
  absPath: base,
  children,
});


describe('locate', () => {
  it('picks the owning mount and returns root-relative parts', () => {
    const mounts: RootMount[] = [
      { rootId: 'k', kind: 'kiln', basePath: '/vault', root: root('/vault', []) },
      { rootId: 'p', kind: 'project', basePath: '/proj', root: root('/proj', []) },
    ];
    expect(locate(mounts, '/vault/Meta/Systems.md')).toEqual({
      rootId: 'k',
      relParts: ['Meta', 'Systems.md'],
    });
    expect(locate(mounts, '/proj/src/a.ts')).toEqual({ rootId: 'p', relParts: ['src', 'a.ts'] });
  });

  it('returns null outside every basePath and does not match a path-prefix sibling', () => {
    const mounts: RootMount[] = [
      { rootId: 'k', kind: 'kiln', basePath: '/vault', root: root('/vault', []) },
    ];
    expect(locate(mounts, '/other/x.md')).toBeNull();
    expect(locate(mounts, '/vault2/x.md')).toBeNull(); // /vault is not a segment prefix
  });
});

describe('foldersToInvalidate', () => {
  const PROJ = '/proj';
  // src is loaded (children defined); node_modules is NOT loaded (undefined).
  const projRoot = root(PROJ, [
    dir('src', PROJ, [leaf('src/a.ts', PROJ)]),
    dir('node_modules', PROJ, undefined),
  ]);
  const mount: RootMount = { rootId: 'p', kind: 'project', basePath: PROJ, root: projRoot };

  it('returns the parent folder of a change when it is loaded', () => {
    const out = foldersToInvalidate(mount, [
      { type: 'changed', path: '/proj/src/b.ts', kind: 'created' },
    ]);
    expect(out).toEqual(['src']);
  });

  it('returns [] when the parent folder is unloaded or absent', () => {
    const unloaded = foldersToInvalidate(mount, [
      { type: 'changed', path: '/proj/node_modules/x/index.js', kind: 'modified' },
    ]);
    expect(unloaded).toEqual([]);
    const absent = foldersToInvalidate(mount, [
      { type: 'deleted', path: '/proj/nowhere/y.ts' },
    ]);
    expect(absent).toEqual([]);
  });

  it('a top-level change invalidates the root ("")', () => {
    const out = foldersToInvalidate(mount, [
      { type: 'changed', path: '/proj/README.md', kind: 'created' },
    ]);
    expect(out).toEqual(['']);
  });

  it('moved dedupes both endpoints parents', () => {
    const out = foldersToInvalidate(mount, [
      { type: 'moved', from: '/proj/src/a.ts', to: '/proj/src/b.ts' },
    ]);
    expect(out).toEqual(['src']);
  });
});

describe('createFsEventBatcher', () => {
  it('coalesces a burst within flushMs into one onFlush call', () => {
    vi.useFakeTimers();
    const onFlush = vi.fn();
    const b = createFsEventBatcher(150, onFlush);
    b.push({ type: 'changed', path: '/vault/a.md', kind: 'created' });
    b.push({ type: 'changed', path: '/vault/b.md', kind: 'created' });
    expect(onFlush).not.toHaveBeenCalled();
    vi.advanceTimersByTime(150);
    expect(onFlush).toHaveBeenCalledTimes(1);
    expect(onFlush.mock.calls[0][0]).toHaveLength(2);
    vi.useRealTimers();
  });

  it('flush() emits pending immediately and clears them', () => {
    vi.useFakeTimers();
    const onFlush = vi.fn();
    const b = createFsEventBatcher(150, onFlush);
    b.push({ type: 'deleted', path: '/vault/a.md' });
    b.flush();
    expect(onFlush).toHaveBeenCalledTimes(1);
    b.flush(); // nothing pending
    expect(onFlush).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});


it('invalidates kiln listings for empty directories and non-Markdown assets', () => {
  const mount: RootMount = { rootId: 'k', kind: 'kiln', basePath: '/vault', root: root('/vault', []) };
  expect(reconcileMount(mount, [
    { type: 'changed', path: '/vault/empty', kind: 'created' },
    { type: 'changed', path: '/vault/image.png', kind: 'created' },
  ])).toEqual({ invalidate: [''] });
});
