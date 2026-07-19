import { describe, it, expect } from 'vitest';
import { sortTree, makeFileCollection } from '../collection';
import type { FileTreeNode, SortSpec } from '../types';

const leaf = (name: string, modified?: number): FileTreeNode => ({
  relPath: name,
  name,
  isDir: false,
  absPath: `/r/${name}`,
  modified,
});
const dir = (name: string, kids: FileTreeNode[]): FileTreeNode => ({
  relPath: name,
  name,
  isDir: true,
  absPath: `/r/${name}`,
  children: kids,
});
const root = (kids: FileTreeNode[]): FileTreeNode => ({
  relPath: '',
  name: '',
  isDir: true,
  absPath: '/r',
  children: kids,
});

const names = (n: FileTreeNode) => (n.children ?? []).map((c) => c.name);

describe('sortTree', () => {
  it('always puts folders before files regardless of axis/dir', () => {
    const specs: SortSpec[] = [
      { key: 'name', dir: 'asc' },
      { key: 'name', dir: 'desc' },
      { key: 'modified', dir: 'asc' },
      { key: 'modified', dir: 'desc' },
    ];
    const tree = root([leaf('z.md', 5), dir('a', []), leaf('a.md', 1), dir('z', [])]);
    for (const s of specs) {
      const sorted = sortTree(tree, s);
      const kids = sorted.children!;
      const firstFileIdx = kids.findIndex((c) => !c.isDir);
      const lastDirIdx = kids.map((c) => c.isDir).lastIndexOf(true);
      expect(lastDirIdx).toBeLessThan(firstFileIdx);
    }
  });

  it('sorts by name asc with natural numeric collation (note2 < note10)', () => {
    const tree = root([leaf('note10.md'), leaf('note2.md'), leaf('note1.md')]);
    expect(names(sortTree(tree, { key: 'name', dir: 'asc' }))).toEqual([
      'note1.md',
      'note2.md',
      'note10.md',
    ]);
  });

  it('sorts by name desc', () => {
    const tree = root([leaf('a.md'), leaf('c.md'), leaf('b.md')]);
    expect(names(sortTree(tree, { key: 'name', dir: 'desc' }))).toEqual(['c.md', 'b.md', 'a.md']);
  });

  it('sorts by modified asc and desc', () => {
    const tree = root([leaf('a.md', 30), leaf('b.md', 10), leaf('c.md', 20)]);
    expect(names(sortTree(tree, { key: 'modified', dir: 'asc' }))).toEqual(['b.md', 'c.md', 'a.md']);
    expect(names(sortTree(tree, { key: 'modified', dir: 'desc' }))).toEqual(['a.md', 'c.md', 'b.md']);
  });

  it('breaks equal-key ties by name-asc', () => {
    const tree = root([leaf('b.md', 5), leaf('a.md', 5)]);
    expect(names(sortTree(tree, { key: 'modified', dir: 'desc' }))).toEqual(['a.md', 'b.md']);
  });

  it('recurses into nested directories', () => {
    const tree = root([dir('d', [leaf('y.md'), leaf('x.md')])]);
    const sorted = sortTree(tree, { key: 'name', dir: 'asc' });
    expect(names(sorted.children![0])).toEqual(['x.md', 'y.md']);
  });

  it('is pure — does not mutate the input tree', () => {
    const tree = root([leaf('b.md'), leaf('a.md')]);
    const snapshot = names(tree);
    sortTree(tree, { key: 'name', dir: 'asc' });
    expect(names(tree)).toEqual(snapshot);
  });
});

describe('makeFileCollection', () => {
  it('exposes rootNode children and resolves relPath via nodeToValue', () => {
    // Use realistic nested relPaths (as the kiln builder produces).
    const child: FileTreeNode = {
      relPath: 'Meta/Systems.md',
      name: 'Systems.md',
      isDir: false,
      absPath: '/r/Meta/Systems.md',
    };
    const tree = root([dir('Meta', [child])]);
    const col = makeFileCollection(tree);
    expect(col.rootNode.children!.map((c) => c.name)).toEqual(['Meta']);
    const idx = col.getIndexPath('Meta/Systems.md');
    expect(idx).toBeTruthy();
    expect(col.getValuePath(idx!)).toEqual(['Meta', 'Meta/Systems.md']);
  });

  it('treats an unloaded dir (children: undefined) as a branch', () => {
    const lazyDir: FileTreeNode = {
      relPath: 'src',
      name: 'src',
      isDir: true,
      absPath: '/r/src',
    };
    const col = makeFileCollection(root([lazyDir]));
    expect(col.isBranchNode(col.rootNode.children![0])).toBe(true);
  });
});
