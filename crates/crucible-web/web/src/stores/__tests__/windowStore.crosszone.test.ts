import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore, windowActions, findEdgePanelForGroup } from '../windowStore';
import type { Tab, EdgePanelPosition, TabGroup, LayoutNode } from '@/types/windowTypes';

function resetToState(overrides: Partial<{
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, {
    id: string;
    tabGroupId: string;
    isCollapsed: boolean;
    width?: number;
    height?: number;
  }>;
  layout: LayoutNode;
  activePaneId: string | null;
  focusedRegion: 'left' | 'right' | 'bottom' | 'center';
  flyoutState: {
    isOpen: boolean;
    panelPosition: EdgePanelPosition;
    tabId: string | null;
  } | null;
}>) {
  setStore(
    produce((s) => {
      if (overrides.tabGroups !== undefined) s.tabGroups = overrides.tabGroups;
      if (overrides.edgePanels !== undefined) s.edgePanels = overrides.edgePanels as any;
      if (overrides.layout !== undefined) s.layout = overrides.layout;
      if (overrides.activePaneId !== undefined) s.activePaneId = overrides.activePaneId;
      if (overrides.focusedRegion !== undefined) s.focusedRegion = overrides.focusedRegion;
      if (overrides.flyoutState !== undefined) s.flyoutState = overrides.flyoutState as any;
      s.dragState = null;
      if (!('flyoutState' in overrides)) s.flyoutState = null;
    })
  );
}

const makeTab = (id: string, title = id): Tab => ({
  id,
  title,
  contentType: 'file',
});

const makeEdgePanel = (position: EdgePanelPosition, tabGroupId: string, isCollapsed = false) => ({
  id: `${position}-panel`,
  tabGroupId,
  isCollapsed,
  ...(position === 'bottom' ? { height: 200 } : { width: 250 }),
});

const makeTabGroup = (id: string, tabs: Tab[], activeTabId: string | null = tabs[0]?.id ?? null): TabGroup => ({
  id,
  tabs,
  activeTabId,
});

const simpleLayout = (paneId: string, groupId: string): LayoutNode => ({
  id: paneId,
  type: 'pane' as const,
  tabGroupId: groupId,
});

const splitLayout = (pane1Id: string, group1Id: string, pane2Id: string, group2Id: string): LayoutNode => ({
  id: 'split-root',
  type: 'split' as const,
  direction: 'horizontal',
  splitRatio: 0.5,
  first: { id: pane1Id, type: 'pane' as const, tabGroupId: group1Id },
  second: { id: pane2Id, type: 'pane' as const, tabGroupId: group2Id },
});

describe('initial state structure', () => {
  it('creates 5 tab groups (2 center + 3 edge)', () => {
    const groupIds = Object.keys(windowStore.tabGroups);
    expect(groupIds).toHaveLength(5);
  });

  it('edgePanels.left.tabGroupId references a group in tabGroups', () => {
    const leftGroupId = windowStore.edgePanels.left.tabGroupId;
    expect(windowStore.tabGroups[leftGroupId]).toBeDefined();
    expect(windowStore.tabGroups[leftGroupId]!.tabs.length).toBeGreaterThan(0);
  });

  it('edgePanels.right.tabGroupId references a group in tabGroups', () => {
    const rightGroupId = windowStore.edgePanels.right.tabGroupId;
    expect(windowStore.tabGroups[rightGroupId]).toBeDefined();
  });

  it('edgePanels.bottom.tabGroupId references a group in tabGroups', () => {
    const bottomGroupId = windowStore.edgePanels.bottom.tabGroupId;
    expect(windowStore.tabGroups[bottomGroupId]).toBeDefined();
  });

  it('edge panels have no position or tabs fields', () => {
    const left = windowStore.edgePanels.left;
    expect(left).not.toHaveProperty('position');
    expect(left).not.toHaveProperty('tabs');
    expect(left).not.toHaveProperty('activeTabId');
  });

  it('edge tab groups contain plain Tab objects without panelPosition', () => {
    const leftGroupId = windowStore.edgePanels.left.tabGroupId;
    const group = windowStore.tabGroups[leftGroupId]!;
    for (const tab of group.tabs) {
      expect(tab).not.toHaveProperty('panelPosition');
    }
  });
});

