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


describe('the last centre pane remains reusable after a tab leaves', () => {
  beforeEach(() => { configureWindowing(neutralPolicy()); stackRightRail(); });

  it('keeps its empty group through swap, close, swap, reopen and a subsequent tab drop', () => {
    const rail = windowStore.edgePanels.right.layout;
    if (rail.type !== 'split') throw new Error('expected stacked rail');
    const documentPane = rail.first.id;
    const groupId = rail.first.type === 'pane' ? rail.first.tabGroupId! : '';
    windowActions.swapCentreWithEdge('right', documentPane);
    windowActions.removeTab(groupId, 'tab-right');
    expect(windowStore.tabGroups[groupId]).toMatchObject({ tabs: [], activeTabId: null });
    windowActions.swapCentreWithEdge('right', windowStore.edgePanels.right.layout.type === 'split'
      ? windowStore.edgePanels.right.layout.first.id : '');
    windowActions.addTab(groupId, { id: 'reopened', title: 'Reopened', contentType: 'gamma' });
    expect(windowStore.edgePanels.right.layout).toMatchObject({ first: { type: 'pane', id: documentPane, tabGroupId: groupId } });
    const sourceGroup = findFirstPane(windowStore.layout)!.tabGroupId!;
    windowActions.moveTab(sourceGroup, windowActions.getPaneTabGroupId(documentPane)!, 'tab-alpha');
    expect(windowStore.tabGroups[groupId].tabs.map(tab => tab.id)).toEqual(['reopened', 'tab-alpha']);
  });

  it('keeps the final centre group when its last tab is dragged out', () => {
    const pane = findFirstPane(windowStore.layout)!;
    const groupId = pane.tabGroupId!;
    windowActions.removeTab(groupId, 'tab-beta');
    const railGroup = findFirstPane(windowStore.edgePanels.right.layout)!.tabGroupId!;
    windowActions.moveTab(groupId, railGroup, 'tab-alpha');
    expect(windowStore.tabGroups[groupId]).toMatchObject({ tabs: [], activeTabId: null });
    windowActions.moveTab(railGroup, windowActions.getPaneTabGroupId(pane.id)!, 'tab-alpha');
    expect(windowStore.tabGroups[groupId].tabs.map(tab => tab.id)).toEqual(['tab-alpha']);
  });
});
