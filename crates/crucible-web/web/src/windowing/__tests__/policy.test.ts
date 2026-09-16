import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  configureWindowing,
  windowStore,
  windowActions,
  resetWindowingForTest,
} from '@/windowing/store';
import { stubPolicy } from './stubPolicy';

/** The centre group of the stub seed. */
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

  it('seeds the store from the policy', () => {
    configureWindowing(stubPolicy());
    expect(windowStore.tabGroups[centreGroupId()]!.tabs.map((t) => t.id)).toEqual(['t1']);
  });

  it('asks the policy before it closes a tab', () => {
    const mayCloseTab = vi.fn(() => false);
    configureWindowing(stubPolicy({ mayCloseTab }));
    const g = centreGroupId();
    windowActions.removeTab(g, 't1');
    expect(mayCloseTab).toHaveBeenCalledWith(expect.anything(), g, 't1');
    expect(windowStore.tabGroups[g]!.tabs).toHaveLength(1);
  });

  it('answers canCloseTab from the policy', () => {
    configureWindowing(stubPolicy({ mayCloseTab: () => false }));
    expect(windowActions.canCloseTab(centreGroupId(), 't1')).toBe(false);
  });

  it('lets the policy repair a restored layout', () => {
    const repairLayout = vi.fn();
    configureWindowing(stubPolicy({ repairLayout }));
    windowActions.importLayout(windowActions.exportLayout());
    expect(repairLayout).toHaveBeenCalledTimes(1);
  });

  it('reads a stored layout with the policy hooks', () => {
    const base = stubPolicy();
    const prune = vi.fn();
    configureWindowing(stubPolicy({ layoutHooks: { ...base.layoutHooks, prune } }));
    windowActions.importLayout(windowActions.exportLayout());
    expect(prune).toHaveBeenCalledTimes(1);
  });

  it('resets from the policy seed, then repairs', () => {
    const seed = vi.fn(stubPolicy().seed);
    const repairLayout = vi.fn();
    configureWindowing(stubPolicy({ seed, repairLayout }));
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
    configureWindowing(stubPolicy({ onActiveTabChange }));
    windowActions.setActiveTab(centreGroupId(), 't1');
    expect(onActiveTabChange).toHaveBeenCalledWith(expect.objectContaining({ id: 't1' }));
  });
});
