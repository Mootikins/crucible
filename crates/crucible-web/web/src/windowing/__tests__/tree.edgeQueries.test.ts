import { describe, it, expect } from 'vitest';
import { edgeLeaf, firstLeafGroupId } from '@/windowing/model/tree';
import type { LayoutNode } from '@/windowing/model/types';

const pane = (id: string, tabGroupId: string | null = `g-${id}`): LayoutNode => ({ id, type: 'pane', tabGroupId });
const split = (direction: 'horizontal' | 'vertical', first: LayoutNode, second: LayoutNode): LayoutNode => ({
  id: `s-${first.id}-${second.id}`,
  type: 'split',
  direction,
  first,
  second,
  splitRatio: 0.5,
});

describe('edgeLeaf', () => {
  it('names the only pane for either side', () => {
    expect(edgeLeaf(pane('a'), 'left')).toEqual({ paneId: 'a', groupId: 'g-a' });
    expect(edgeLeaf(pane('a'), 'right')).toEqual({ paneId: 'a', groupId: 'g-a' });
  });

  it('takes the first half of a side-by-side split on the left and the second on the right', () => {
    const row = split('horizontal', pane('a'), pane('b'));
    expect(edgeLeaf(row, 'left').paneId).toBe('a');
    expect(edgeLeaf(row, 'right').paneId).toBe('b');
  });

  it('counts both halves of a stacked split as the same edge, and the top one wins', () => {
    const column = split('vertical', pane('top'), pane('bottom'));
    expect(edgeLeaf(column, 'left').paneId).toBe('top');
    expect(edgeLeaf(column, 'right').paneId).toBe('top');
  });

  it('walks nested splits to the outermost leaf', () => {
    const layout = split('horizontal', split('vertical', pane('a'), pane('b')), split('horizontal', pane('c'), pane('d')));
    expect(edgeLeaf(layout, 'left').paneId).toBe('a');
    expect(edgeLeaf(layout, 'right').paneId).toBe('d');
  });

  it('reports a leaf with no group as a null group', () => {
    expect(edgeLeaf(pane('a', null), 'left')).toEqual({ paneId: 'a', groupId: null });
  });
});

describe('firstLeafGroupId', () => {
  it('names the group of the first leaf in reading order', () => {
    expect(firstLeafGroupId(split('horizontal', pane('a'), pane('b')))).toBe('g-a');
  });

  it('skips a leaf that has no group', () => {
    expect(firstLeafGroupId(split('horizontal', pane('a', null), pane('b')))).toBe('g-b');
  });

  it('is null when no leaf has a group', () => {
    expect(firstLeafGroupId(split('vertical', pane('a', null), pane('b', null)))).toBeNull();
  });
});
