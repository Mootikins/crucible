import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore, windowActions, findEdgePanelForGroup } from '../windowStore';
import { createInitialState, primaryEdgeGroupId } from '@/stores/windowStoreInternals';
import type { Tab, EdgePanelPosition, TabGroup, LayoutNode } from '@/types/windowTypes';

const LEGACY_EDGE_TAB_FIELD = 'panel' + 'Position';

/** First leaf group of an edge panel — the single group, pre-splits. */
const edgeGroup = (pos: EdgePanelPosition) => primaryEdgeGroupId(windowStore, pos)!;

function resetToState(overrides: Partial<{
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, {
    id: string;
    layout: LayoutNode;
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
  layout: { id: `${position}-pane`, type: 'pane' as const, tabGroupId },
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

// The windowStore is a module-level singleton. The mutating describes below
// seed it via resetToState() in their own beforeEach, but the read-only
// "initial state structure" and "findEdgePanelForGroup" describes assert
// against the pristine default — which only held because they happened to run
// first. Reset every test to a fresh createInitialState() so their assertions
// are independent of execution order.
beforeEach(() => {
  const fresh = createInitialState();
  setStore(
    produce((s) => {
      s.layout = fresh.layout;
      s.tabGroups = fresh.tabGroups;
      s.edgePanels = fresh.edgePanels;
      s.floatingWindows = fresh.floatingWindows;
      s.activePaneId = fresh.activePaneId;
      s.focusedRegion = fresh.focusedRegion;
      s.nextZIndex = fresh.nextZIndex;
    })
  );
});

describe('initial state structure', () => {
  it('creates 4 tab groups (1 center + 3 edge)', () => {
    const groupIds = Object.keys(windowStore.tabGroups);
    expect(groupIds).toHaveLength(4);
  });

  it('edgePanels.left.tabGroupId references a group in tabGroups', () => {
    const leftGroupId = edgeGroup('left');
    expect(windowStore.tabGroups[leftGroupId]).toBeDefined();
    expect(windowStore.tabGroups[leftGroupId]!.tabs.length).toBeGreaterThan(0);
  });

  it('edgePanels.right.tabGroupId references a group in tabGroups', () => {
    const rightGroupId = edgeGroup('right');
    expect(windowStore.tabGroups[rightGroupId]).toBeDefined();
  });

  it('edgePanels.bottom.tabGroupId references a group in tabGroups', () => {
    const bottomGroupId = edgeGroup('bottom');
    expect(windowStore.tabGroups[bottomGroupId]).toBeDefined();
  });

  it('edge panels have no position or tabs fields', () => {
    const left = windowStore.edgePanels.left;
    expect(left).not.toHaveProperty('position');
    expect(left).not.toHaveProperty('tabs');
    expect(left).not.toHaveProperty('activeTabId');
  });

  it('edge tab groups contain plain Tab objects without legacy edge metadata', () => {
    const leftGroupId = edgeGroup('left');
    const group = windowStore.tabGroups[leftGroupId]!;
    for (const tab of group.tabs) {
      expect(tab).not.toHaveProperty(LEGACY_EDGE_TAB_FIELD);
    }
  });
});

describe('findEdgePanelForGroup', () => {
  it('returns left for the left panel group', () => {
    const leftGroupId = edgeGroup('left');
    expect(findEdgePanelForGroup(leftGroupId)).toBe('left');
  });

  it('returns right for the right panel group', () => {
    const rightGroupId = edgeGroup('right');
    expect(findEdgePanelForGroup(rightGroupId)).toBe('right');
  });

  it('returns bottom for the bottom panel group', () => {
    const bottomGroupId = edgeGroup('bottom');
    expect(findEdgePanelForGroup(bottomGroupId)).toBe('bottom');
  });

  it('returns null for a center group', () => {
    const edgeGroupIds = new Set([
      edgeGroup('left'),
      edgeGroup('right'),
      edgeGroup('bottom'),
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
    expect(edgeGroup('right')).toBe('right-group');
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
    expect(edgeGroup('left')).toBe('left-group');
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

});

describe('setEdgePanelActiveTab', () => {
  it('sets activeTabId on the edge group via tabGroups', () => {
    const leftGroupId = edgeGroup('left');
    const group = windowStore.tabGroups[leftGroupId]!;
    const secondTab = group.tabs[1];
    if (!secondTab) return;

    windowActions.setEdgePanelActiveTab('left', secondTab.id);
    expect(windowStore.tabGroups[leftGroupId]!.activeTabId).toBe(secondTab.id);
    expect(windowStore.focusedRegion).toBe('left');
  });
});

describe('edge panel split trees (v5 model)', () => {
  const seedSplitBottom = () => {
    resetToState({
      tabGroups: {
        'g-center': makeTabGroup('g-center', [makeTab('c1')], 'c1'),
        'g-b1': makeTabGroup('g-b1', [
          { id: 'term-1', title: 'Terminal', contentType: 'terminal' },
        ], 'term-1'),
        'g-b2': makeTabGroup('g-b2', [
          { id: 'chat-1', title: 'Chat', contentType: 'tool' },
        ], 'chat-1'),
      },
      edgePanels: {
        left: makeEdgePanel('left', 'g-center-unused-left'),
        right: makeEdgePanel('right', 'g-center-unused-right'),
        bottom: {
          id: 'bottom-panel',
          layout: {
            id: 'bottom-split',
            type: 'split' as const,
            direction: 'horizontal' as const,
            splitRatio: 0.5,
            first: { id: 'pane-b1', type: 'pane' as const, tabGroupId: 'g-b1' },
            second: { id: 'pane-b2', type: 'pane' as const, tabGroupId: 'g-b2' },
          },
          isCollapsed: false,
          height: 200,
        },
      },
      layout: simpleLayout('pane-center', 'g-center'),
      activePaneId: 'pane-b2',
      focusedRegion: 'bottom',
    });
  };

  it('splitPane routes into the edge tree and focuses the edge region', () => {
    resetToState({
      tabGroups: {
        'g-center': makeTabGroup('g-center', [makeTab('c1')], 'c1'),
        'g-left': makeTabGroup('g-left', [makeTab('l1')], 'l1'),
      },
      edgePanels: {
        left: makeEdgePanel('left', 'g-left'),
        right: makeEdgePanel('right', 'g-r'),
        bottom: makeEdgePanel('bottom', 'g-b'),
      },
      layout: simpleLayout('pane-center', 'g-center'),
      activePaneId: 'pane-center',
      focusedRegion: 'center',
    });

    windowActions.splitPane('left-pane', 'vertical');

    expect(windowStore.edgePanels.left.layout.type).toBe('split');
    // Center tiling untouched.
    expect(windowStore.layout.type).toBe('pane');
    expect(windowStore.focusedRegion).toBe('left');
  });

  it('commitSplitRatio finds a split living inside an edge panel', () => {
    seedSplitBottom();
    windowActions.commitSplitRatio('bottom-split', 0.3);
    const layout = windowStore.edgePanels.bottom.layout;
    expect(layout.type).toBe('split');
    if (layout.type === 'split') expect(layout.splitRatio).toBe(0.3);
  });

  it('removing the last tab of one pane in a multi-pane edge panel collapses that pane out, keeps the panel expanded, and re-points activePaneId', () => {
    seedSplitBottom();
    windowActions.removeTab('g-b2', 'chat-1');

    // The emptied pane collapsed out of the tree; its group is gone.
    const layout = windowStore.edgePanels.bottom.layout;
    expect(layout).toMatchObject({ type: 'pane', tabGroupId: 'g-b1' });
    expect(windowStore.tabGroups['g-b2']).toBeUndefined();
    // Panel stays expanded (it still has content), unlike the sole-pane case.
    expect(windowStore.edgePanels.bottom.isCollapsed).toBe(false);
    // activePaneId pointed at the collapsed pane — must be re-pointed, or
    // every keyboard shortcut dead-ends on a pane that exists in no tree.
    expect(windowStore.activePaneId).toBe('pane-b1');
  });

  it('sole-pane edge panels keep the old behavior: empty group survives, panel collapses', () => {
    resetToState({
      tabGroups: {
        'g-center': makeTabGroup('g-center', [makeTab('c1')], 'c1'),
        'g-solo': makeTabGroup('g-solo', [makeTab('solo-tab')], 'solo-tab'),
      },
      edgePanels: {
        left: makeEdgePanel('left', 'g-solo'),
        right: makeEdgePanel('right', 'g-r'),
        bottom: makeEdgePanel('bottom', 'g-b'),
      },
      layout: simpleLayout('pane-center', 'g-center'),
      activePaneId: 'pane-center',
      focusedRegion: 'center',
    });

    windowActions.removeTab('g-solo', 'solo-tab');

    expect(windowStore.tabGroups['g-solo']).toMatchObject({ tabs: [], activeTabId: null });
    expect(windowStore.edgePanels.left.isCollapsed).toBe(true);
    expect(windowStore.edgePanels.left.layout).toMatchObject({ type: 'pane', tabGroupId: 'g-solo' });
  });
});
