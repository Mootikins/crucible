import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import {
  collectPanes,
  findPaneInLayout,
} from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';
import { serializeLayout, deserializeLayout } from '@/windowing/model/serializer';
import { appLayoutHooks } from '@/stores/layoutMigrations';

const resetStore = () => {
  const fresh = defaultLayout();
  setStore(
    produce((s) => {
      s.layout = fresh.layout;
      s.tabGroups = fresh.tabGroups;
      s.edgePanels = fresh.edgePanels;
      s.floatingWindows = [];
      s.activePaneId = fresh.activePaneId;
      s.focusedRegion = 'center';
      s.nextZIndex = 100;
    }),
  );
};

const rightPane = (paneId: string) =>
  findPaneInLayout(windowStore.edgePanels.right.layout, paneId);

const rightSplitRatio = () => {
  const root = windowStore.edgePanels.right.layout;
  return root.type === 'split' ? root.splitRatio : null;
};

beforeEach(resetStore);

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

describe('setPaneCollapsed', () => {
  it('expands and re-collapses one pane, leaving its sibling alone', () => {
    windowActions.setPaneCollapsed('right-term-pane', false);
    expect(rightPane('right-term-pane')?.collapsed).toBe(false);
    expect(rightPane('right-pane')?.collapsed).not.toBe(true);

    windowActions.setPaneCollapsed('right-term-pane', true);
    expect(rightPane('right-term-pane')?.collapsed).toBe(true);
    expect(rightPane('right-pane')?.collapsed).not.toBe(true);
  });

  it('toggles a pane both ways', () => {
    windowActions.togglePaneCollapsed('right-term-pane');
    expect(rightPane('right-term-pane')?.collapsed).toBe(false);
    windowActions.togglePaneCollapsed('right-term-pane');
    expect(rightPane('right-term-pane')?.collapsed).toBe(true);
  });

  // A rail of nothing but bars reads as a broken rail. Hiding everything is
  // what the rail's OWN collapse is for.
  it('refuses to collapse the last expanded pane of a rail', () => {
    windowActions.setPaneCollapsed('right-pane', true);
    expect(rightPane('right-pane')?.collapsed).not.toBe(true);
  });

  it('refuses to collapse the only pane of a single-pane rail', () => {
    const leftPaneId = collectPanes(windowStore.edgePanels.left.layout)[0].id;
    windowActions.setPaneCollapsed(leftPaneId, true);
    expect(
      findPaneInLayout(windowStore.edgePanels.left.layout, leftPaneId)?.collapsed,
    ).not.toBe(true);
  });

  // The guard is per ROOT, not per region: the only pane of the centre tiling
  // is also the last expanded pane of its root, so it is refused for the same
  // reason a rail's last pane is.
  it('refuses the last expanded pane of the centre tiling', () => {
    const centrePaneId = windowStore.layout.id;
    windowActions.setPaneCollapsed(centrePaneId, true);
    expect(collectPanes(windowStore.layout)[0].collapsed).not.toBe(true);
  });

  // ...and it is ONLY that rule. A centre with two panes collapses one, which
  // the old rail-scoped signature could not express at all: it demanded an
  // EdgePanelPosition, so no centre pane could ever be named.
  it('collapses a centre pane once the centre has a sibling', () => {
    const firstId = windowStore.layout.id;
    const groupId = Object.keys(windowStore.tabGroups)[0];
    windowActions.openTabInNewPane(firstId, 'right', {
      id: 'tab-collapse-probe',
      title: 'Probe',
      contentType: 'settings',
    });
    void groupId;
    const panes = collectPanes(windowStore.layout);
    expect(panes.length).toBe(2);
    windowActions.setPaneCollapsed(panes[1].id, true);
    expect(collectPanes(windowStore.layout)[1].collapsed).toBe(true);
    expect(collectPanes(windowStore.layout)[0].collapsed).not.toBe(true);
  });

  // `splitRatio` is the size the pane opens back to; a collapse must not
  // spend it.
  it('leaves the split ratio untouched across a collapse cycle', () => {
    windowActions.commitSplitRatio('right-split', 0.4);
    expect(rightSplitRatio()).toBe(0.4);

    windowActions.setPaneCollapsed('right-term-pane', false);
    windowActions.setPaneCollapsed('right-term-pane', true);
    expect(rightSplitRatio()).toBe(0.4);
  });
});

describe('rail pane collapse survives layout transforms', () => {
  // `mirrorLayout` returns leaves untouched, which is exactly why the flag
  // lives on the pane node: a flip carries it with no mirror rule of its own.
  it('survives a left-right flip', () => {
    windowActions.swapSidePanels();
    const flipped = collectPanes(windowStore.edgePanels.left.layout);
    expect(flipped.map((p) => p.id)).toEqual(['right-pane', 'right-term-pane']);
    expect(flipped[1].collapsed).toBe(true);

    windowActions.swapSidePanels();
    expect(
      collectPanes(windowStore.edgePanels.right.layout)[1].collapsed,
    ).toBe(true);
  });

  it('survives a serialize / deserialize round trip', () => {
    windowActions.setPaneCollapsed('right-term-pane', false);
    windowActions.setPaneCollapsed('right-pane', true);

    const restored = deserializeLayout(serializeLayout(windowStore), appLayoutHooks);
    const panes = collectPanes(restored.edgePanels.right.layout);
    expect(panes[0].collapsed).toBe(true);
    expect(panes[1].collapsed).toBe(false);
  });
});