describe('findEdgePanelForGroup', () => {
  it('returns left for the left panel group', () => {
    const leftGroupId = windowStore.edgePanels.left.tabGroupId;
    expect(findEdgePanelForGroup(leftGroupId)).toBe('left');
  });

  it('returns right for the right panel group', () => {
    const rightGroupId = windowStore.edgePanels.right.tabGroupId;
    expect(findEdgePanelForGroup(rightGroupId)).toBe('right');
  });

  it('returns bottom for the bottom panel group', () => {
    const bottomGroupId = windowStore.edgePanels.bottom.tabGroupId;
    expect(findEdgePanelForGroup(bottomGroupId)).toBe('bottom');
  });

  it('returns null for a center group', () => {
    const edgeGroupIds = new Set([
      windowStore.edgePanels.left.tabGroupId,
      windowStore.edgePanels.right.tabGroupId,
      windowStore.edgePanels.bottom.tabGroupId,
    ]);
    const centerGroupId = Object.keys(windowStore.tabGroups).find(id => !edgeGroupIds.has(id));
    expect(centerGroupId).toBeDefined();
    expect(findEdgePanelForGroup(centerGroupId!)).toBeNull();
  });

  it('returns null for a nonexistent group', () => {
    expect(findEdgePanelForGroup('nonexistent-group')).toBeNull();
  });
});

describe('moveTab: edge → center', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1'), makeTab('center-2')], 'center-1'),
        'left-group': makeTabGroup('left-group', [
          { id: 'left-1', title: 'Explorer', contentType: 'tool' },
          { id: 'left-2', title: 'Search', contentType: 'tool' },
        ], 'left-1'),
        'right-group': makeTabGroup('right-group', [
          { id: 'right-1', title: 'Outline', contentType: 'tool' },
        ], 'right-1'),
        'bottom-group': makeTabGroup('bottom-group', [
          { id: 'bottom-1', title: 'Terminal', contentType: 'terminal' },
        ], 'bottom-1'),
      },
      edgePanels: {
        left: makeEdgePanel('left', 'left-group'),
        right: makeEdgePanel('right', 'right-group'),
        bottom: makeEdgePanel('bottom', 'bottom-group'),
      },
      layout: simpleLayout('pane-1', 'group-1'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('moves tab from edge group to center group', () => {
    windowActions.moveTab('left-group', 'group-1', 'left-1');

    const leftGroup = windowStore.tabGroups['left-group'];
    expect(leftGroup!.tabs).toHaveLength(1);
    expect(leftGroup!.tabs.find((t) => t.id === 'left-1')).toBeUndefined();
    expect(windowStore.tabGroups['group-1']!.tabs).toHaveLength(3);
    expect(windowStore.tabGroups['group-1']!.tabs.find((t) => t.id === 'left-1')).toBeDefined();
    expect(windowStore.tabGroups['group-1']!.activeTabId).toBe('left-1');
  });

  it('sets focusedRegion to center when target is center group', () => {
    windowActions.moveTab('left-group', 'group-1', 'left-1');
    expect(windowStore.focusedRegion).toBe('center');
  });

  it('auto-collapses edge panel when last tab moves out', () => {
    windowActions.moveTab('right-group', 'group-1', 'right-1');

    expect(windowStore.tabGroups['right-group']).toBeDefined();
    expect(windowStore.tabGroups['right-group']!.tabs).toHaveLength(0);
    expect(windowStore.tabGroups['right-group']!.activeTabId).toBeNull();
    expect(windowStore.edgePanels.right.isCollapsed).toBe(true);
  });

  it('preserves edge group when emptied', () => {
    windowActions.moveTab('right-group', 'group-1', 'right-1');
    expect(windowStore.tabGroups['right-group']).toBeDefined();
    expect(windowStore.edgePanels.right.tabGroupId).toBe('right-group');
  });
});

describe('moveTab: center → edge', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1'), makeTab('center-2')], 'center-1'),
        'group-2': makeTabGroup('group-2', [makeTab('center-3')], 'center-3'),
        'left-group': makeTabGroup('left-group', [
          { id: 'left-1', title: 'Explorer', contentType: 'tool' },
        ], 'left-1'),
        'right-group': makeTabGroup('right-group', [], null),
        'bottom-group': makeTabGroup('bottom-group', [], null),
      },
      edgePanels: {
        left: makeEdgePanel('left', 'left-group'),
        right: makeEdgePanel('right', 'right-group', true),
        bottom: makeEdgePanel('bottom', 'bottom-group'),
      },
      layout: splitLayout('pane-1', 'group-1', 'pane-2', 'group-2'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('moves tab from center group to edge group', () => {
    windowActions.moveTab('group-1', 'left-group', 'center-1');

    expect(windowStore.tabGroups['group-1']!.tabs).toHaveLength(1);
    expect(windowStore.tabGroups['left-group']!.tabs).toHaveLength(2);
    expect(windowStore.tabGroups['left-group']!.tabs.find((t) => t.id === 'center-1')).toBeDefined();
  });

  it('moves tab to empty edge group', () => {
    windowActions.moveTab('group-1', 'right-group', 'center-1');

    expect(windowStore.tabGroups['right-group']!.tabs).toHaveLength(1);
    expect(windowStore.tabGroups['right-group']!.tabs[0]!.id).toBe('center-1');
  });

  it('sets focusedRegion to edge position when target is edge group', () => {
    windowActions.moveTab('group-1', 'left-group', 'center-1');
    expect(windowStore.focusedRegion).toBe('left');
  });

  it('expands collapsed edge panel when receiving a tab', () => {
    expect(windowStore.edgePanels.right.isCollapsed).toBe(true);
    windowActions.moveTab('group-1', 'right-group', 'center-1');
    expect(windowStore.edgePanels.right.isCollapsed).toBe(false);
  });

  it('deletes center group and collapses layout when last center tab moves out', () => {
    windowActions.moveTab('group-2', 'left-group', 'center-3');

    expect(windowStore.tabGroups['group-2']).toBeUndefined();
    expect(windowStore.layout.type).toBe('pane');
  });
});

