import { describe, expect, it } from 'vitest';
import {
  DUPLICATE_OFFSET,
  HISTORY_LIMIT,
  MIN_NODE_SIZE,
  addEdge,
  addNode,
  canRedo,
  canUndo,
  commit,
  duplicateNodes,
  groupSelection,
  initHistory,
  moveSelection,
  nodesInRect,
  redo,
  removeNodes,
  resizeNode,
  undo,
  updateNode,
  withGroupMembers,
} from '../canvas-doc';
import type { CanvasDoc, CanvasNode, GroupNode } from '../canvas-types';

const text = (id: string, x = 0, y = 0, w = 100, h = 100): CanvasNode => ({
  id,
  type: 'text',
  text: id,
  x,
  y,
  width: w,
  height: h,
});

const group = (id: string, x: number, y: number, w: number, h: number): GroupNode => ({
  id,
  type: 'group',
  x,
  y,
  width: w,
  height: h,
});

const doc = (nodes: CanvasNode[], edges: CanvasDoc['edges'] = []): CanvasDoc => ({ nodes, edges });

describe('node mutations', () => {
  it('patches only the named node and keeps unknown keys', () => {
    const original: CanvasDoc = doc([{ ...text('a'), styleAttributes: { shape: 'diamond' } }]);
    const next = updateNode(original, 'a', { x: 500 });

    expect(next.nodes[0].x).toBe(500);
    expect(next.nodes[0].styleAttributes).toEqual({ shape: 'diamond' });
    expect(original.nodes[0].x).toBe(0);
  });

  it('appends new nodes so they land on top in z-order', () => {
    const next = addNode(doc([text('back')]), text('front'));
    expect(next.nodes.map((n) => n.id)).toEqual(['back', 'front']);
  });

  /** A dangling edge renders as nothing with no explanation — worse than gone. */
  it('removes edges that touched a deleted node', () => {
    const original = addEdge(doc([text('a'), text('b'), text('c')]), {
      id: 'e1',
      fromNode: 'a',
      toNode: 'b',
    });
    const withSecond = addEdge(original, { id: 'e2', fromNode: 'b', toNode: 'c' });

    const next = removeNodes(withSecond, new Set(['b']));

    expect(next.nodes.map((n) => n.id)).toEqual(['a', 'c']);
    expect(next.edges).toEqual([]);
  });

  it('refuses to resize a node below the minimum', () => {
    const next = resizeNode(doc([text('a')]), 'a', { x: 0, y: 0, width: 2, height: -50 });
    expect(next.nodes[0].width).toBe(MIN_NODE_SIZE);
    expect(next.nodes[0].height).toBe(MIN_NODE_SIZE);
  });
});

describe('selection and groups', () => {
  const scene = doc([
    group('g', 0, 0, 400, 400),
    text('inside', 50, 50),
    text('outside', 900, 900),
  ]);

  it('expands a group selection to its geometric members', () => {
    expect(withGroupMembers(scene, new Set(['g']))).toEqual(new Set(['g', 'inside']));
  });

  it('carries group members when the group moves', () => {
    const next = moveSelection(scene, new Set(['g']), 100, 0);
    expect(next.nodes.find((n) => n.id === 'inside')!.x).toBe(150);
    expect(next.nodes.find((n) => n.id === 'outside')!.x).toBe(900);
  });

  /**
   * Membership must be resolved before the move, or a group sliding over a card
   * would pick it up mid-drag.
   */
  it('does not pick up nodes the group merely slides over', () => {
    const next = moveSelection(scene, new Set(['g']), 800, 800);
    expect(next.nodes.find((n) => n.id === 'outside')!.x).toBe(900);
  });

  it('moving a plain selection leaves everything else alone', () => {
    const next = moveSelection(scene, new Set(['inside']), 10, 20);
    expect(next.nodes.find((n) => n.id === 'inside')!.y).toBe(70);
    expect(next.nodes.find((n) => n.id === 'g')!.y).toBe(0);
  });

  /** A backdrop drawn last would cover the cards it contains. */
  it('prepends a new group so it sits behind its members', () => {
    const next = groupSelection(doc([text('a', 0, 0), text('b', 200, 0)]), new Set(['a', 'b']), 'g1');
    expect(next.nodes[0].id).toBe('g1');
    expect(next.nodes[0].type).toBe('group');
  });

  it('sizes a new group to enclose its members', () => {
    const next = groupSelection(doc([text('a', 0, 0), text('b', 200, 100)]), new Set(['a', 'b']), 'g1');
    const g = next.nodes[0];
    expect(g.x).toBeLessThan(0);
    expect(g.x + g.width).toBeGreaterThan(300);
    expect(g.y + g.height).toBeGreaterThan(200);
  });

  it('grouping an empty selection is a no-op', () => {
    const original = doc([text('a')]);
    expect(groupSelection(original, new Set(), 'g1')).toBe(original);
  });

  it('selects nodes intersecting a marquee', () => {
    const scene2 = doc([text('hit', 0, 0), text('miss', 900, 900)]);
    expect(nodesInRect(scene2, { x: -10, y: -10, width: 200, height: 200 })).toEqual(['hit']);
  });
});

describe('duplication', () => {
  it('offsets copies and reports the new ids', () => {
    const { doc: next, created } = duplicateNodes(doc([text('a', 10, 10)]), new Set(['a']), (i) => `copy-${i}`);
    expect(created).toEqual(['copy-0']);
    const copy = next.nodes.find((n) => n.id === 'copy-0')!;
    expect(copy.x).toBe(10 + DUPLICATE_OFFSET);
    expect(next.nodes).toHaveLength(2);
  });
});

describe('history', () => {
  const a = doc([text('a')]);
  const b = doc([text('b')]);
  const c = doc([text('c')]);

  it('undoes and redoes through the stack', () => {
    let h = initHistory(a);
    h = commit(h, b);
    h = commit(h, c);

    expect(h.present).toBe(c);
    h = undo(h);
    expect(h.present).toBe(b);
    h = undo(h);
    expect(h.present).toBe(a);
    h = redo(h);
    expect(h.present).toBe(b);
  });

  it('reports what is available', () => {
    const fresh = initHistory(a);
    expect(canUndo(fresh)).toBe(false);
    expect(canRedo(fresh)).toBe(false);

    const after = commit(fresh, b);
    expect(canUndo(after)).toBe(true);
    expect(canRedo(undo(after))).toBe(true);
  });

  it('is a no-op at either end rather than throwing', () => {
    const fresh = initHistory(a);
    expect(undo(fresh)).toBe(fresh);
    expect(redo(fresh)).toBe(fresh);
  });

  /** A drag that moved nothing must not cost an undo press that does nothing. */
  it('drops a commit identical to the present', () => {
    const h = initHistory(a);
    expect(commit(h, a)).toBe(h);
  });

  it('discards the redo branch once a new edit lands', () => {
    let h = commit(commit(initHistory(a), b), c);
    h = undo(h);
    h = commit(h, doc([text('d')]));
    expect(canRedo(h)).toBe(false);
  });

  it('caps retained depth', () => {
    let h = initHistory(a);
    for (let i = 0; i < HISTORY_LIMIT + 25; i++) {
      h = commit(h, doc([text(`n${i}`)]));
    }
    expect(h.past.length).toBe(HISTORY_LIMIT);
  });
});
