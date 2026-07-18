import { describe, it, expect } from 'vitest';
import { notesToTree } from '../kiln-builder';
import type { FileTreeNode } from '../types';
import type { NoteEntry } from '@/lib/types';

const note = (path: string, updated_at = ''): NoteEntry => ({
  name: path.split('/').pop() ?? path,
  path,
  title: null,
  tags: [],
  updated_at,
});

const ROOT = '/vault';
const children = (n: FileTreeNode) => n.children ?? [];
const byName = (n: FileTreeNode, name: string) => children(n).find((c) => c.name === name);

describe('notesToTree', () => {
  it('flat top-level notes become leaves with relPath === name and joined absPath', () => {
    const tree = notesToTree([note('a.md'), note('b.md')], ROOT);
    expect(children(tree).map((c) => c.name)).toEqual(['a.md', 'b.md']);
    const a = byName(tree, 'a.md')!;
    expect(a.isDir).toBe(false);
    expect(a.relPath).toBe('a.md');
    expect(a.absPath).toBe('/vault/a.md');
  });

  it('nests folder segments into a single shared directory node', () => {
    const tree = notesToTree(
      [note('Meta/Systems.md'), note('Meta/Roadmap.md'), note('README.md')],
      ROOT,
    );
    expect(children(tree).map((c) => c.name).sort()).toEqual(['Meta', 'README.md']);
    const meta = byName(tree, 'Meta')!;
    expect(meta.isDir).toBe(true);
    expect(meta.relPath).toBe('Meta');
    expect(meta.absPath).toBe('/vault/Meta');
    expect(children(meta).map((c) => c.name).sort()).toEqual(['Roadmap.md', 'Systems.md']);
    expect(byName(meta, 'Systems.md')!.relPath).toBe('Meta/Systems.md');
  });

  it('creates three nested dirs for a deep path', () => {
    const tree = notesToTree([note('a/b/c/d.md')], ROOT);
    const a = byName(tree, 'a')!;
    const b = byName(a, 'b')!;
    const c = byName(b, 'c')!;
    expect([a.isDir, b.isDir, c.isDir]).toEqual([true, true, true]);
    expect(c.relPath).toBe('a/b/c');
    expect(byName(c, 'd.md')!.relPath).toBe('a/b/c/d.md');
  });

  it('same stem in different folders yields two distinct leaves', () => {
    const tree = notesToTree([note('x/note.md'), note('y/note.md')], ROOT);
    const x = byName(tree, 'x')!;
    const y = byName(tree, 'y')!;
    expect(byName(x, 'note.md')!.relPath).toBe('x/note.md');
    expect(byName(y, 'note.md')!.relPath).toBe('y/note.md');
    expect(byName(x, 'note.md')).not.toBe(byName(y, 'note.md'));
  });

  it('reuses a directory node across sibling files (one node, not two)', () => {
    const tree = notesToTree([note('d/one.md'), note('d/two.md')], ROOT);
    const dirs = children(tree).filter((c) => c.name === 'd');
    expect(dirs).toHaveLength(1);
    expect(children(dirs[0]).map((c) => c.name).sort()).toEqual(['one.md', 'two.md']);
  });

  it('deduplicates identical paths (first wins)', () => {
    const tree = notesToTree([note('dup.md'), note('dup.md')], ROOT);
    expect(children(tree).filter((c) => c.name === 'dup.md')).toHaveLength(1);
  });

  it('directory wins over a same-named leaf (leaf dropped)', () => {
    // 'A/B' appears as both a directory prefix (via A/B/c.md) and a leaf.
    const tree = notesToTree([note('A/B'), note('A/B/c.md')], ROOT);
    const a = byName(tree, 'A')!;
    const b = byName(a, 'B')!;
    expect(b.isDir).toBe(true);
    expect(children(a).filter((c) => c.name === 'B')).toHaveLength(1);
    expect(byName(b, 'c.md')!.relPath).toBe('A/B/c.md');
  });

  it('normalizes leading ./ and / and skips empty paths', () => {
    const tree = notesToTree([note('./x.md'), note('/y.md'), note('')], ROOT);
    expect(children(tree).map((c) => c.name).sort()).toEqual(['x.md', 'y.md']);
  });

  it('directory modified is the max of descendant leaf epochs', () => {
    const tree = notesToTree(
      [
        note('d/old.md', '2020-01-01T00:00:00Z'),
        note('d/new.md', '2024-01-01T00:00:00Z'),
      ],
      ROOT,
    );
    const d = byName(tree, 'd')!;
    expect(d.modified).toBe(Math.floor(Date.parse('2024-01-01T00:00:00Z') / 1000));
  });
});
