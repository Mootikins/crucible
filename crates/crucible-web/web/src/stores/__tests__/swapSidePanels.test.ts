import { describe, it, expect, beforeEach } from 'vitest';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import {
  collectLeafGroupIds,
  findFirstPane,
  mirrorLayout,
  primaryEdgeGroupId,
} from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';
import type { LayoutNode } from '@/types/windowTypes';
import { isEdgeCollapsed } from '@/types/windowTypes';

const leftGroup = () => primaryEdgeGroupId(windowStore, 'left');
const rightGroup = () => primaryEdgeGroupId(windowStore, 'right');

describe('swapSidePanels', () => {
  beforeEach(() => {
    setStore(defaultLayout());
  });

  it('moves each side’s panes to the other side', () => {
    const before = { left: leftGroup(), right: rightGroup() };
    windowActions.swapSidePanels();
    expect(leftGroup()).toBe(before.right);
    expect(rightGroup()).toBe(before.left);
  });

  // Every field travels with its panes, so two flips return the start state
  // whatever the two sides say about collapse.
  it('is its own inverse', () => {
    const before = { left: leftGroup(), right: rightGroup() };
    windowActions.swapSidePanels();
    windowActions.swapSidePanels();
    expect(leftGroup()).toBe(before.left);
    expect(rightGroup()).toBe(before.right);
  });

  // The focus ring is drawn where `focusedRegion` says, and the panes it
  // named are on the other side now. Every other action that moves a pane
  // between regions recomputes this; the swap must too, or no rail shows a
  // ring at all after one.
  it('carries the focused region across with its panes', () => {
    setStore('focusedRegion', 'left');
    windowActions.swapSidePanels();
    expect(windowStore.focusedRegion).toBe('right');
    windowActions.swapSidePanels();
    expect(windowStore.focusedRegion).toBe('left');
  });

  it('leaves a focus that is not on either rail alone', () => {
    setStore('focusedRegion', 'center');
    windowActions.swapSidePanels();
    expect(windowStore.focusedRegion).toBe('center');
  });

  // Width travels with the CONTENTS: a file tree dragged out to 320px should
  // not be re-cramped into the other rail's width on every swap.
  it('carries each side’s width across with its panes', () => {
    setStore('edgePanels', 'left', 'width', 250);
    setStore('edgePanels', 'right', 'width', 320);
    windowActions.swapSidePanels();
    expect(windowStore.edgePanels.left.width).toBe(320);
    expect(windowStore.edgePanels.right.width).toBe(250);
  });

  // Collapse travels with the PANES, the same as layout and width. A panel a
  // user stowed stays stowed after a flip; a panel a user opened stays open.
  // The flip moves panels between sides. It does not open or stow anything.
  it('carries each side’s collapse across with its panes', () => {
    const stowed = rightGroup();
    setStore('edgePanels', 'left', 'mode', 'docked');
    setStore('edgePanels', 'right', 'mode', 'strip');

    windowActions.swapSidePanels();

    expect(leftGroup()).toBe(stowed);
    expect(windowStore.edgePanels.left.mode).toBe('strip');
    expect(windowStore.edgePanels.right.mode).toBe('docked');
  });

  // A hidden rail keeps its cue on its new side. The cue describes how that
  // panel comes back, so it belongs to the panel, not to the side.
  it('carries a hidden rail’s cue across with its mode', () => {
    setStore('edgePanels', 'left', 'mode', 'hidden');
    setStore('edgePanels', 'left', 'cue', 'none');
    setStore('edgePanels', 'right', 'mode', 'docked');
    setStore('edgePanels', 'right', 'cue', undefined);

    windowActions.swapSidePanels();

    expect(windowStore.edgePanels.right.mode).toBe('hidden');
    expect(windowStore.edgePanels.right.cue).toBe('none');
    expect(windowStore.edgePanels.left.mode).toBe('docked');
    expect(windowStore.edgePanels.left.cue).toBeUndefined();
  });

  // Collapse and width describe the same panel, so they must not separate.
  // A 320px panel that a user stowed is a stowed 320px panel on its new side.
  it('carries collapse and width together, so they stay on one panel', () => {
    setStore('edgePanels', 'left', 'mode', 'strip');
    setStore('edgePanels', 'left', 'width', 250);
    setStore('edgePanels', 'right', 'mode', 'docked');
    setStore('edgePanels', 'right', 'width', 320);

    windowActions.swapSidePanels();

    expect(windowStore.edgePanels.left.mode).toBe('docked');
    expect(windowStore.edgePanels.left.width).toBe(320);
    expect(windowStore.edgePanels.right.mode).toBe('strip');
    expect(windowStore.edgePanels.right.width).toBe(250);
  });

  // The toggles are POSITIONAL — toggleEdgePanel('left') means "the left
  // side", not "the session list" — which is the whole reason a contents-only
  // swap needs no remapping anywhere.
  it('leaves the positional toggles pointing at the right sides', () => {
    windowActions.swapSidePanels();
    const swapped = leftGroup();
    const before = isEdgeCollapsed(windowStore.edgePanels.left);
    windowActions.toggleEdgePanel('left');
    expect(isEdgeCollapsed(windowStore.edgePanels.left)).toBe(!before);
    expect(leftGroup()).toBe(swapped);
  });

  it('survives a side with no panes', () => {
    const before = leftGroup();
    setStore('edgePanels', 'right', 'layout', { id: 'empty', type: 'pane', tabGroupId: null });
    windowActions.swapSidePanels();
    expect(leftGroup()).toBeNull();
    expect(rightGroup()).toBe(before);
  });
});

