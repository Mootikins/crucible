import { describe, it, expect } from 'vitest';
import { collectPanes } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';

// These cases read the app seed, so they stay with the app. The collapse
// actions and their transforms run on the neutral policy in
// src/windowing/__tests__/paneCollapse.test.ts.
describe('rail pane collapse — default seed', () => {
  // The shipped default: a fresh rail shows the tree at full height with a
  // terminal BAR under it.
  it('seeds the terminal pane collapsed and the tree pane expanded', () => {
    const state = defaultLayout();
    const panes = collectPanes(state.edgePanels.right.layout);
    expect(panes.map((p) => p.id)).toEqual(['right-pane', 'right-term-pane']);
    expect(panes[0].collapsed).not.toBe(true);
    expect(panes[1].collapsed).toBe(true);
  });

  // The rail's own collapse and a pane's collapse are separate controls: the
  // seed opening the rail must not open the terminal with it.
  it('collapsing state of the rail is independent of the pane', () => {
    const state = defaultLayout();
    state.edgePanels.right.mode = 'docked';
    expect(collectPanes(state.edgePanels.right.layout)[1].collapsed).toBe(true);
  });
});
