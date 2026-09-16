import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { collectPanes } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';
import type { SerializedLayout } from '@/lib/layout-serializer';

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

/**
 * Reduced from a layout taken off a running daemon (`GET /api/layout`), which
 * is where this defect was found. The shape that matters: a split whose second
 * pane names a tab group that is NOT in `tabGroups`.
 *
 * That pane drew EmptyPane on every load and no user action could remove it —
 * opening a session or a file adds a tab to the OTHER pane, and none of those
 * paths collapse anything. Only tab-close and window operations ran
 * `collapseEmptyNodes`, and restore never did.
 */
const DANGLING: SerializedLayout = {
  version: 8,
  layout: {
    id: 'root-split',
    type: 'split',
    direction: 'horizontal',
    splitRatio: 0.5,
    first: { id: 'good-pane', type: 'pane', tabGroupId: 'chat-group' },
    second: { id: 'orphan-pane', type: 'pane', tabGroupId: 'group-that-was-deleted' },
  },
  tabGroups: {
    'chat-group': {
      id: 'chat-group',
      tabs: [{ id: 'chat-1', title: 'Test Session', contentType: 'chat' }],
      activeTabId: 'chat-1',
    },
  },
  edgePanels: defaultLayout().edgePanels,
  floatingWindows: [],
} as unknown as SerializedLayout;

describe('restoring a layout with a dangling tab group', () => {
  beforeEach(resetStore);

  it('drops the pane whose tab group no longer exists', () => {
    windowActions.importLayout(DANGLING);

    const paneIds = collectPanes(windowStore.layout).map((p) => p.id);
    expect(paneIds).not.toContain('orphan-pane');
    expect(paneIds).toContain('good-pane');
    // The split collapsed with it — a split with one child is not a split.
    expect(windowStore.layout.type).toBe('pane');
  });

  it('keeps the surviving pane focusable', () => {
    windowActions.importLayout(DANGLING);
    expect(windowStore.activePaneId).toBe('good-pane');
  });

  it('leaves a layout whose groups all resolve untouched', () => {
    const healthy = {
      ...DANGLING,
      layout: {
        ...DANGLING.layout,
        second: { id: 'second-pane', type: 'pane', tabGroupId: 'files-group' },
      },
      tabGroups: {
        ...DANGLING.tabGroups,
        'files-group': {
          id: 'files-group',
          tabs: [{ id: 'files-1', title: 'Files', contentType: 'files' }],
          activeTabId: 'files-1',
        },
      },
    } as unknown as SerializedLayout;

    windowActions.importLayout(healthy);

    const paneIds = collectPanes(windowStore.layout).map((p) => p.id);
    expect(paneIds).toEqual(expect.arrayContaining(['good-pane', 'second-pane']));
    expect(windowStore.layout.type).toBe('split');
  });

  it('collapses an all-dangling layout to one empty pane, not to nothing', () => {
    const allDangling = {
      ...DANGLING,
      tabGroups: {},
    } as unknown as SerializedLayout;

    windowActions.importLayout(allDangling);

    // "Nothing open" is a legitimate state; a layout with no pane at all is not.
    expect(collectPanes(windowStore.layout).length).toBe(1);
    expect(windowStore.activePaneId).not.toBeNull();
  });
});
