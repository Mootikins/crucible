import { describe, it, expect } from 'vitest';
import { collectLeafGroupIds, primaryEdgeGroupId } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';

describe('defaultLayout default seed', () => {
  // Regression / drift guard: every seeded tab group's activeTabId must be an
  // actual tab in that group. The right panel used to seed 'outline-tab', a tab
  // removed in the clean-slate roster refactor, so it opened to "Select a tab".
  it('every tab group opens to one of its own tabs', () => {
    const state = defaultLayout();
    for (const [groupId, group] of Object.entries(state.tabGroups)) {
      if (group.activeTabId === null) continue;
      const ids = group.tabs.map((t) => t.id);
      expect(ids, `group ${groupId} activeTabId must be one of its tabs`).toContain(
        group.activeTabId
      );
    }
  });

  // One document group, two navigation panes, and conversations above Terminal.
  it('seeds 5 tab groups: documents, navigation, conversations and terminal', () => {
    const state = defaultLayout();
    expect(Object.keys(state.tabGroups)).toHaveLength(5);
    expect(collectLeafGroupIds(state.layout)).toHaveLength(1);
    const railGroups = (['left', 'right'] as const).flatMap((pos) =>
      collectLeafGroupIds(state.edgePanels[pos].layout),
    );
    expect(railGroups).toHaveLength(4);
  });

  it('the right edge panel reserves a conversation pane', () => {
    const state = defaultLayout();
    const rightGroupId = primaryEdgeGroupId(state, 'right')!;
    expect(state.tabGroups[rightGroupId].activeTabId).toBeNull();
  });

  // The navigation panes remain simultaneously visible.
  it('stacks Sessions and Files in separate panes on the left', () => {
    const state = defaultLayout();
    const left = state.tabGroups[primaryEdgeGroupId(state, 'left')!];
    const files = state.tabGroups[collectLeafGroupIds(state.edgePanels.left.layout)[1]];
    expect(left.tabs.map((t) => t.contentType)).toEqual(['sessions']);
    expect(left.activeTabId).toBe(left.tabs[0].id);
    expect(files.tabs.map((t) => t.contentType)).toContain('files');
    expect(left.tabs.map((t) => t.contentType)).not.toContain('files');
  });

  // Search searches files, notes AND sessions. Seeding it into a rail would
  // claim a scope it does not have; Ctrl+Shift+F opens it on demand.
  it('seeds Search into no rail at all', () => {
    const state = defaultLayout();
    for (const group of Object.values(state.tabGroups)) {
      expect(group.tabs.map((t) => t.contentType)).not.toContain('search');
    }
  });

  // The bug this catches: the Navigator refactor unregistered the 'files' and
  // 'sessions' panels but left them in the DEFAULT layout, so a fresh profile
  // (no saved layout) opened a left panel of dead tabs reading "Unknown
  // content type". Persisted layouts were migrated; the seed was not.
  it('every seeded tab renders a REGISTERED panel', () => {
    resetGlobalRegistry();
    registerPanels();
    const registry = getGlobalRegistry();
    const state = defaultLayout();
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
