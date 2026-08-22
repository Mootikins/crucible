import { describe, it, expect, beforeEach } from 'vitest';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState } from '@/stores/windowStoreInternals';
import { primaryEdgeGroupId } from '@/stores/windowStoreInternals';

const leftGroup = () => primaryEdgeGroupId(windowStore, 'left');
const rightGroup = () => primaryEdgeGroupId(windowStore, 'right');

describe('swapSidePanels', () => {
  beforeEach(() => {
    setStore(createInitialState());
  });

  it('moves each side’s panes to the other side', () => {
    const before = { left: leftGroup(), right: rightGroup() };
    windowActions.swapSidePanels();
    expect(leftGroup()).toBe(before.right);
    expect(rightGroup()).toBe(before.left);
  });

  it('is its own inverse while both sides agree on collapse', () => {
    const before = { left: leftGroup(), right: rightGroup() };
    setStore('edgePanels', 'left', 'isCollapsed', false);
    setStore('edgePanels', 'right', 'isCollapsed', false);
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

  // Collapse stays with the SIDE. This is the gesture the feature exists for:
  // right rail stowed, one swap, and the file tree is on the visible left
  // while the session list goes away — "focus on editing, not on a session".
  it('leaves collapse with the side, so a swap reveals the stowed panel', () => {
    const stowed = rightGroup();
    setStore('edgePanels', 'left', 'isCollapsed', false);
    setStore('edgePanels', 'right', 'isCollapsed', true);

    windowActions.swapSidePanels();

    expect(leftGroup()).toBe(stowed);
    expect(windowStore.edgePanels.left.isCollapsed).toBe(false);
    expect(windowStore.edgePanels.right.isCollapsed).toBe(true);
  });

  // The toggles are POSITIONAL — toggleEdgePanel('left') means "the left
  // side", not "the session list" — which is the whole reason a contents-only
  // swap needs no remapping anywhere.
  it('leaves the positional toggles pointing at the right sides', () => {
    windowActions.swapSidePanels();
    const swapped = leftGroup();
    windowActions.toggleEdgePanel('left');
    expect(windowStore.edgePanels.left.isCollapsed).toBe(true);
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
