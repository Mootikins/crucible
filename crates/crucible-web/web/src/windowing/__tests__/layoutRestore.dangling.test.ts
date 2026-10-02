import { describe, it, expect, beforeEach } from 'vitest';
import { configureWindowing, windowStore, windowActions } from '@/windowing/store';
import { collectPanes } from '@/windowing/model/tree';
import type { SerializedLayout } from '@/windowing/model/serializer';
import { neutralPolicy } from '@/windowing/testing/neutralPolicy';

/**
 * Reduced from a layout taken off a running daemon (`GET /api/layout`), which
 * is where this defect was found. The shape that matters: a split whose second
 * pane names a tab group that is NOT in `tabGroups`.
 *
 * That pane drew EmptyPane on every load and no user action could remove it —
 * opening a session or a file adds a tab to the OTHER pane, and none of those
 * paths collapse anything. Only tab-close and window operations ran
 * `collapseEmptyNodes`, and restore never did.
 *
 * The payload is in the current format, with the rails of the neutral seed,
 * so the read needs no legacy upgrade.
 */
const dangling = (): SerializedLayout => ({
  ...windowActions.exportLayout(),
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
      tabs: [{ id: 'chat-1', title: 'Test Session', contentType: 'alpha' }],
      activeTabId: 'chat-1',
    },
  },
});

describe('restoring a layout with a dangling tab group', () => {
  beforeEach(() => configureWindowing(neutralPolicy()));

  it('drops the pane whose tab group no longer exists', () => {
    windowActions.importLayout(dangling());

    const paneIds = collectPanes(windowStore.layout).map((p) => p.id);
    expect(paneIds).not.toContain('orphan-pane');
    expect(paneIds).toContain('good-pane');
    // The split collapsed with it — a split with one child is not a split.
    expect(windowStore.layout.type).toBe('pane');
  });

  it('keeps the surviving pane focusable', () => {
    windowActions.importLayout(dangling());
    expect(windowStore.activePaneId).toBe('good-pane');
  });

  it('leaves a layout whose groups all resolve untouched', () => {
    const base = dangling();
    const healthy: SerializedLayout = {
      ...base,
      layout: {
        ...base.layout,
        second: { id: 'second-pane', type: 'pane', tabGroupId: 'files-group' },
      },
      tabGroups: {
        ...base.tabGroups,
        'files-group': {
          id: 'files-group',
          tabs: [{ id: 'files-1', title: 'Files', contentType: 'beta' }],
          activeTabId: 'files-1',
        },
      },
    } as SerializedLayout;

    windowActions.importLayout(healthy);

    const paneIds = collectPanes(windowStore.layout).map((p) => p.id);
    expect(paneIds).toEqual(expect.arrayContaining(['good-pane', 'second-pane']));
    expect(windowStore.layout.type).toBe('split');
  });

  it('collapses an all-dangling layout to one empty pane, not to nothing', () => {
    const allDangling: SerializedLayout = {
      ...dangling(),
      tabGroups: {},
    };

    windowActions.importLayout(allDangling);

    // "Nothing open" is a legitimate state; a layout with no pane at all is not.
    expect(collectPanes(windowStore.layout).length).toBe(1);
    expect(windowStore.activePaneId).not.toBeNull();
  });
});

it('repairs a saved right rail with a dangling middle pane while keeping its folded lower pane', () => {
  configureWindowing(neutralPolicy());
  const saved = windowActions.exportLayout();
  const chatGroup = collectPanes(saved.edgePanels.right.layout)[0].tabGroupId!;
  saved.tabGroups.lower = { id: 'lower', tabs: [{ id: 'lower-tab', title: 'Lower', contentType: 'gamma' }], activeTabId: 'lower-tab' };
  saved.tabGroups.empty = { id: 'empty', tabs: [], activeTabId: null };
  const left = saved.edgePanels.left.layout;
  saved.edgePanels.left.layout = {
    id: 'left-stack', type: 'split', direction: 'vertical', splitRatio: .6,
    first: left, second: { id: 'valid-empty-pane', type: 'pane', tabGroupId: 'empty' },
  };
  saved.edgePanels.right.layout = {
    id: 'right-stack', type: 'split', direction: 'vertical', splitRatio: .7,
    first: {
      id: 'chat-and-orphan', type: 'split', direction: 'vertical', splitRatio: .5,
      first: { id: 'chat-pane', type: 'pane', tabGroupId: chatGroup },
      second: { id: 'blank-pane', type: 'pane', tabGroupId: 'deleted-group' },
    },
    second: { id: 'lower-pane', type: 'pane', tabGroupId: 'lower', collapsed: true },
  };
  windowActions.importLayout(saved);
  expect(windowStore.edgePanels.right.layout).toMatchObject({
    id: 'right-stack', splitRatio: .7,
    first: { id: 'chat-pane', type: 'pane', tabGroupId: chatGroup },
    second: { id: 'lower-pane', collapsed: true },
  });
  expect(collectPanes(windowStore.edgePanels.left.layout).map(pane => pane.id)).toContain('valid-empty-pane');
  expect(windowStore.tabGroups.empty).toMatchObject({ tabs: [], activeTabId: null });
});

it.each(['deleted-group', null])('gives a retained sole centre pane a usable group when saved group is %s', (tabGroupId) => {
  configureWindowing(neutralPolicy());
  const saved = windowActions.exportLayout();
  saved.layout = { id: 'retained-center', type: 'pane', tabGroupId };
  windowActions.importLayout(saved);
  const group = windowActions.getPaneTabGroupId('retained-center');
  expect(group).not.toBeNull();
  expect(windowStore.tabGroups[group!]).toMatchObject({ tabs: [], activeTabId: null });
  const railGroup = collectPanes(windowStore.edgePanels.right.layout)[0].tabGroupId!;
  windowActions.moveTab(railGroup, group!, 'tab-right');
  expect(windowStore.tabGroups[group!].tabs.map(tab => tab.id)).toEqual(['tab-right']);
});
