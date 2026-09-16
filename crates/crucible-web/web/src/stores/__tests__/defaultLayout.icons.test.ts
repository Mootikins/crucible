import { describe, it, expect } from 'vitest';
import { defaultLayout } from '@/stores/defaultLayout';
import { collectLeafGroupIds } from '@/windowing/model/tree';
import type { EdgePanelPosition } from '@/types/windowTypes';

describe('the default rail roster', () => {
  it('gives every rail tab a component icon', () => {
    const state = defaultLayout();
    // EVERY leaf of each rail, not just the first: the right rail is a column.
    const tabs = (['left', 'right'] as EdgePanelPosition[]).flatMap((p) =>
      collectLeafGroupIds(state.edgePanels[p].layout).flatMap((id) => state.tabGroups[id]?.tabs ?? []),
    );
    // Identity on the left, working context on the right.
    expect(tabs.map((t) => t.title)).toEqual([
      'Sessions',
      'Files',
      'Backlinks',
      'Activity',
      // The terminal is a pane UNDER the tree now, in the same rail — not a
      // full-width dock. `Chat` went with that dock.
      'Terminal',
    ]);
    for (const tab of tabs) {
      expect(typeof tab.icon, `${tab.title} should carry a component icon`).toBe('function');
    }
  });
});
