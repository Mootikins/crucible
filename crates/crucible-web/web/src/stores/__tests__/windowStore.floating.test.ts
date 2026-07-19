import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, windowActions, setStore } from '../windowStore';
import { createInitialState, findFirstPane } from '../windowStoreInternals';
import type { Tab } from '@/types/windowTypes';

const tab = (id: string, overrides: Partial<Tab> = {}): Tab => ({
  id,
  title: id,
  contentType: 'file',
  ...overrides,
});

/** Reset to a fresh initial state and return the main pane + its group id. */
function resetStore(): { paneId: string; groupId: string } {
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
  return { paneId: pane.id, groupId: pane.tabGroupId! };
}

describe('popOutPane — pop a pane\'s tabs into a floating window', () => {
  let paneId: string;
  let groupId: string;

  beforeEach(() => {
    ({ paneId, groupId } = resetStore());
    windowActions.addTab(groupId, tab('tab-a'));
    windowActions.addTab(groupId, tab('tab-b'));
  });

  it('moves the group into a new floating window and detaches it from the pane', () => {
    const windowId = windowActions.popOutPane(paneId);

    expect(windowId).not.toBeNull();
    const win = windowStore.floatingWindows.find((w) => w.id === windowId)!;
    expect(win.tabGroupId).toBe(groupId);
    // The pane no longer references the popped-out group — the same group must
    // never be rendered by two tab bars (duplicate solid-dnd ids).
    const pane = windowActions.findPaneById(paneId);
    expect(pane?.tabGroupId).not.toBe(groupId);
    // The group itself survives with its tabs.
    expect(windowStore.tabGroups[groupId]?.tabs.map((t) => t.id)).toEqual(['tab-a', 'tab-b']);
  });

  it('titles the window after the active tab', () => {
    windowActions.setActiveTab(groupId, 'tab-b');
    const windowId = windowActions.popOutPane(paneId);
    const win = windowStore.floatingWindows.find((w) => w.id === windowId)!;
    expect(win.title).toBe('tab-b');
  });

  it('is a no-op for a pane with no tabs', () => {
    const { paneId: emptyPane } = resetStore();
    const windowId = windowActions.popOutPane(emptyPane);
    expect(windowId).toBeNull();
    expect(windowStore.floatingWindows).toHaveLength(0);
  });

  it('collapses the emptied pane when it is part of a split', () => {
    // Split so there are two panes; pop out the one that has the tabs.
    windowActions.splitPane(paneId, 'horizontal');
    const layout = windowStore.layout;
    expect(layout.type).toBe('split');
    const first = (layout as Extract<typeof layout, { type: 'split' }>).first;
    expect(first.type).toBe('pane');
    const firstPane = first as Extract<typeof first, { type: 'pane' }>;

    windowActions.popOutPane(firstPane.id);

    // The emptied pane collapses out of the split; the layout is a lone pane.
    expect(windowStore.layout.type).toBe('pane');
    expect(windowStore.floatingWindows).toHaveLength(1);
  });
});

describe('closeFloatingWindow — closing a window closes its tabs', () => {
  let paneId: string;
  let groupId: string;

  beforeEach(() => {
    ({ paneId, groupId } = resetStore());
    windowActions.addTab(groupId, tab('tab-a'));
  });

  it('removes the window AND its tab group (no orphaned tabs)', () => {
    const windowId = windowActions.popOutPane(paneId)!;

    windowActions.closeFloatingWindow(windowId);

    expect(windowStore.floatingWindows).toHaveLength(0);
    expect(windowStore.tabGroups[groupId]).toBeUndefined();
  });

  it('is a no-op for an unknown window id', () => {
    const before = Object.keys(windowStore.tabGroups).length;
    windowActions.closeFloatingWindow('nope');
    expect(Object.keys(windowStore.tabGroups)).toHaveLength(before);
  });
});
