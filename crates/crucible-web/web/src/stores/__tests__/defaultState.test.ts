import { describe, it, expect } from 'vitest';
import { createInitialState, primaryEdgeGroupId } from '@/stores/windowStoreInternals';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';

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
    const rightGroupId = primaryEdgeGroupId(state, 'right')!;
    expect(state.tabGroups[rightGroupId].activeTabId).toBe('backlinks-tab');
  });

  it('the left edge panel opens to the Navigator', () => {
    const state = createInitialState();
    const leftGroupId = primaryEdgeGroupId(state, 'left')!;
    const group = state.tabGroups[leftGroupId];
    expect(group.tabs.map((t) => t.contentType)).toEqual(['navigator']);
    expect(group.activeTabId).toBe(group.tabs[0].id);
  });

  // The bug this catches: the Navigator refactor unregistered the 'files' and
  // 'sessions' panels but left them in the DEFAULT layout, so a fresh profile
  // (no saved layout) opened a left panel of dead tabs reading "Unknown
  // content type". Persisted layouts were migrated; the seed was not.
  it('every seeded tab renders a REGISTERED panel', () => {
    resetGlobalRegistry();
    registerPanels();
    const registry = getGlobalRegistry();
    const state = createInitialState();
    for (const group of Object.values(state.tabGroups)) {
      for (const tab of group.tabs) {
        expect(
          registry.get(tab.contentType),
          `seeded tab "${tab.title}" has unregistered contentType "${tab.contentType}"`,
        ).toBeDefined();
      }
    }
  });
});
