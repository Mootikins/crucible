import { describe, it, expect, beforeEach } from 'vitest';
import { configureWindowing, windowStore, windowActions } from '@/windowing/store';
import { neutralPolicy } from '@/windowing/testing/neutralPolicy';
import { findFirstPane, primaryEdgeGroupId } from '@/windowing/model/tree';

/** The neutral seed's centre group and pane. */
const centre = () => {
  const pane = findFirstPane(windowStore.layout)!;
  return { paneId: pane.id, groupId: pane.tabGroupId! };
};
/** The neutral seed's right-rail group: it holds `tab-right`. */
const rightGroup = () => primaryEdgeGroupId(windowStore, 'right')!;

describe('expanding a rail over the centre', () => {
  beforeEach(() => configureWindowing(neutralPolicy()));

  it('starts with no rail expanded and the toggle exit', () => {
    expect(windowStore.expandedEdge).toBeNull();
    expect(windowStore.expandExit).toBe('toggle');
  });

  it('docks a stowed rail, expands it and moves focus to it', () => {
    // The neutral seed stows the right rail.
    expect(windowStore.edgePanels.right.mode).toBe('strip');
    windowActions.expandEdge('right');
    expect(windowStore.edgePanels.right.mode).toBe('docked');
    expect(windowStore.expandedEdge).toBe('right');
    expect(windowStore.focusedRegion).toBe('right');
  });

  it('toggles on and off, and a toggle of the other rail moves the expand', () => {
    windowActions.toggleEdgeExpanded('right');
    expect(windowStore.expandedEdge).toBe('right');
    windowActions.toggleEdgeExpanded('left');
    expect(windowStore.expandedEdge).toBe('left');
    windowActions.toggleEdgeExpanded('left');
    expect(windowStore.expandedEdge).toBeNull();
  });

  it('with the toggle exit, focus in the centre keeps the expand', () => {
    windowActions.expandEdge('right');
    windowActions.setActiveTab(centre().groupId, 'tab-beta');
    windowActions.setActivePane(centre().paneId);
    expect(windowStore.expandedEdge).toBe('right');
  });

  it('with the centre-focus exit, a centre tab taking focus ends the expand', () => {
    windowActions.setExpandExit('centre-focus');
    windowActions.expandEdge('right');
    windowActions.setActiveTab(centre().groupId, 'tab-beta');
    expect(windowStore.expandedEdge).toBeNull();
  });

  it('with the centre-focus exit, a centre pane taking focus ends the expand', () => {
    windowActions.setExpandExit('centre-focus');
    windowActions.expandEdge('right');
    windowActions.setActivePane(centre().paneId);
    expect(windowStore.expandedEdge).toBeNull();
  });

  it('with the centre-focus exit, focus inside the expanded rail keeps it', () => {
    windowActions.setExpandExit('centre-focus');
    windowActions.expandEdge('right');
    windowActions.setActiveTab(rightGroup(), 'tab-right');
    expect(windowStore.expandedEdge).toBe('right');
  });

  it('with the centre-focus exit, a floating window taking focus keeps the expand', () => {
    // A peek floats over the expanded rail. The store files a floating
    // group's focus under `center`, but it is not the centre tiling.
    windowActions.setExpandExit('centre-focus');
    windowActions.expandEdge('right');
    const peekGroup = windowActions.createTabGroup();
    windowActions.addTab(peekGroup, { id: 'tab-peek', title: 'Peek', contentType: 'beta' });
    windowActions.createFloatingWindow(peekGroup, 40, 40, 320, 240, { transient: true });
    windowActions.setActiveTab(peekGroup, 'tab-peek');
    expect(windowStore.focusedRegion).toBe('center');
    expect(windowStore.expandedEdge).toBe('right');
  });

  it.each([
    ['toggleEdgePanel', () => windowActions.toggleEdgePanel('right')],
    ['setEdgePanelCollapsed', () => windowActions.setEdgePanelCollapsed('right', true)],
    ['setEdgeMode', () => windowActions.setEdgeMode('right', 'flyout')],
  ])('stowing the expanded rail with %s gives the centre back', (_name, stow) => {
    windowActions.expandEdge('right');
    stow();
    expect(windowStore.expandedEdge).toBeNull();
  });

  it.each(['close', 'move', 'pop-out'] as const)('%s of the final expanded rail tab restores the centre', (action) => {
    windowActions.expandEdge('right');
    const group = rightGroup();
    if (action === 'close') windowActions.removeTab(group, 'tab-right');
    if (action === 'move') windowActions.moveTab(group, centre().groupId, 'tab-right');
    if (action === 'pop-out') windowActions.popOutPane(findFirstPane(windowStore.edgePanels.right.layout)!.id, 'tab-right');
    expect(windowStore.edgePanels.right.mode).toBe('strip');
    expect(windowStore.expandedEdge).toBeNull();
  });

  it('stowing the OTHER rail keeps the expand', () => {
    windowActions.expandEdge('right');
    windowActions.toggleEdgePanel('left');
    expect(windowStore.expandedEdge).toBe('right');
  });

  it('a swap carries the expand with the rail', () => {
    windowActions.expandEdge('right');
    windowActions.swapSidePanels();
    expect(windowStore.expandedEdge).toBe('left');
    windowActions.swapSidePanels();
    expect(windowStore.expandedEdge).toBe('right');
  });

  it('a reset clears the expand and keeps the exit setting', () => {
    windowActions.setExpandExit('centre-focus');
    windowActions.expandEdge('right');
    windowActions.resetLayoutToDefaults();
    expect(windowStore.expandedEdge).toBeNull();
    expect(windowStore.expandExit).toBe('centre-focus');
  });

  it('a restore clears the expand and is not stored in the layout', () => {
    windowActions.expandEdge('right');
    const saved = windowActions.exportLayout();
    expect(JSON.stringify(saved)).not.toContain('expandedEdge');
    windowActions.importLayout(saved);
    expect(windowStore.expandedEdge).toBeNull();
  });
});