describe('moveTab: edge → edge', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1')]),
        'left-group': makeTabGroup('left-group', [
          { id: 'left-1', title: 'Explorer', contentType: 'tool' },
          { id: 'left-2', title: 'Search', contentType: 'tool' },
        ], 'left-1'),
        'right-group': makeTabGroup('right-group', [
          { id: 'right-1', title: 'Outline', contentType: 'tool' },
        ], 'right-1'),
        'bottom-group': makeTabGroup('bottom-group', [], null),
      },
      edgePanels: {
        left: makeEdgePanel('left', 'left-group'),
        right: makeEdgePanel('right', 'right-group'),
        bottom: makeEdgePanel('bottom', 'bottom-group', true),
      },
      layout: simpleLayout('pane-1', 'group-1'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('moves tab between edge groups', () => {
    windowActions.moveTab('left-group', 'bottom-group', 'left-1');

    expect(windowStore.tabGroups['left-group']!.tabs).toHaveLength(1);
    expect(windowStore.tabGroups['bottom-group']!.tabs).toHaveLength(1);
    expect(windowStore.tabGroups['bottom-group']!.tabs[0]!.id).toBe('left-1');
  });

  it('sets focusedRegion to target edge position', () => {
    windowActions.moveTab('left-group', 'right-group', 'left-1');
    expect(windowStore.focusedRegion).toBe('right');
  });

  it('auto-collapses source edge panel when emptied', () => {
    windowActions.moveTab('right-group', 'left-group', 'right-1');

    expect(windowStore.tabGroups['right-group']!.tabs).toHaveLength(0);
    expect(windowStore.edgePanels.right.isCollapsed).toBe(true);
  });

  it('expands collapsed target edge panel', () => {
    expect(windowStore.edgePanels.bottom.isCollapsed).toBe(true);
    windowActions.moveTab('left-group', 'bottom-group', 'left-1');
    expect(windowStore.edgePanels.bottom.isCollapsed).toBe(false);
  });
});

describe('moveTab: same-group reorder', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('a'), makeTab('b'), makeTab('c')], 'a'),
        'left-group': makeTabGroup('left-group', [
          { id: 'l1', title: 'L1', contentType: 'tool' },
          { id: 'l2', title: 'L2', contentType: 'tool' },
          { id: 'l3', title: 'L3', contentType: 'tool' },
        ], 'l1'),
        'right-group': makeTabGroup('right-group', [], null),
        'bottom-group': makeTabGroup('bottom-group', [], null),
      },
      edgePanels: {
        left: makeEdgePanel('left', 'left-group'),
        right: makeEdgePanel('right', 'right-group'),
        bottom: makeEdgePanel('bottom', 'bottom-group'),
      },
      layout: simpleLayout('pane-1', 'group-1'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('reorders within center group', () => {
    windowActions.moveTab('group-1', 'group-1', 'c', 0);

    const tabs = windowStore.tabGroups['group-1']!.tabs;
    expect(tabs.map(t => t.id)).toEqual(['c', 'a', 'b']);
    expect(windowStore.tabGroups['group-1']!.activeTabId).toBe('c');
  });

  it('reorders within edge group', () => {
    windowActions.moveTab('left-group', 'left-group', 'l3', 0);

    const tabs = windowStore.tabGroups['left-group']!.tabs;
    expect(tabs.map(t => t.id)).toEqual(['l3', 'l1', 'l2']);
  });
});

