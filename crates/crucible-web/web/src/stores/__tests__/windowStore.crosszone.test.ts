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
}>) {
  setStore(
    produce((s) => {
      if (overrides.tabGroups !== undefined) s.tabGroups = overrides.tabGroups;
      if (overrides.edgePanels !== undefined) s.edgePanels = overrides.edgePanels as any;
      if (overrides.layout !== undefined) s.layout = overrides.layout;
      if (overrides.activePaneId !== undefined) s.activePaneId = overrides.activePaneId;
      if (overrides.focusedRegion !== undefined) s.focusedRegion = overrides.focusedRegion;
      s.dragState = null;
      s.flyoutState = null;
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

describe('moveEdgeTabToCenter (legacy — tests cross-zone actions)', () => {
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

  it('moves tab from edge group to center group via moveTab', () => {
    windowActions.moveTab('left-group', 'group-1', 'left-1');

    const leftGroup = windowStore.tabGroups['left-group'];
    expect(leftGroup!.tabs).toHaveLength(1);
    expect(leftGroup!.tabs.find((t) => t.id === 'left-1')).toBeUndefined();
    expect(windowStore.tabGroups['group-1']!.tabs).toHaveLength(3);
    expect(windowStore.tabGroups['group-1']!.tabs.find((t) => t.id === 'left-1')).toBeDefined();
    expect(windowStore.tabGroups['group-1']!.activeTabId).toBe('left-1');
  });
});

describe('moveEdgeTabToEdge (legacy — tests cross-zone via moveTab)', () => {
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
        bottom: makeEdgePanel('bottom', 'bottom-group'),
      },
      layout: simpleLayout('pane-1', 'group-1'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('moves tab between edge groups via moveTab', () => {
    windowActions.moveTab('left-group', 'bottom-group', 'left-1');

    expect(windowStore.tabGroups['left-group']!.tabs).toHaveLength(1);
    expect(windowStore.tabGroups['bottom-group']!.tabs).toHaveLength(1);
    expect(windowStore.tabGroups['bottom-group']!.tabs[0]!.id).toBe('left-1');
  });
});

describe('moveCenterTabToEdge (legacy — tests cross-zone via moveTab)', () => {
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
        right: makeEdgePanel('right', 'right-group'),
        bottom: makeEdgePanel('bottom', 'bottom-group'),
      },
      layout: splitLayout('pane-1', 'group-1', 'pane-2', 'group-2'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('moves tab from center group to edge group via moveTab', () => {
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
});


