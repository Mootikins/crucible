import { describe, expect, it } from 'vitest';
import { adoptPositions, buildAdjacency, buildGraph, nodeRadius } from '../build';
import type { GraphDto, GraphFilters, GraphNode } from '../types';

const filters = (over: Partial<GraphFilters> = {}): GraphFilters => ({
  query: '',
  showTags: false,
  showPhantoms: true,
  showOrphans: true,
  ...over,
});

const dto: GraphDto = {
  notes: [
    { path: 'Help/Wikilinks.md', title: 'Wikilinks', tags: ['help'] },
    { path: 'Help/Notes.md', title: 'Notes', tags: ['help', 'basics'] },
    { path: 'Lonely.md', title: 'Lonely', tags: [] },
  ],
  links: [
    { source: 'Help/Wikilinks.md', target: 'Help/Notes.md', resolved: true },
    { source: 'Help/Notes.md', target: 'missing note', resolved: false },
  ],
};

describe('buildGraph', () => {
  it('builds note nodes and resolved edges with degrees', () => {
    const g = buildGraph(dto, filters({ showPhantoms: false }));
    expect(g.nodes.map((n) => n.id).sort()).toEqual([
      'Help/Notes.md',
      'Help/Wikilinks.md',
      'Lonely.md',
    ]);
    expect(g.edges).toHaveLength(1);
    const byId = new Map(g.nodes.map((n) => [n.id, n]));
    expect(byId.get('Help/Wikilinks.md')!.degree).toBe(1);
    expect(byId.get('Help/Notes.md')!.degree).toBe(1);
    expect(byId.get('Lonely.md')!.degree).toBe(0);
  });

  it('synthesizes phantom nodes for unresolved links', () => {
    const g = buildGraph(dto, filters());
    const phantom = g.nodes.find((n) => n.kind === 'phantom');
    expect(phantom).toMatchObject({ id: 'phantom:missing note', label: 'missing note', degree: 1 });
    expect(g.edges.some((e) => e.kind === 'unresolved')).toBe(true);
  });

  it('synthesizes deduped tag nodes when enabled', () => {
    const g = buildGraph(dto, filters({ showTags: true }));
    const tags = g.nodes.filter((n) => n.kind === 'tag').map((n) => n.label).sort();
    expect(tags).toEqual(['#basics', '#help']);
    // Two notes share #help → one node, two edges.
    expect(g.edges.filter((e) => e.target === 'tag:help')).toHaveLength(2);
  });

  it('drops orphan notes when showOrphans is off (phantoms count as links)', () => {
    const g = buildGraph(dto, filters({ showOrphans: false }));
    expect(g.nodes.find((n) => n.id === 'Lonely.md')).toBeUndefined();
    // Notes.md links to a phantom, so it survives.
    expect(g.nodes.find((n) => n.id === 'Help/Notes.md')).toBeDefined();
  });

  it('a note only linked to a hidden phantom becomes an orphan', () => {
    const only: GraphDto = {
      notes: [{ path: 'A.md', title: 'A', tags: [] }],
      links: [{ source: 'A.md', target: 'ghost', resolved: false }],
    };
    const g = buildGraph(only, filters({ showPhantoms: false, showOrphans: false }));
    expect(g.nodes).toHaveLength(0);
  });

  it('query filters notes and prunes edges to hidden notes', () => {
    const g = buildGraph(dto, filters({ query: 'wikilink' }));
    expect(g.nodes.map((n) => n.id)).toEqual(['Help/Wikilinks.md']);
    expect(g.edges).toHaveLength(0);
  });

  it('dedupes repeated links and skips self-links', () => {
    const noisy: GraphDto = {
      notes: [
        { path: 'A.md', title: 'A', tags: [] },
        { path: 'B.md', title: 'B', tags: [] },
      ],
      links: [
        { source: 'A.md', target: 'B.md', resolved: true },
        { source: 'A.md', target: 'B.md', resolved: true },
        { source: 'A.md', target: 'A.md', resolved: true },
      ],
    };
    const g = buildGraph(noisy, filters());
    expect(g.edges).toHaveLength(1);
  });

  it('falls back to file stem when title is empty', () => {
    const g = buildGraph(
      { notes: [{ path: 'Deep/Dir/thing.md', title: '', tags: [] }], links: [] },
      filters(),
    );
    expect(g.nodes[0].label).toBe('thing');
  });
});

describe('buildAdjacency', () => {
  it('is undirected and handles node-ref endpoints', () => {
    const a: GraphNode = { id: 'a', label: 'a', kind: 'note', degree: 1 };
    const adj = buildAdjacency([
      { source: a, target: 'b', kind: 'link' },
      { source: 'b', target: 'c', kind: 'link' },
    ]);
    expect([...adj.get('b')!].sort()).toEqual(['a', 'c']);
    expect(adj.get('a')!.has('b')).toBe(true);
  });
});

describe('adoptPositions', () => {
  it('carries prior coordinates by id and leaves new nodes unseeded', () => {
    const prev: GraphNode[] = [
      { id: 'a', label: 'a', kind: 'note', degree: 0, x: 10, y: 20, vx: 1, vy: 2 },
    ];
    const next: GraphNode[] = [
      { id: 'a', label: 'a', kind: 'note', degree: 0 },
      { id: 'b', label: 'b', kind: 'note', degree: 0 },
    ];
    adoptPositions(next, prev);
    expect(next[0]).toMatchObject({ x: 10, y: 20, vx: 1, vy: 2 });
    expect(next[1].x).toBeUndefined();
  });
});

describe('nodeRadius', () => {
  it('grows with degree and scales with the size knob', () => {
    const lo: GraphNode = { id: 'a', label: 'a', kind: 'note', degree: 0 };
    const hi: GraphNode = { id: 'b', label: 'b', kind: 'note', degree: 9 };
    expect(nodeRadius(hi, 1)).toBeGreaterThan(nodeRadius(lo, 1));
    expect(nodeRadius(lo, 2)).toBeCloseTo(nodeRadius(lo, 1) * 2);
  });
});
