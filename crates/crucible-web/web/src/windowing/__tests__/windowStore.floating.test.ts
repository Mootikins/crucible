import { describe, it, expect, beforeEach } from 'vitest';
import { configureWindowing, windowStore, windowActions } from '@/windowing/store';
import { collectLeafGroupIds, emptyState, findFirstPane } from '@/windowing/model/tree';
import { neutralPolicy } from '@/windowing/testing/neutralPolicy';
import type { Tab } from '@/windowing/model/types';

const tab = (id: string, overrides: Partial<Tab> = {}): Tab => ({
  id,
  title: id,
  contentType: 'file',
  ...overrides,
});

/** Reset to an empty seed and return the main pane + its group id. */
function resetStore(): { paneId: string; groupId: string } {
  configureWindowing(neutralPolicy({ seed: emptyState }));
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

describe('popOutPane with a tab id — pop one tab into a floating window', () => {
  let paneId: string;
  let groupId: string;

  beforeEach(() => {
    ({ paneId, groupId } = resetStore());
    windowActions.addTab(groupId, tab('tab-a'));
    windowActions.addTab(groupId, tab('tab-b'));
    windowActions.addTab(groupId, tab('tab-c'));
  });

  it('moves that tab only, and the pane keeps the others', () => {
    windowActions.setActiveTab(groupId, 'tab-b');
    const windowId = windowActions.popOutPane(paneId, 'tab-b');

    const win = windowStore.floatingWindows.find((w) => w.id === windowId)!;
    expect(win.title).toBe('tab-b');
    expect(win.tabGroupId).not.toBe(groupId);
    expect(windowStore.tabGroups[win.tabGroupId]?.tabs.map((t) => t.id)).toEqual(['tab-b']);
    expect(windowActions.findPaneById(paneId)?.tabGroupId).toBe(groupId);
    expect(windowStore.tabGroups[groupId]?.tabs.map((t) => t.id)).toEqual(['tab-a', 'tab-c']);
    // The neighbour that took the place of the tab is active.
    expect(windowStore.tabGroups[groupId]?.activeTabId).toBe('tab-c');
  });

  it('moves the whole group when the tab is the only tab', () => {
    const { paneId: solo, groupId: soloGroup } = resetStore();
    windowActions.addTab(soloGroup, tab('tab-solo'));
    const windowId = windowActions.popOutPane(solo, 'tab-solo');
    const win = windowStore.floatingWindows.find((w) => w.id === windowId)!;
    expect(win.tabGroupId).toBe(soloGroup);
    expect(windowActions.findPaneById(solo)?.tabGroupId ?? null).not.toBe(soloGroup);
  });

  it('refuses a tab that the policy keeps', () => {
    configureWindowing(neutralPolicy({ seed: emptyState, mayCloseTab: () => false }));
    const pane = findFirstPane(windowStore.layout)!;
    windowActions.addTab(pane.tabGroupId!, tab('kept'));
    windowActions.addTab(pane.tabGroupId!, tab('other'));
    expect(windowActions.canPopOutTab(pane.tabGroupId!, 'kept')).toBe(false);
    expect(windowActions.popOutPane(pane.id, 'kept')).toBeNull();
    expect(windowStore.floatingWindows).toHaveLength(0);
  });

  it('refuses a tab that the policy calls unavailable', () => {
    configureWindowing(
      neutralPolicy({ seed: emptyState, unavailableReason: (t) => (t.id === 'gone' ? 'no' : null) }),
    );
    const pane = findFirstPane(windowStore.layout)!;
    windowActions.addTab(pane.tabGroupId!, tab('gone'));
    windowActions.addTab(pane.tabGroupId!, tab('here'));
    expect(windowActions.canPopOutTab(pane.tabGroupId!, 'gone')).toBe(false);
    expect(windowActions.canPopOutTab(pane.tabGroupId!, 'here')).toBe(true);
    expect(windowActions.popOutPane(pane.id, 'gone')).toBeNull();
  });
});

describe('dockFloatingWindow with a tab id — dock one tab', () => {
  let paneId: string;
  let groupId: string;
  let windowId: string;
  let floatGroup: string;

  beforeEach(() => {
    ({ paneId, groupId } = resetStore());
    windowActions.addTab(groupId, tab('tab-main'));
    floatGroup = windowActions.createTabGroup();
    windowActions.addTab(floatGroup, tab('tab-x'));
    windowActions.addTab(floatGroup, tab('tab-y'));
    windowId = windowActions.createFloatingWindow(floatGroup, 10, 10);
  });

  /** The tabs that the centre tiling shows. */
  const centreTabIds = () =>
    collectLeafGroupIds(windowStore.layout).flatMap(
      (id) => windowStore.tabGroups[id]?.tabs.map((t) => t.id) ?? [],
    );

  it('moves that tab into a docked pane, and the window keeps the others', () => {
    windowActions.dockFloatingWindow(windowId, 'tab-x');
    expect(windowStore.floatingWindows.map((w) => w.id)).toEqual([windowId]);
    expect(windowStore.tabGroups[floatGroup]?.tabs.map((t) => t.id)).toEqual(['tab-y']);
    expect(centreTabIds()).toContain('tab-x');
    expect(windowStore.layout.type).toBe('split');
    expect(windowActions.findPaneById(paneId)?.tabGroupId).toBe(groupId);
  });

  it('moves the whole window when the tab is its last tab', () => {
    windowActions.dockFloatingWindow(windowId, 'tab-x');
    windowActions.dockFloatingWindow(windowId, 'tab-y');
    expect(windowStore.floatingWindows).toHaveLength(0);
    expect(centreTabIds()).toEqual(expect.arrayContaining(['tab-main', 'tab-x', 'tab-y']));
  });

  // A split root is no pane. The old lookup found nothing there, and the dock
  // did nothing: the window stayed and the button seemed dead.
  it('docks into a centre whose root is a split', () => {
    windowActions.splitPane(paneId, 'horizontal');
    for (const id of collectLeafGroupIds(windowStore.layout)) {
      if (!windowStore.tabGroups[id]?.tabs.length) windowActions.addTab(id, tab(`fill-${id}`));
    }
    windowActions.dockFloatingWindow(windowId);
    expect(windowStore.floatingWindows).toHaveLength(0);
    expect(centreTabIds()).toEqual(expect.arrayContaining(['tab-x', 'tab-y']));
  });
});
