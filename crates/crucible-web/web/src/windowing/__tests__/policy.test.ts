import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  configureWindowing,
  windowStore,
  windowActions,
  resetWindowingForTest,
} from '@/windowing/store';
import { neutralPolicy } from '@/windowing/testing/neutralPolicy';
import { primaryEdgeGroupId } from '@/windowing/model/tree';

/** The centre group of the neutral seed. */
function centreGroupId(): string {
  const layout = windowStore.layout;
  if (layout.type !== 'pane' || !layout.tabGroupId) throw new Error('seed has no centre group');
  return layout.tabGroupId;
}

describe('WindowPolicy', () => {
  beforeEach(() => resetWindowingForTest());

  it('refuses to act before configuration', () => {
    expect(() => windowActions.splitPane('x', 'horizontal')).toThrow(/configureWindowing/);
  });

  it('lets a spy replace an action', () => {
    configureWindowing(neutralPolicy());
    // A call before the spy, as the app makes before a test spies.
    windowActions.toggleEdgePanel('left');
    const spy = vi.spyOn(windowActions, 'toggleEdgePanel').mockImplementation(() => {});
    try {
      const before = windowStore.edgePanels.left.mode;
      windowActions.toggleEdgePanel('left');
      expect(spy).toHaveBeenCalledWith('left');
      expect(windowStore.edgePanels.left.mode).toBe(before);
    } finally {
      spy.mockRestore();
    }
  });

  it('seeds the store from the policy', () => {
    configureWindowing(neutralPolicy());
    expect(windowStore.tabGroups[centreGroupId()]!.tabs.map((t) => t.id)).toEqual([
      'tab-alpha',
      'tab-beta',
    ]);
  });

  it('gives each rail of the neutral seed one tab, the left docked and the right a strip', () => {
    configureWindowing(neutralPolicy());
    const tabsOf = (side: 'left' | 'right') =>
      windowStore.tabGroups[primaryEdgeGroupId(windowStore, side)!]!.tabs.map((t) => t.id);
    expect(tabsOf('left')).toEqual(['tab-left']);
    expect(tabsOf('right')).toEqual(['tab-right']);
    expect(windowStore.edgePanels.left.mode).toBe('docked');
    expect(windowStore.edgePanels.right.mode).toBe('strip');
    expect(windowStore.tabGroups[centreGroupId()]!.activeTabId).toBe('tab-alpha');
  });

  it('asks the policy before it closes a tab', () => {
    const mayCloseTab = vi.fn(() => false);
    configureWindowing(neutralPolicy({ mayCloseTab }));
    const g = centreGroupId();
    windowActions.removeTab(g, 'tab-alpha');
    expect(mayCloseTab).toHaveBeenCalledWith(expect.anything(), g, 'tab-alpha');
    expect(windowStore.tabGroups[g]!.tabs).toHaveLength(2);
  });

  it('answers canCloseTab from the policy', () => {
    configureWindowing(neutralPolicy({ mayCloseTab: () => false }));
    expect(windowActions.canCloseTab(centreGroupId(), 'tab-alpha')).toBe(false);
  });

  it('lets the policy repair a restored layout', () => {
    const repairLayout = vi.fn();
    configureWindowing(neutralPolicy({ repairLayout }));
    windowActions.importLayout(windowActions.exportLayout());
    expect(repairLayout).toHaveBeenCalledTimes(1);
  });

  it('reads a stored layout with the policy hooks', () => {
    const base = neutralPolicy();
    const prune = vi.fn();
    configureWindowing(neutralPolicy({ layoutHooks: { ...base.layoutHooks, prune } }));
    windowActions.importLayout(windowActions.exportLayout());
    expect(prune).toHaveBeenCalledTimes(1);
  });

  it('resets from the policy seed, then repairs', () => {
    const seed = vi.fn(neutralPolicy().seed);
    const repairLayout = vi.fn();
    configureWindowing(neutralPolicy({ seed, repairLayout }));
    seed.mockClear();
    windowActions.resetLayoutToDefaults();
    expect(seed).toHaveBeenCalledTimes(1);
    expect(repairLayout).toHaveBeenCalledTimes(1);
    expect(seed.mock.invocationCallOrder[0]!).toBeLessThan(
      repairLayout.mock.invocationCallOrder[0]!,
    );
  });

  it('reports the active tab to the policy', () => {
    const onActiveTabChange = vi.fn();
    configureWindowing(neutralPolicy({ onActiveTabChange }));
    windowActions.setActiveTab(centreGroupId(), 'tab-alpha');
    expect(onActiveTabChange).toHaveBeenCalledWith(expect.objectContaining({ id: 'tab-alpha' }));
  });
});
