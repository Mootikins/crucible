import { describe, it, expect, beforeEach } from 'vitest';
import { configureWindowing, windowStore, windowActions, setStore } from '@/windowing/store';
import { findFirstPane } from '@/windowing/model/tree';
import { neutralPolicy } from '@/windowing/testing/neutralPolicy';
import { stackRightRail } from '@/windowing/testing/stackedRail';

/**
 * The centre and a rail trade their whole layouts: a swap of places, not a
 * change of where new tabs go.
 */

const snapshot = () => JSON.parse(JSON.stringify({ centre: windowStore.layout, right: windowStore.edgePanels.right.layout }));

describe('swapCentreWithEdge', () => {
  beforeEach(() => {
    configureWindowing(neutralPolicy());
    stackRightRail();
  });

  it('moves the rail column into the centre, splits and collapse included', () => {
    const before = snapshot();
    windowActions.swapCentreWithEdge('right');
    expect(windowStore.layout).toEqual(before.right);
    expect(windowStore.edgePanels.right.layout).toEqual(before.centre);
    const split = windowStore.layout;
    expect(split.type === 'split' && split.second.type === 'pane' && split.second.collapsed).toBe(true);
  });

  it('is its own inverse', () => {
    const before = snapshot();
    windowActions.swapCentreWithEdge('right');
    windowActions.swapCentreWithEdge('right');
    expect(snapshot()).toEqual(before);
  });

  it('keeps the rail width, and docks a stowed rail so the swap shows', () => {
    setStore('edgePanels', 'right', 'width', 333);
    windowActions.setEdgePanelCollapsed('right', true);
    windowActions.swapCentreWithEdge('right');
    expect(windowStore.edgePanels.right.width).toBe(333);
    expect(windowStore.edgePanels.right.mode).toBe('docked');
  });

  it('ends an expand, which would cover the centre it just filled', () => {
    windowActions.expandEdge('right');
    windowActions.swapCentreWithEdge('right');
    expect(windowStore.expandedEdge).toBeNull();
  });

  it('keeps focus on its pane, and finds that pane on its new side', () => {
    const centrePane = findFirstPane(windowStore.layout)!.id;
    windowActions.setActivePane(centrePane);
    expect(windowStore.focusedRegion).toBe('center');
    windowActions.swapCentreWithEdge('right');
    expect(windowStore.activePaneId).toBe(centrePane);
    expect(windowStore.focusedRegion).toBe('right');
  });
});
