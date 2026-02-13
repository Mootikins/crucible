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

describe('reorderEdgeTab', () => {
  beforeEach(() => {
    resetToState({
      tabGroups: {
        'group-1': makeTabGroup('group-1', [makeTab('center-1')]),
      },
      edgePanels: {
        left: makeEdgePanel('left', [
          makeEdgeTab('left-1', 'left', 'Explorer'),
          makeEdgeTab('left-2', 'left', 'Search'),
          makeEdgeTab('left-3', 'left', 'Source Control'),
        ]),
        right: makeEdgePanel('right', [makeEdgeTab('right-1', 'right', 'Outline')]),
        bottom: makeEdgePanel('bottom', []),
      },
      layout: simpleLayout('pane-1', 'group-1'),
      activePaneId: 'pane-1',
      focusedRegion: 'center',
    });
  });

  it('reorders first tab to last position', () => {
    windowActions.reorderEdgeTab('left', 'left-1', 2);

    expect(windowStore.edgePanels.left.tabs).toHaveLength(3);
    expect(windowStore.edgePanels.left.tabs[0]!.id).toBe('left-2');
    expect(windowStore.edgePanels.left.tabs[1]!.id).toBe('left-3');
    expect(windowStore.edgePanels.left.tabs[2]!.id).toBe('left-1');
  });

  it('reorders last tab to first position', () => {
    windowActions.reorderEdgeTab('left', 'left-3', 0);

    expect(windowStore.edgePanels.left.tabs).toHaveLength(3);
    expect(windowStore.edgePanels.left.tabs[0]!.id).toBe('left-3');
    expect(windowStore.edgePanels.left.tabs[1]!.id).toBe('left-1');
    expect(windowStore.edgePanels.left.tabs[2]!.id).toBe('left-2');
  });

  it('reorders middle tab to end', () => {
    windowActions.reorderEdgeTab('left', 'left-2', 2);

    expect(windowStore.edgePanels.left.tabs).toHaveLength(3);
    expect(windowStore.edgePanels.left.tabs[0]!.id).toBe('left-1');
    expect(windowStore.edgePanels.left.tabs[1]!.id).toBe('left-3');
    expect(windowStore.edgePanels.left.tabs[2]!.id).toBe('left-2');
  });

  it('is a no-op when moving to same position', () => {
    const tabsBefore = [...windowStore.edgePanels.left.tabs];

    windowActions.reorderEdgeTab('left', 'left-1', 0);

    expect(windowStore.edgePanels.left.tabs).toHaveLength(3);
    expect(windowStore.edgePanels.left.tabs[0]!.id).toBe(tabsBefore[0]!.id);
    expect(windowStore.edgePanels.left.tabs[1]!.id).toBe(tabsBefore[1]!.id);
    expect(windowStore.edgePanels.left.tabs[2]!.id).toBe(tabsBefore[2]!.id);
  });

  it('is a no-op when tab not found', () => {
    const tabsBefore = [...windowStore.edgePanels.left.tabs];

    windowActions.reorderEdgeTab('left', 'nonexistent', 1);

    expect(windowStore.edgePanels.left.tabs).toHaveLength(3);
    expect(windowStore.edgePanels.left.tabs[0]!.id).toBe(tabsBefore[0]!.id);
    expect(windowStore.edgePanels.left.tabs[1]!.id).toBe(tabsBefore[1]!.id);
    expect(windowStore.edgePanels.left.tabs[2]!.id).toBe(tabsBefore[2]!.id);
  });

  it('is a no-op when panel not found', () => {
    const tabsBefore = [...windowStore.edgePanels.left.tabs];

    windowActions.reorderEdgeTab('bottom', 'left-1', 0);

    expect(windowStore.edgePanels.left.tabs).toHaveLength(3);
    expect(windowStore.edgePanels.left.tabs[0]!.id).toBe(tabsBefore[0]!.id);
  });

  it('handles off-by-one: moving from earlier to later position', () => {
    // Moving left-1 (index 0) to index 2 should result in:
    // [left-2, left-3, left-1]
    windowActions.reorderEdgeTab('left', 'left-1', 2);

    expect(windowStore.edgePanels.left.tabs[0]!.id).toBe('left-2');
    expect(windowStore.edgePanels.left.tabs[1]!.id).toBe('left-3');
    expect(windowStore.edgePanels.left.tabs[2]!.id).toBe('left-1');
  });

  it('handles off-by-one: moving from later to earlier position', () => {
    // Moving left-3 (index 2) to index 0 should result in:
    // [left-3, left-1, left-2]
    windowActions.reorderEdgeTab('left', 'left-3', 0);

    expect(windowStore.edgePanels.left.tabs[0]!.id).toBe('left-3');
    expect(windowStore.edgePanels.left.tabs[1]!.id).toBe('left-1');
    expect(windowStore.edgePanels.left.tabs[2]!.id).toBe('left-2');
  });

  it('preserves tab properties during reorder', () => {
    const tabBefore = windowStore.edgePanels.left.tabs[0]!;

    windowActions.reorderEdgeTab('left', 'left-1', 2);

    const tabAfter = windowStore.edgePanels.left.tabs[2]!;
    expect(tabAfter.id).toBe(tabBefore.id);
    expect(tabAfter.title).toBe(tabBefore.title);
    expect(tabAfter.contentType).toBe(tabBefore.contentType);
    expect(tabAfter.panelPosition).toBe(tabBefore.panelPosition);
  });

  it('works with single-tab panel (no-op)', () => {
    windowActions.reorderEdgeTab('right', 'right-1', 0);

    expect(windowStore.edgePanels.right.tabs).toHaveLength(1);
    expect(windowStore.edgePanels.right.tabs[0]!.id).toBe('right-1');
  });

  it('reorders in different edge panels independently', () => {
    windowActions.reorderEdgeTab('left', 'left-1', 2);
    windowActions.reorderEdgeTab('right', 'right-1', 0);

    expect(windowStore.edgePanels.left.tabs[2]!.id).toBe('left-1');
    expect(windowStore.edgePanels.right.tabs[0]!.id).toBe('right-1');
  });
});
