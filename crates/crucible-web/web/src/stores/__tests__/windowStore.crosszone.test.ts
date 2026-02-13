import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore, windowActions } from '../windowStore';
import type { Tab, EdgePanelTab, EdgePanelPosition, TabGroup, LayoutNode } from '@/types/windowTypes';

function resetToState(overrides: Partial<{
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, {
    id: string;
    position: EdgePanelPosition;
    tabs: EdgePanelTab[];
    activeTabId: string | null;
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

const makeEdgeTab = (id: string, position: EdgePanelPosition, title = id): EdgePanelTab => ({
  id,
  title,
  contentType: 'tool',
  panelPosition: position,
});

const makeEdgePanel = (position: EdgePanelPosition, tabs: EdgePanelTab[], activeTabId: string | null = tabs[0]?.id ?? null, isCollapsed = false) => ({
  id: `${position}-panel`,
  position,
  tabs,
  activeTabId,
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

describe('moveEdgeTabToCenter', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1'), makeTab('center-2')], 'center-1'),
      },
      edgePanels: {
        left: makeEdgePanel('left', [
          makeEdgeTab('left-1', 'left', 'Explorer'),
          makeEdgeTab('left-2', 'left', 'Search'),
        ]),
        right: makeEdgePanel('right', [makeEdgeTab('right-1', 'right', 'Outline')]),
        bottom: makeEdgePanel('bottom', [makeEdgeTab('bottom-1', 'bottom', 'Terminal')]),
      },
      layout: simpleLayout('pane-1', 'group-1'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('moves tab from edge panel to center group', () => {
    windowActions.moveEdgeTabToCenter('left', 'left-1', 'group-1');

    expect(windowStore.edgePanels.left.tabs).toHaveLength(1);
    expect(windowStore.edgePanels.left.tabs.find((t) => t.id === 'left-1')).toBeUndefined();
    expect(windowStore.tabGroups['group-1']!.tabs).toHaveLength(3);
    expect(windowStore.tabGroups['group-1']!.tabs.find((t) => t.id === 'left-1')).toBeDefined();
    expect(windowStore.tabGroups['group-1']!.activeTabId).toBe('left-1');
  });

  it('strips panelPosition from promoted tab', () => {
    windowActions.moveEdgeTabToCenter('left', 'left-1', 'group-1');

    const promotedTab = windowStore.tabGroups['group-1']!.tabs.find((t) => t.id === 'left-1')!;
    expect(promotedTab).toBeDefined();
    expect('panelPosition' in promotedTab).toBe(false);
  });

  it('updates edge panel activeTabId when active tab is moved', () => {
    windowActions.moveEdgeTabToCenter('left', 'left-1', 'group-1');

    expect(windowStore.edgePanels.left.activeTabId).toBe('left-2');
  });

  it('preserves edge panel activeTabId when non-active tab is moved', () => {
    windowActions.moveEdgeTabToCenter('left', 'left-2', 'group-1');

    expect(windowStore.edgePanels.left.activeTabId).toBe('left-1');
  });

  it('auto-collapses edge panel when last tab is removed', () => {
    windowActions.moveEdgeTabToCenter('right', 'right-1', 'group-1');

    expect(windowStore.edgePanels.right.tabs).toHaveLength(0);
    expect(windowStore.edgePanels.right.isCollapsed).toBe(true);
    expect(windowStore.edgePanels.right.activeTabId).toBeNull();
  });

  it('sets focusedRegion to center', () => {
    resetToState({
      ...getBaseState(),
      focusedRegion: 'left',
    });

    windowActions.moveEdgeTabToCenter('left', 'left-1', 'group-1');

    expect(windowStore.focusedRegion).toBe('center');
  });

  it('is a no-op when tab not found', () => {
    const leftTabsBefore = windowStore.edgePanels.left.tabs.length;
    const centerTabsBefore = windowStore.tabGroups['group-1']!.tabs.length;

    windowActions.moveEdgeTabToCenter('left', 'nonexistent', 'group-1');

    expect(windowStore.edgePanels.left.tabs).toHaveLength(leftTabsBefore);
    expect(windowStore.tabGroups['group-1']!.tabs).toHaveLength(centerTabsBefore);
  });

  it('is a no-op when target group not found', () => {
    const leftTabsBefore = windowStore.edgePanels.left.tabs.length;

    windowActions.moveEdgeTabToCenter('left', 'left-1', 'nonexistent');

    expect(windowStore.edgePanels.left.tabs).toHaveLength(leftTabsBefore);
  });
});

describe('moveEdgeTabToEdge', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1')]),
      },
      edgePanels: {
        left: makeEdgePanel('left', [
          makeEdgeTab('left-1', 'left', 'Explorer'),
          makeEdgeTab('left-2', 'left', 'Search'),
        ]),
        right: makeEdgePanel('right', [makeEdgeTab('right-1', 'right', 'Outline')]),
        bottom: makeEdgePanel('bottom', []),
      },
      layout: simpleLayout('pane-1', 'group-1'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('moves tab between different edge panels', () => {
    windowActions.moveEdgeTabToEdge('left', 'left-1', 'bottom');

    expect(windowStore.edgePanels.left.tabs).toHaveLength(1);
    expect(windowStore.edgePanels.left.tabs.find((t) => t.id === 'left-1')).toBeUndefined();
    expect(windowStore.edgePanels.bottom.tabs).toHaveLength(1);
    expect(windowStore.edgePanels.bottom.tabs.find((t) => t.id === 'left-1')).toBeDefined();
  });

  it('updates panelPosition to target position', () => {
    windowActions.moveEdgeTabToEdge('left', 'left-1', 'bottom');

    const movedTab = windowStore.edgePanels.bottom.tabs.find((t) => t.id === 'left-1')!;
    expect(movedTab.panelPosition).toBe('bottom');
  });

  it('sets active tab in target panel', () => {
    windowActions.moveEdgeTabToEdge('left', 'left-1', 'bottom');

    expect(windowStore.edgePanels.bottom.activeTabId).toBe('left-1');
  });

  it('updates source activeTabId when active tab is moved', () => {
    windowActions.moveEdgeTabToEdge('left', 'left-1', 'bottom');

    expect(windowStore.edgePanels.left.activeTabId).toBe('left-2');
  });

  it('auto-collapses source panel when last tab is removed', () => {
    windowActions.moveEdgeTabToEdge('right', 'right-1', 'bottom');

    expect(windowStore.edgePanels.right.tabs).toHaveLength(0);
    expect(windowStore.edgePanels.right.isCollapsed).toBe(true);
    expect(windowStore.edgePanels.right.activeTabId).toBeNull();
  });

  it('sets focusedRegion to target position', () => {
    windowActions.moveEdgeTabToEdge('left', 'left-1', 'bottom');

    expect(windowStore.focusedRegion).toBe('bottom');
  });

  it('is a no-op when source and target are the same', () => {
    const leftTabsBefore = [...windowStore.edgePanels.left.tabs];

    windowActions.moveEdgeTabToEdge('left', 'left-1', 'left');

    expect(windowStore.edgePanels.left.tabs).toHaveLength(leftTabsBefore.length);
    expect(windowStore.edgePanels.left.tabs[0]!.id).toBe('left-1');
  });

  it('is a no-op when tab not found', () => {
    const leftTabsBefore = windowStore.edgePanels.left.tabs.length;
    const bottomTabsBefore = windowStore.edgePanels.bottom.tabs.length;

    windowActions.moveEdgeTabToEdge('left', 'nonexistent', 'bottom');

    expect(windowStore.edgePanels.left.tabs).toHaveLength(leftTabsBefore);
    expect(windowStore.edgePanels.bottom.tabs).toHaveLength(bottomTabsBefore);
  });
});

describe('moveCenterTabToEdge', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1'), makeTab('center-2')], 'center-1'),
        'group-2': makeTabGroup('group-2', [makeTab('center-3')], 'center-3'),
      },
      edgePanels: {
        left: makeEdgePanel('left', [makeEdgeTab('left-1', 'left', 'Explorer')]),
        right: makeEdgePanel('right', []),
        bottom: makeEdgePanel('bottom', []),
      },
      layout: splitLayout('pane-1', 'group-1', 'pane-2', 'group-2'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('moves tab from center group to edge panel', () => {
    windowActions.moveCenterTabToEdge('group-1', 'center-1', 'left');

    expect(windowStore.tabGroups['group-1']!.tabs).toHaveLength(1);
    expect(windowStore.tabGroups['group-1']!.tabs.find((t) => t.id === 'center-1')).toBeUndefined();
    expect(windowStore.edgePanels.left.tabs).toHaveLength(2);
    expect(windowStore.edgePanels.left.tabs.find((t) => t.id === 'center-1')).toBeDefined();
  });

  it('adds panelPosition to demoted tab', () => {
    windowActions.moveCenterTabToEdge('group-1', 'center-1', 'left');

    const demotedTab = windowStore.edgePanels.left.tabs.find((t) => t.id === 'center-1')!;
    expect(demotedTab.panelPosition).toBe('left');
  });

  it('sets active tab in target edge panel', () => {
    windowActions.moveCenterTabToEdge('group-1', 'center-1', 'left');

    expect(windowStore.edgePanels.left.activeTabId).toBe('center-1');
  });

  it('updates source group activeTabId when active tab is moved', () => {
    windowActions.moveCenterTabToEdge('group-1', 'center-1', 'left');

    expect(windowStore.tabGroups['group-1']!.activeTabId).toBe('center-2');
  });

  it('preserves source group activeTabId when non-active tab is moved', () => {
    windowActions.moveCenterTabToEdge('group-1', 'center-2', 'left');

    expect(windowStore.tabGroups['group-1']!.activeTabId).toBe('center-1');
  });

  it('deletes group and collapses layout when last tab is removed', () => {
    windowActions.moveCenterTabToEdge('group-2', 'center-3', 'right');

    expect(windowStore.tabGroups['group-2']).toBeUndefined();
    expect(windowStore.edgePanels.right.tabs).toHaveLength(1);
    expect(windowStore.edgePanels.right.tabs[0]!.id).toBe('center-3');
    expect(windowStore.edgePanels.right.tabs[0]!.panelPosition).toBe('right');
  });

  it('sets focusedRegion to target edge position', () => {
    windowActions.moveCenterTabToEdge('group-1', 'center-1', 'bottom');

    expect(windowStore.focusedRegion).toBe('bottom');
  });

  it('is a no-op when source group not found', () => {
    const leftTabsBefore = windowStore.edgePanels.left.tabs.length;

    windowActions.moveCenterTabToEdge('nonexistent', 'center-1', 'left');

    expect(windowStore.edgePanels.left.tabs).toHaveLength(leftTabsBefore);
  });

  it('is a no-op when tab not found in source group', () => {
    const leftTabsBefore = windowStore.edgePanels.left.tabs.length;
    const groupTabsBefore = windowStore.tabGroups['group-1']!.tabs.length;

    windowActions.moveCenterTabToEdge('group-1', 'nonexistent', 'left');

    expect(windowStore.edgePanels.left.tabs).toHaveLength(leftTabsBefore);
    expect(windowStore.tabGroups['group-1']!.tabs).toHaveLength(groupTabsBefore);
  });

  it('moves tab to empty edge panel', () => {
    windowActions.moveCenterTabToEdge('group-1', 'center-1', 'right');

    expect(windowStore.edgePanels.right.tabs).toHaveLength(1);
    expect(windowStore.edgePanels.right.tabs[0]!.id).toBe('center-1');
    expect(windowStore.edgePanels.right.activeTabId).toBe('center-1');
  });
});

function getBaseState() {
  return {
    tabGroups: {
      'group-1': makeTabGroup('group-1', [makeTab('center-1'), makeTab('center-2')], 'center-1'),
    },
    edgePanels: {
      left: makeEdgePanel('left', [
        makeEdgeTab('left-1', 'left', 'Explorer'),
        makeEdgeTab('left-2', 'left', 'Search'),
      ]),
      right: makeEdgePanel('right', [makeEdgeTab('right-1', 'right', 'Outline')]),
      bottom: makeEdgePanel('bottom', [makeEdgeTab('bottom-1', 'bottom', 'Terminal')]),
    },
    layout: simpleLayout('pane-1', 'group-1') as LayoutNode,
    activePaneId: 'pane-1',
    focusedRegion: 'center' as const,
  };
}
