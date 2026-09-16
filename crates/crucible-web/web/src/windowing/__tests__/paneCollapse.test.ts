import { describe, it, expect, beforeEach } from 'vitest';
import { configureWindowing, windowStore, windowActions } from '@/windowing/store';
import { collectPanes, findPaneInLayout } from '@/windowing/model/tree';
import { serializeLayout, deserializeLayout } from '@/windowing/model/serializer';
import { neutralPolicy } from '@/windowing/testing/neutralPolicy';
import { stackRightRail } from '@/windowing/testing/stackedRail';

/**
 * The neutral seed with a column in the right rail: `right-pane` above the
 * collapsed `right-term-pane`. The app seed has the same shape, and
 * src/stores/__tests__/paneCollapse.seed.test.ts pins that seed.
 */
const resetStore = () => {
  configureWindowing(neutralPolicy());
  stackRightRail();
};

const rightPane = (paneId: string) =>
  findPaneInLayout(windowStore.edgePanels.right.layout, paneId);

const rightSplitRatio = () => {
  const root = windowStore.edgePanels.right.layout;
  return root.type === 'split' ? root.splitRatio : null;
};

beforeEach(resetStore);

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
      contentType: 'alpha',
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

    const p = neutralPolicy();
    const restored = deserializeLayout(serializeLayout(windowStore), p.layoutHooks, (t) => p.iconFor(t));
    const panes = collectPanes(restored.edgePanels.right.layout);
    expect(panes[0].collapsed).toBe(true);
    expect(panes[1].collapsed).toBe(false);
  });
});
