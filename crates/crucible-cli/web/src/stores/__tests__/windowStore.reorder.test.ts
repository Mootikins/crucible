import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore, windowActions } from '../windowStore';
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

describe('reorder tabs in edge tab groups via moveTab', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1')]),
        'left-group': makeTabGroup('left-group', [
          { id: 'left-1', title: 'Explorer', contentType: 'tool' },
          { id: 'left-2', title: 'Search', contentType: 'tool' },
          { id: 'left-3', title: 'Source Control', contentType: 'tool' },
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

  it('reorders first tab to last position via moveTab', () => {
    windowActions.moveTab('left-group', 'left-group', 'left-1', 2);

    const group = windowStore.tabGroups['left-group']!;
    expect(group.tabs).toHaveLength(3);
    expect(group.tabs[0]!.id).toBe('left-2');
    expect(group.tabs[1]!.id).toBe('left-3');
    expect(group.tabs[2]!.id).toBe('left-1');
  });

  it('reorders last tab to first position via moveTab', () => {
    windowActions.moveTab('left-group', 'left-group', 'left-3', 0);

    const group = windowStore.tabGroups['left-group']!;
    expect(group.tabs).toHaveLength(3);
    expect(group.tabs[0]!.id).toBe('left-3');
    expect(group.tabs[1]!.id).toBe('left-1');
    expect(group.tabs[2]!.id).toBe('left-2');
  });

  it('preserves tab properties during reorder', () => {
    const tabBefore = windowStore.tabGroups['left-group']!.tabs[0]!;

    windowActions.moveTab('left-group', 'left-group', 'left-1', 2);

    const tabAfter = windowStore.tabGroups['left-group']!.tabs[2]!;
    expect(tabAfter.id).toBe(tabBefore.id);
    expect(tabAfter.title).toBe(tabBefore.title);
    expect(tabAfter.contentType).toBe(tabBefore.contentType);
  });

  it('works with single-tab group (no-op)', () => {
    windowActions.moveTab('right-group', 'right-group', 'right-1', 0);

    expect(windowStore.tabGroups['right-group']!.tabs).toHaveLength(1);
    expect(windowStore.tabGroups['right-group']!.tabs[0]!.id).toBe('right-1');
  });

  it('reorders in different edge groups independently', () => {
    windowActions.moveTab('left-group', 'left-group', 'left-1', 2);

    expect(windowStore.tabGroups['left-group']!.tabs[2]!.id).toBe('left-1');
    expect(windowStore.tabGroups['right-group']!.tabs[0]!.id).toBe('right-1');
  });

  it('reorders in a two-tab group', () => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1')]),
        'two-tab-group': makeTabGroup('two-tab-group', [makeTab('A'), makeTab('B')], 'A'),
        'left-group': makeTabGroup('left-group', []),
        'right-group': makeTabGroup('right-group', []),
        'bottom-group': makeTabGroup('bottom-group', []),
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

    windowActions.moveTab('two-tab-group', 'two-tab-group', 'A', 1);

    const group = windowStore.tabGroups['two-tab-group']!;
    expect(group.tabs).toHaveLength(2);
    expect(group.tabs[0]!.id).toBe('B');
    expect(group.tabs[1]!.id).toBe('A');
  });

  it('no-op when reordering tab to its current position', () => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1')]),
        'noop-group': makeTabGroup('noop-group', [makeTab('A'), makeTab('B'), makeTab('C')], 'A'),
        'left-group': makeTabGroup('left-group', []),
        'right-group': makeTabGroup('right-group', []),
        'bottom-group': makeTabGroup('bottom-group', []),
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

    windowActions.moveTab('noop-group', 'noop-group', 'A', 0);

    const group = windowStore.tabGroups['noop-group']!;
    expect(group.tabs).toHaveLength(3);
    expect(group.tabs[0]!.id).toBe('A');
    expect(group.tabs[1]!.id).toBe('B');
    expect(group.tabs[2]!.id).toBe('C');
    expect(group.activeTabId).toBe('A');
  });

  it('reorders last tab to middle position in edge group', () => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1')]),
        'left-group': makeTabGroup('left-group', [makeTab('L1'), makeTab('L2'), makeTab('L3')], 'L1'),
        'right-group': makeTabGroup('right-group', []),
        'bottom-group': makeTabGroup('bottom-group', []),
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

    windowActions.moveTab('left-group', 'left-group', 'L3', 1);

    const group = windowStore.tabGroups['left-group']!;
    expect(group.tabs).toHaveLength(3);
    expect(group.tabs[0]!.id).toBe('L1');
    expect(group.tabs[1]!.id).toBe('L3');
    expect(group.tabs[2]!.id).toBe('L2');
  });
});