describe('moveTab: flyout guard', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1')]),
        'left-group': makeTabGroup('left-group', [
          { id: 'left-1', title: 'Explorer', contentType: 'tool' },
          { id: 'left-2', title: 'Search', contentType: 'tool' },
        ], 'left-1'),
        'right-group': makeTabGroup('right-group', [], null),
        'bottom-group': makeTabGroup('bottom-group', [], null),
      },
      edgePanels: {
        left: makeEdgePanel('left', 'left-group'),
        right: makeEdgePanel('right', 'right-group'),
        bottom: makeEdgePanel('bottom', 'bottom-group'),
      },
      layout: simpleLayout('pane-1', 'group-1'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
      flyoutState: { isOpen: true, panelPosition: 'left', tabId: 'left-1' },
    });
  });

  it('dismisses flyout when moved tab matches flyoutState.tabId', () => {
    expect(windowStore.flyoutState).not.toBeNull();
    windowActions.moveTab('left-group', 'group-1', 'left-1');
    expect(windowStore.flyoutState).toBeNull();
  });

  it('preserves flyout when moved tab does not match', () => {
    windowActions.moveTab('left-group', 'group-1', 'left-2');
    expect(windowStore.flyoutState).not.toBeNull();
  });
});

describe('removeTab: edge-aware', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1'), makeTab('center-2')], 'center-1'),
        'group-2': makeTabGroup('group-2', [makeTab('center-3')], 'center-3'),
        'left-group': makeTabGroup('left-group', [
          { id: 'left-1', title: 'Explorer', contentType: 'tool' },
        ], 'left-1'),
        'right-group': makeTabGroup('right-group', [
          { id: 'right-1', title: 'Outline', contentType: 'tool' },
          { id: 'right-2', title: 'Debug', contentType: 'tool' },
        ], 'right-1'),
        'bottom-group': makeTabGroup('bottom-group', [], null),
      },
      edgePanels: {
        left: makeEdgePanel('left', 'left-group'),
        right: makeEdgePanel('right', 'right-group'),
        bottom: makeEdgePanel('bottom', 'bottom-group'),
      },
      layout: splitLayout('pane-1', 'group-1', 'pane-2', 'group-2'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
      flyoutState: { isOpen: true, panelPosition: 'left', tabId: 'left-1' },
    });
  });

  it('collapses edge panel when last edge tab is removed', () => {
    windowActions.removeTab('left-group', 'left-1');

    expect(windowStore.tabGroups['left-group']).toBeDefined();
    expect(windowStore.tabGroups['left-group']!.tabs).toHaveLength(0);
    expect(windowStore.tabGroups['left-group']!.activeTabId).toBeNull();
    expect(windowStore.edgePanels.left.isCollapsed).toBe(true);
  });

  it('does not delete edge group when emptied', () => {
    windowActions.removeTab('left-group', 'left-1');
    expect(windowStore.tabGroups['left-group']).toBeDefined();
    expect(windowStore.edgePanels.left.tabGroupId).toBe('left-group');
  });

  it('removes non-last edge tab without collapsing', () => {
    windowActions.removeTab('right-group', 'right-1');

    expect(windowStore.tabGroups['right-group']!.tabs).toHaveLength(1);
    expect(windowStore.edgePanels.right.isCollapsed).toBe(false);
  });

  it('deletes center group and collapses layout when last center tab removed', () => {
    windowActions.removeTab('group-2', 'center-3');

    expect(windowStore.tabGroups['group-2']).toBeUndefined();
    expect(windowStore.layout.type).toBe('pane');
  });

  it('removes non-last center tab normally', () => {
    windowActions.removeTab('group-1', 'center-1');

    expect(windowStore.tabGroups['group-1']!.tabs).toHaveLength(1);
    expect(windowStore.tabGroups['group-1']!.activeTabId).toBe('center-2');
  });

  it('dismisses flyout when removed tab matches flyoutState.tabId', () => {
    expect(windowStore.flyoutState).not.toBeNull();
    windowActions.removeTab('left-group', 'left-1');
    expect(windowStore.flyoutState).toBeNull();
  });

  it('preserves flyout when removed tab does not match', () => {
    windowActions.removeTab('right-group', 'right-1');
    expect(windowStore.flyoutState).not.toBeNull();
  });
});

describe('setEdgePanelActiveTab', () => {
  it('sets activeTabId on the edge group via tabGroups', () => {
    const leftGroupId = windowStore.edgePanels.left.tabGroupId;
    const group = windowStore.tabGroups[leftGroupId]!;
    const secondTab = group.tabs[1];
    if (!secondTab) return;

    windowActions.setEdgePanelActiveTab('left', secondTab.id);
    expect(windowStore.tabGroups[leftGroupId]!.activeTabId).toBe(secondTab.id);
    expect(windowStore.focusedRegion).toBe('left');
  });
});
