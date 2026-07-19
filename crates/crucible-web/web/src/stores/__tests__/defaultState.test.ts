import { describe, it, expect } from 'vitest';
import { createInitialState } from '@/stores/windowStoreInternals';

describe('createInitialState default seed', () => {
  // Regression / drift guard: every seeded tab group's activeTabId must be an
  // actual tab in that group. The right panel used to seed 'outline-tab', a tab
  // removed in the clean-slate roster refactor, so it opened to "Select a tab".
  it('every tab group opens to one of its own tabs', () => {
    const state = createInitialState();
    for (const [groupId, group] of Object.entries(state.tabGroups)) {
      if (group.activeTabId === null) continue;
      const ids = group.tabs.map((t) => t.id);
      expect(ids, `group ${groupId} activeTabId must be one of its tabs`).toContain(
        group.activeTabId
      );
    }
  });

  it('the right edge panel opens to Backlinks', () => {
    const state = createInitialState();
    const rightGroupId = state.edgePanels.right.tabGroupId;
    expect(state.tabGroups[rightGroupId].activeTabId).toBe('backlinks-tab');
  });
});
