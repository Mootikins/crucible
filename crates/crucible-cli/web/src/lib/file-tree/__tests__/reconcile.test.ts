import { describe, it, expect, vi } from 'vitest';
import {
  locate,
  reconcileKilnTree,
  projectFoldersToInvalidate,
  findNodeByRelPath,
  createFsEventBatcher,
  type RootMount,
} from '../reconcile';
import type { FileTreeNode } from '../types';
import type { FsEvent } from '@/lib/types';

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

const KILN = '/vault';
const names = (n: FileTreeNode | null) => (n?.children ?? []).map((c) => c.name);
const find = (r: FileTreeNode, rel: string) => findNodeByRelPath(r, rel);

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

describe('reconcileKilnTree', () => {
  it('adds a created markdown leaf, synthesizing missing dirs', () => {
    const tree = root(KILN, []);
    const ev: FsEvent = { type: 'changed', path: '/vault/Meta/New.md', kind: 'created' };
    const next = reconcileKilnTree(tree, KILN, [ev]);
    const meta = find(next, 'Meta')!;
    expect(meta.isDir).toBe(true);
    expect(names(meta)).toEqual(['New.md']);
    expect(find(next, 'Meta/New.md')!.absPath).toBe('/vault/Meta/New.md');
  });

  it('modified on an absent leaf adds it; on a present leaf keeps it (idempotent)', () => {
    const tree = root(KILN, []);
    const ev: FsEvent = { type: 'changed', path: '/vault/a.md', kind: 'modified' };
    const once = reconcileKilnTree(tree, KILN, [ev]);
    expect(names(once)).toEqual(['a.md']);
    const twice = reconcileKilnTree(once, KILN, [ev]);
    expect(names(twice)).toEqual(['a.md']);
  });

  it('deletes the exact leaf but keeps the (now empty) ancestor dir', () => {
    const tree = root(KILN, [dir('Meta', KILN, [leaf('Meta/S.md', KILN)])]);
    const ev: FsEvent = { type: 'deleted', path: '/vault/Meta/S.md' };
    const next = reconcileKilnTree(tree, KILN, [ev]);
    expect(find(next, 'Meta')!.isDir).toBe(true);
    expect(names(find(next, 'Meta'))).toEqual([]);
  });

  it('ignores non-markdown events', () => {
    const tree = root(KILN, []);
    const ev: FsEvent = { type: 'changed', path: '/vault/image.png', kind: 'created' };
    const next = reconcileKilnTree(tree, KILN, [ev]);
    expect(names(next)).toEqual([]);
  });

  it('moved is decomposed into remove(from) + add(to)', () => {
    const tree = root(KILN, [leaf('a.md', KILN)]);
    const ev: FsEvent = { type: 'moved', from: '/vault/a.md', to: '/vault/b.md' };
    const next = reconcileKilnTree(tree, KILN, [ev]);
    expect(names(next)).toEqual(['b.md']);
  });

  it('moved and delete+create converge to the same tree (order-independent)', () => {
    const start = root(KILN, [leaf('a.md', KILN)]);
    const viaMove = reconcileKilnTree(start, KILN, [
      { type: 'moved', from: '/vault/a.md', to: '/vault/b.md' },
    ]);
    const viaPair = reconcileKilnTree(start, KILN, [
      { type: 'deleted', path: '/vault/a.md' },
      { type: 'changed', path: '/vault/b.md', kind: 'created' },
    ]);
    const reversed = reconcileKilnTree(start, KILN, [
      { type: 'changed', path: '/vault/b.md', kind: 'created' },
      { type: 'deleted', path: '/vault/a.md' },
    ]);
    expect(names(viaMove)).toEqual(['b.md']);
    expect(names(viaPair).sort()).toEqual(['b.md']);
    expect(names(reversed).sort()).toEqual(['b.md']);
  });

  it('does not mutate the input tree', () => {
    const tree = root(KILN, [leaf('a.md', KILN)]);
    reconcileKilnTree(tree, KILN, [{ type: 'changed', path: '/vault/z.md', kind: 'created' }]);
    expect(names(tree)).toEqual(['a.md']);
  });
});

describe('projectFoldersToInvalidate', () => {
  const PROJ = '/proj';
  // src is loaded (children defined); node_modules is NOT loaded (undefined).
  const projRoot = root(PROJ, [
    dir('src', PROJ, [leaf('src/a.ts', PROJ)]),
    dir('node_modules', PROJ, undefined),
  ]);
  const mount: RootMount = { rootId: 'p', kind: 'project', basePath: PROJ, root: projRoot };

  it('returns the parent folder of a change when it is loaded', () => {
    const out = projectFoldersToInvalidate(mount, [
      { type: 'changed', path: '/proj/src/b.ts', kind: 'created' },
    ]);
    expect(out).toEqual(['src']);
  });

  it('returns [] when the parent folder is unloaded or absent', () => {
    const unloaded = projectFoldersToInvalidate(mount, [
      { type: 'changed', path: '/proj/node_modules/x/index.js', kind: 'modified' },
    ]);
    expect(unloaded).toEqual([]);
    const absent = projectFoldersToInvalidate(mount, [
      { type: 'deleted', path: '/proj/nowhere/y.ts' },
    ]);
    expect(absent).toEqual([]);
  });

  it('a top-level change invalidates the root ("")', () => {
    const out = projectFoldersToInvalidate(mount, [
      { type: 'changed', path: '/proj/README.md', kind: 'created' },
    ]);
    expect(out).toEqual(['']);
  });

  it('moved dedupes both endpoints parents', () => {
    const out = projectFoldersToInvalidate(mount, [
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