/** Tab-group ids left to right across the centre. */
const centreOrder = () => collectLeafGroupIds(windowStore.layout);
/** Group ids top to bottom inside one rail. */
const railOrder = (side: 'left' | 'right') =>
  collectLeafGroupIds(windowStore.edgePanels[side].layout);

describe('swapSidePanels — a 100% flip, not a rail swap', () => {
  beforeEach(() => {
    setStore(defaultLayout());
  });

  it('reverses the CENTRE columns too', () => {
    // A conversation left of its editor must end up right of it. Mirroring
    // only the rails left the middle of the row untouched, and half a mirror
    // reads as a bug because the eye checks the whole row.
    const pane = findFirstPane(windowStore.layout)!;
    windowActions.openTabInNewPane(pane.id, 'left', {
      id: 'tab-chat-1',
      title: 'chat',
      contentType: 'chat',
    });
    const before = centreOrder();
    expect(before).toHaveLength(2);

    windowActions.swapSidePanels();
    expect(centreOrder()).toEqual([...before].reverse());
  });

  it('keeps what is STACKED inside a rail stacked the same way', () => {
    // The file tree with a terminal under it: the column moves sides, the
    // terminal stays under the tree. Mirroring vertical splits too would put
    // it above.
    const before = railOrder('right');
    expect(before).toHaveLength(2);

    windowActions.swapSidePanels();
    expect(railOrder('left')).toEqual(before);
  });

  it('is its own inverse across every region', () => {
    const pane = findFirstPane(windowStore.layout)!;
    windowActions.openTabInNewPane(pane.id, 'left', {
      id: 'tab-chat-1',
      title: 'chat',
      contentType: 'chat',
    });
    const before = {
      centre: centreOrder(),
      left: railOrder('left'),
      right: railOrder('right'),
    };

    windowActions.swapSidePanels();
    windowActions.swapSidePanels();

    expect(centreOrder()).toEqual(before.centre);
    expect(railOrder('left')).toEqual(before.left);
    expect(railOrder('right')).toEqual(before.right);
  });
});

describe('mirrorLayout', () => {
  const pane = (id: string): LayoutNode => ({ id, type: 'pane', tabGroupId: `g-${id}` });
  /** Leaf ids left to right — the shape, without the arithmetic. */
  const leafIds = (n: LayoutNode): string[] =>
    n.type === 'pane' ? [n.id] : [...leafIds(n.first), ...leafIds(n.second)];

  it('swaps a horizontal split and inverts its ratio', () => {
    const node: LayoutNode = {
      id: 's', type: 'split', direction: 'horizontal', splitRatio: 0.25,
      first: pane('a'), second: pane('b'),
    };
    const out = mirrorLayout(node) as Extract<LayoutNode, { type: 'split' }>;
    expect(out.first.id).toBe('b');
    expect(out.second.id).toBe('a');
    // The ratio measures the FIRST half, and the halves traded places.
    expect(out.splitRatio).toBe(0.75);
  });

  it('leaves a vertical split standing', () => {
    const node: LayoutNode = {
      id: 's', type: 'split', direction: 'vertical', splitRatio: 0.65,
      first: pane('tree'), second: pane('terminal'),
    };
    const out = mirrorLayout(node) as Extract<LayoutNode, { type: 'split' }>;
    expect(out.first.id).toBe('tree');
    expect(out.second.id).toBe('terminal');
    expect(out.splitRatio).toBe(0.65);
  });

  it('reverses columns while preserving the stacks inside them', () => {
    const node: LayoutNode = {
      id: 'root', type: 'split', direction: 'horizontal', splitRatio: 0.5,
      first: pane('editor'),
      second: {
        id: 'col', type: 'split', direction: 'vertical', splitRatio: 0.65,
        first: pane('tree'), second: pane('terminal'),
      },
    };
    const out = mirrorLayout(node) as Extract<LayoutNode, { type: 'split' }>;
    expect(out.first.id).toBe('col');
    expect(out.second.id).toBe('editor');
    const col = out.first as Extract<LayoutNode, { type: 'split' }>;
    expect([col.first.id, col.second.id]).toEqual(['tree', 'terminal']);
  });

  it('is pure and its own inverse', () => {
    const node: LayoutNode = {
      id: 'root', type: 'split', direction: 'horizontal', splitRatio: 0.3,
      first: pane('a'),
      second: {
        id: 'inner', type: 'split', direction: 'horizontal', splitRatio: 0.4,
        first: pane('b'), second: pane('c'),
      },
    };
    const snapshot = JSON.stringify(node);
    const back = mirrorLayout(mirrorLayout(node)) as Extract<LayoutNode, { type: 'split' }>;

    // Structure, and ratios to within float error: `1 - (1 - 0.3)` is
    // 0.30000000000000004, so a JSON compare would fail on arithmetic rather
    // than on the property under test.
    expect(leafIds(back)).toEqual(leafIds(node));
    expect(back.splitRatio).toBeCloseTo(0.3, 10);
    expect((back.second as Extract<LayoutNode, { type: 'split' }>).splitRatio).toBeCloseTo(0.4, 10);
    // Pure: the input is untouched.
    expect(JSON.stringify(node)).toBe(snapshot);
  });
});
