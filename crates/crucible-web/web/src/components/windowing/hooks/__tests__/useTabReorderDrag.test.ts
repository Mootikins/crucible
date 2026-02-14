import { describe, it, expect, beforeEach } from 'vitest';
import {
  computeInsertIndex,
  getPendingReorder,
  clearPendingReorder,
} from '../useTabReorderDrag';

function makeContainer(
  tabs: { id: string; left: number; width: number }[],
): HTMLElement {
  const container = document.createElement('div');
  for (const tab of tabs) {
    const el = document.createElement('div');
    el.dataset.tabId = tab.id;
    el.getBoundingClientRect = () =>
      ({
        left: tab.left,
        right: tab.left + tab.width,
        width: tab.width,
        top: 0,
        bottom: 30,
        height: 30,
        x: tab.left,
        y: 0,
        toJSON() {},
      }) as DOMRect;
    container.appendChild(el);
  }
  return container;
}

describe('computeInsertIndex', () => {
  // Three 100px-wide tabs: a=[0,100) b=[100,200) c=[200,300) — midpoints at 50, 150, 250
  const threeTabs = [
    { id: 'a', left: 0, width: 100 },
    { id: 'b', left: 100, width: 100 },
    { id: 'c', left: 200, width: 100 },
  ];

  it('returns index 0 when pointer is before first tab midpoint', () => {
    const result = computeInsertIndex(makeContainer(threeTabs), 30);
    expect(result).toEqual({ logical: 0, display: 0 });
  });

  it('returns index 1 when pointer is between first and second midpoints', () => {
    const result = computeInsertIndex(makeContainer(threeTabs), 80);
    expect(result).toEqual({ logical: 1, display: 1 });
  });

  it('returns index 2 when pointer is between second and third midpoints', () => {
    const result = computeInsertIndex(makeContainer(threeTabs), 180);
    expect(result).toEqual({ logical: 2, display: 2 });
  });

  it('returns last index when pointer is past all tabs', () => {
    const result = computeInsertIndex(makeContainer(threeTabs), 999);
    expect(result).toEqual({ logical: 3, display: 3 });
  });

  it('returns index 0 when pointer is at first tab left edge', () => {
    const result = computeInsertIndex(makeContainer(threeTabs), 0);
    expect(result).toEqual({ logical: 0, display: 0 });
  });

  it('pointer exactly at midpoint lands after that tab', () => {
    const result = computeInsertIndex(makeContainer(threeTabs), 50);
    expect(result).toEqual({ logical: 1, display: 1 });
  });

  describe('with draggedTabId', () => {
    it('skips dragged tab in logical index but preserves display index', () => {
      const result = computeInsertIndex(makeContainer(threeTabs), 180, 'b');
      expect(result).toEqual({ logical: 1, display: 2 });
    });

    it('dragging first tab, pointer past all → logical count excludes dragged', () => {
      const result = computeInsertIndex(makeContainer(threeTabs), 999, 'a');
      expect(result).toEqual({ logical: 2, display: 3 });
    });

    it('dragging last tab, pointer before all → index 0', () => {
      const result = computeInsertIndex(makeContainer(threeTabs), 10, 'c');
      expect(result).toEqual({ logical: 0, display: 0 });
    });

    it('ignores draggedTabId not present in container', () => {
      const result = computeInsertIndex(makeContainer(threeTabs), 80, 'nonexistent');
      expect(result).toEqual({ logical: 1, display: 1 });
    });
  });

  describe('edge cases', () => {
    it('empty container returns logical 0, display 0', () => {
      const result = computeInsertIndex(makeContainer([]), 100);
      expect(result).toEqual({ logical: 0, display: 0 });
    });

    it('single tab — pointer before midpoint', () => {
      const result = computeInsertIndex(
        makeContainer([{ id: 'only', left: 0, width: 200 }]),
        50,
      );
      expect(result).toEqual({ logical: 0, display: 0 });
    });

    it('single tab — pointer after midpoint', () => {
      const result = computeInsertIndex(
        makeContainer([{ id: 'only', left: 0, width: 200 }]),
        150,
      );
      expect(result).toEqual({ logical: 1, display: 1 });
    });

    it('negative pointer position returns index 0', () => {
      const result = computeInsertIndex(makeContainer(threeTabs), -50);
      expect(result).toEqual({ logical: 0, display: 0 });
    });
  });
});

describe('getPendingReorder / clearPendingReorder', () => {
  beforeEach(() => {
    clearPendingReorder();
  });

  it('returns null after clear', () => {
    expect(getPendingReorder()).toBeNull();
  });

  it('clearPendingReorder is idempotent', () => {
    clearPendingReorder();
    clearPendingReorder();
    clearPendingReorder();
    expect(getPendingReorder()).toBeNull();
  });
});
