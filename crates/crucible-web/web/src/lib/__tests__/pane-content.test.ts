import { describe, it, expect } from 'vitest';
import { hasTabsOutsidePane, paneHasTabs, subtreeHasTabs } from '../pane-content';
import type { LayoutNode, TabGroup } from '@/types/windowTypes';

const group = (id: string, count: number): TabGroup => ({
  id,
  tabs: Array.from({ length: count }, (_, i) => ({
    id: `${id}-tab-${i}`,
    title: 'note.md',
    contentType: 'file' as const,
  })),
  activeTabId: count > 0 ? `${id}-tab-0` : null,
});

const groups = {
  full: group('full', 1),
  empty: group('empty', 0),
};

const pane = (id: string, tabGroupId: string | null): LayoutNode => ({
  id,
  type: 'pane',
  tabGroupId,
});

const split = (id: string, first: LayoutNode, second: LayoutNode): LayoutNode => ({
  id,
  type: 'split',
  direction: 'horizontal',
  splitRatio: 0.5,
  first,
  second,
});

describe('paneHasTabs', () => {
  it('is true only for a leaf whose group holds a tab', () => {
    expect(paneHasTabs(groups, pane('a', 'full'))).toBe(true);
    expect(paneHasTabs(groups, pane('b', 'empty'))).toBe(false);
  });

  it('is false for a pane with no group and for a split', () => {
    expect(paneHasTabs(groups, pane('c', null))).toBe(false);
    expect(paneHasTabs(groups, split('s', pane('a', 'full'), pane('b', 'empty')))).toBe(false);
  });

  it('is false for a group id no group answers for (stale layout)', () => {
    expect(paneHasTabs(groups, pane('d', 'gone'))).toBe(false);
  });
});

describe('subtreeHasTabs', () => {
  it('finds a tab at any depth', () => {
    const tree = split(
      'outer',
      pane('a', 'empty'),
      split('inner', pane('b', 'empty'), pane('c', 'full')),
    );
    expect(subtreeHasTabs(groups, tree)).toBe(true);
  });

  it('is false when every leaf is empty', () => {
    const tree = split('outer', pane('a', 'empty'), split('inner', pane('b', 'empty'), pane('c', null)));
    expect(subtreeHasTabs(groups, tree)).toBe(false);
  });
});

describe('hasTabsOutsidePane', () => {
  const tree = split('outer', pane('a', 'empty'), pane('b', 'full'));

  it('sees a sibling that still holds work', () => {
    expect(hasTabsOutsidePane(groups, tree, 'a')).toBe(true);
  });

  it('does not count the pane it is asked about', () => {
    // 'b' holds the only tab in the tree, so nothing OUTSIDE it does.
    expect(hasTabsOutsidePane(groups, tree, 'b')).toBe(false);
  });

  it('is false for a region with nothing open', () => {
    expect(hasTabsOutsidePane(groups, pane('a', 'empty'), 'a')).toBe(false);
  });
});
