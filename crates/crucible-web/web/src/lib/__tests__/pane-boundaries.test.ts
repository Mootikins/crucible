import { describe, it, expect } from 'vitest';
import { paneBoundaries, findSplitInLayout } from '../pane-boundaries';
import type { LayoutNode } from '@/types/windowTypes';

const pane = (id: string): LayoutNode => ({ type: 'pane', id, tabGroupId: `g-${id}` });

const split = (
  id: string,
  first: LayoutNode,
  second: LayoutNode,
  direction: 'horizontal' | 'vertical' = 'vertical',
): LayoutNode => ({ type: 'split', id, direction, splitRatio: 0.5, first, second });

describe('paneBoundaries', () => {
  it('gives the topmost pane no boundary', () => {
    // Its top edge is the region's own top. Nothing put it there, so there is
    // nothing to drag — and in a rail those pixels belong to the rail toggle.
    const tree = split('s1', pane('a'), pane('b'));
    expect(paneBoundaries(tree).get('a')).toBeNull();
  });

  it('names the split that placed each following pane', () => {
    const tree = split('s1', pane('a'), pane('b'));
    expect(paneBoundaries(tree).get('b')).toBe('s1');
  });

  // The user's "someone could drag a third one below that": a third pane gets
  // its own boundary, and it is the NEARER split, not the outer one.
  it('gives a third pane its own nearer boundary', () => {
    const tree = split('outer', pane('a'), split('inner', pane('b'), pane('c')));
    const bounds = paneBoundaries(tree);
    expect([...bounds.entries()]).toEqual([
      ['a', null],
      ['b', 'outer'],
      ['c', 'inner'],
    ]);
  });

  // Nesting on the FIRST side is the case an "is this the second child?" test
  // gets wrong: `b` opens the second half of `outer`, so `outer` is its edge
  // even though `b` is a `first` child of `inner`.
  it('carries an inherited boundary down the first side of a nested split', () => {
    const tree = split('outer', pane('a'), split('inner', split('deep', pane('b'), pane('c')), pane('d')));
    const bounds = paneBoundaries(tree);
    expect(bounds.get('b')).toBe('outer');
    expect(bounds.get('c')).toBe('deep');
    expect(bounds.get('d')).toBe('inner');
  });

  it('gives a lone pane no boundary at all', () => {
    expect([...paneBoundaries(pane('only')).entries()]).toEqual([['only', null]]);
  });
});

describe('findSplitInLayout', () => {
  it('finds a split at any depth', () => {
    const tree = split('outer', pane('a'), split('inner', pane('b'), pane('c')));
    expect(findSplitInLayout(tree, 'inner')?.id).toBe('inner');
    expect(findSplitInLayout(tree, 'outer')?.id).toBe('outer');
  });

  it('returns null for a pane id or an unknown id', () => {
    const tree = split('outer', pane('a'), pane('b'));
    expect(findSplitInLayout(tree, 'a')).toBeNull();
    expect(findSplitInLayout(tree, 'nope')).toBeNull();
  });
});
