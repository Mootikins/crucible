import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { placeNewTab, resolveNewTabTarget } from '../tab-placement';
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

  it('newFloating spawns the window at the drop point, clamped to the viewport', () => {
    placeNewTab({ type: 'newFloating', at: { x: 300, y: 200 } }, fileTab('/k/at.md'));
    const win = windowStore.floatingWindows[0];
    expect(win.x).toBe(260); // x - 40 (handle under the pointer)
    expect(win.y).toBe(184); // y - 16
    // A drop at the far corner clamps inside the viewport.
    placeNewTab({ type: 'newFloating', at: { x: 99999, y: 99999 } }, fileTab('/k/corner.md'));
    const win2 = windowStore.floatingWindows[1];
    expect(win2.x + 520).toBeLessThanOrEqual((globalThis.innerWidth || 1280) - 8 + 520);
    expect(win2.x).toBeGreaterThanOrEqual(8);
    expect(win2.y).toBeGreaterThanOrEqual(8);
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

describe('resolveNewTabTarget (hover-editor drop policy)', () => {
  const point = { x: 10, y: 20 };

  it('no target → tear off into a floating window at the drop point', () => {
    expect(resolveNewTabTarget(undefined, point)).toEqual({
      type: 'newFloating',
      at: point,
    });
  });

  it('a pane BODY release also floats — docking into a pane is via its tab bar', () => {
    expect(resolveNewTabTarget({ type: 'pane', paneId: 'p1', position: 'center' }, point)).toEqual({
      type: 'newFloating',
      at: point,
    });
    expect(resolveNewTabTarget({ type: 'pane', paneId: 'p1' }, point)).toEqual({
      type: 'newFloating',
      at: point,
    });
  });

  it('explicit dock targets pass through unchanged', () => {
    const split = { type: 'pane', paneId: 'p1', position: 'right' } as const;
    expect(resolveNewTabTarget(split, point)).toBe(split);
    const bar = { type: 'tabGroup', groupId: 'g1' } as const;
    expect(resolveNewTabTarget(bar, point)).toBe(bar);
    const edge = { type: 'edgePanel', panelId: 'left' } as const;
    expect(resolveNewTabTarget(edge, point)).toBe(edge);
  });
});
