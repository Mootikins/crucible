import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { placeNewTab } from '../tab-placement';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane } from '@/stores/windowStoreInternals';
import type { Tab } from '@/types/windowTypes';

const fileTab = (path: string): Tab => ({
  id: `tab-file-${path}`,
  title: path.split('/').pop()!,
  contentType: 'file',
  metadata: { filePath: path },
});

let paneId: string;
let centerGroupId: string;

beforeEach(() => {
  const fresh = createInitialState();
  setStore(produce((s) => {
    s.layout = fresh.layout;
    s.tabGroups = fresh.tabGroups;
    s.edgePanels = fresh.edgePanels;
    s.floatingWindows = [];
    s.activePaneId = fresh.activePaneId;
    s.focusedRegion = 'center';
    s.flyoutState = null;
    s.nextZIndex = 100;
  }));
  const pane = findFirstPane(windowStore.layout)!;
  paneId = pane.id;
  centerGroupId = pane.tabGroupId!;
});

describe('placeNewTab (groupless DragSource → any drop target)', () => {
  it('drops into a pane center as a new active tab', () => {
    placeNewTab({ type: 'pane', paneId, position: 'center' }, fileTab('/k/a.md'));
    const group = windowStore.tabGroups[centerGroupId];
    expect(group.tabs.some((t) => t.id === 'tab-file-/k/a.md')).toBe(true);
    expect(group.activeTabId).toBe('tab-file-/k/a.md');
  });

  it('drops into a tab group at the given index', () => {
    windowActions.addTab(centerGroupId, fileTab('/k/first.md'));
    placeNewTab({ type: 'tabGroup', groupId: centerGroupId, insertIndex: 0 }, fileTab('/k/b.md'));
    expect(windowStore.tabGroups[centerGroupId].tabs[0].id).toBe('tab-file-/k/b.md');
  });

  it('drops onto an edge panel and expands it when collapsed', () => {
    setStore(produce((s) => { s.edgePanels.right.isCollapsed = true; }));
    const edgeGroup = windowStore.edgePanels.right.tabGroupId;
    placeNewTab({ type: 'edgePanel', panelId: 'right' }, fileTab('/k/c.md'));
    expect(windowStore.tabGroups[edgeGroup].tabs.some((t) => t.id === 'tab-file-/k/c.md')).toBe(true);
    expect(windowStore.edgePanels.right.isCollapsed).toBe(false);
  });

  it('splits the pane for directional drops, exactly like a tab drag', () => {
    windowActions.addTab(centerGroupId, fileTab('/k/existing.md'));
    placeNewTab({ type: 'pane', paneId, position: 'right' }, fileTab('/k/d.md'));
    // The tab landed somewhere, and the layout gained a split.
    const holder = Object.values(windowStore.tabGroups).find((g) =>
      g.tabs.some((t) => t.id === 'tab-file-/k/d.md'),
    );
    expect(holder).toBeTruthy();
    expect(windowStore.layout.type).toBe('split');
  });

  it('creates a floating window for newFloating drops', () => {
    placeNewTab({ type: 'newFloating' }, fileTab('/k/e.md'));
    expect(windowStore.floatingWindows.length).toBe(1);
    const groupId = windowStore.floatingWindows[0].tabGroupId;
    expect(windowStore.tabGroups[groupId].tabs[0].id).toBe('tab-file-/k/e.md');
  });

  it('focuses the existing tab instead of duplicating (same rule as openFileInEditor)', () => {
    windowActions.addTab(centerGroupId, fileTab('/k/dup.md'));
    windowActions.addTab(centerGroupId, fileTab('/k/other.md'));
    placeNewTab({ type: 'edgePanel', panelId: 'left' }, fileTab('/k/dup.md'));
    // Not added to the edge panel; refocused in its original group.
    const edgeGroup = windowStore.edgePanels.left.tabGroupId;
    expect(windowStore.tabGroups[edgeGroup].tabs.some((t) => t.id === 'tab-file-/k/dup.md')).toBe(false);
    expect(windowStore.tabGroups[centerGroupId].activeTabId).toBe('tab-file-/k/dup.md');
  });
});
