import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import {
  collectLeafGroupIds,
  createInitialState,
  primaryEdgeGroupId,
} from '@/stores/windowStoreInternals';
import type { EdgePanelPosition, TabContentType } from '@/types/windowTypes';
import type { SerializedLayout } from '@/lib/layout-serializer';

/**
 * The two rails are FIXED: one holds Sessions, the other holds Files. A user
 * can collapse a rail, and can drag its panel elsewhere, but cannot leave the
 * shell with no Sessions panel and no Files panel at all.
 *
 * The defect these tests lock down: a restore of a layout that lacked a rail
 * brought the rail back EMPTY (an absent position deserialized to a collapsed
 * panel with no tab group), so the shell came up with no session list and no
 * file tree, and nothing on the rail could re-add one.
 */

const resetStore = () => {
  const fresh = createInitialState();
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

/** Content types the rail at `pos` currently holds. */
const railContent = (pos: EdgePanelPosition): TabContentType[] =>
  collectLeafGroupIds(windowStore.edgePanels[pos].layout).flatMap(
    (id) => windowStore.tabGroups[id]?.tabs.map((t) => t.contentType) ?? [],
  );

/** A serialized layout with one centre chat pane and the edge panels a test gives. */
const layoutWith = (
  edgePanels: Partial<SerializedLayout['edgePanels']>,
): SerializedLayout =>
  ({
    version: 9,
    layout: { id: 'centre-pane', type: 'pane', tabGroupId: 'centre-group' },
    tabGroups: {
      'centre-group': { id: 'centre-group', tabs: [], activeTabId: null },
      ...Object.fromEntries(
        Object.values(edgePanels).flatMap((p) =>
          p && 'layout' in p && p.layout.type === 'pane' && p.layout.tabGroupId
            ? [[p.layout.tabGroupId, { id: p.layout.tabGroupId, tabs: [], activeTabId: null }]]
            : [],
        ),
      ),
    },
    edgePanels,
    floatingWindows: [],
  }) as unknown as SerializedLayout;

/** An empty rail at `pos`; a test fills `${pos}-group` when it wants tabs. */
const railPanel = (pos: EdgePanelPosition) => ({
  id: `${pos}-panel`,
  layout: { id: `${pos}-pane`, type: 'pane' as const, tabGroupId: `${pos}-group` },
  isCollapsed: false,
  width: 280,
});

describe('the two rails are fixed', () => {
  beforeEach(resetStore);

  it('re-adds Sessions on the left when a restored layout has no left panel at all', () => {
    windowActions.importLayout(layoutWith({ right: railPanel('right') }));
    expect(railContent('left')).toContain('sessions');
    expect(windowStore.edgePanels.left.isCollapsed).toBe(false);
  });

  it('re-adds Files on the right when a restored layout has no right panel at all', () => {
    windowActions.importLayout(layoutWith({ left: railPanel('left') }));
    expect(railContent('right')).toContain('files');
  });

  it('re-adds Sessions when the restored left rail exists but is empty', () => {
    const json = layoutWith({
      left: railPanel('left'),
      right: railPanel('right'),
    });
    (json.tabGroups as Record<string, { id: string; tabs: unknown[]; activeTabId: null }>)[
      'right-group'
    ].tabs = [{ id: 'files-tab', title: 'Files', contentType: 'files' }];
    windowActions.importLayout(json);
    expect(railContent('left')).toContain('sessions');
    expect(railContent('right')).toContain('files');
  });

  it('opens the re-added rail on its own panel', () => {
    windowActions.importLayout(layoutWith({ right: railPanel('right') }));
    const groupId = primaryEdgeGroupId(windowStore, 'left')!;
    const group = windowStore.tabGroups[groupId];
    expect(group.tabs.some((t) => t.contentType === 'sessions')).toBe(true);
    expect(group.activeTabId).toBe(group.tabs.find((t) => t.contentType === 'sessions')!.id);
  });

  it('does not add a second copy when the panel is open somewhere else', () => {
    const json = layoutWith({ left: railPanel('left'), right: railPanel('right') });
    (json.tabGroups as Record<string, { id: string; tabs: unknown[]; activeTabId: string | null }>)[
      'centre-group'
    ] = {
      id: 'centre-group',
      tabs: [
        { id: 'sessions-tab', title: 'Sessions', contentType: 'sessions' },
        { id: 'files-tab', title: 'Files', contentType: 'files' },
      ],
      activeTabId: 'sessions-tab',
    };
    windowActions.importLayout(json);
    const all = Object.values(windowStore.tabGroups).flatMap((g) => g.tabs);
    expect(all.filter((t) => t.contentType === 'sessions')).toHaveLength(1);
    expect(all.filter((t) => t.contentType === 'files')).toHaveLength(1);
  });

  it('gives a reset both rails', () => {
    windowActions.importLayout(layoutWith({}));
    windowActions.resetLayoutToDefaults();
    expect(railContent('left')).toContain('sessions');
    expect(railContent('right')).toContain('files');
  });

  it('refuses to close the last Sessions panel', () => {
    const groupId = primaryEdgeGroupId(windowStore, 'left')!;
    const tabId = windowStore.tabGroups[groupId].tabs.find((t) => t.contentType === 'sessions')!.id;
    windowActions.removeTab(groupId, tabId);
    expect(railContent('left')).toContain('sessions');
  });

  it('refuses to close the last Files panel', () => {
    const groupId = primaryEdgeGroupId(windowStore, 'right')!;
    const tabId = windowStore.tabGroups[groupId].tabs.find((t) => t.contentType === 'files')!.id;
    windowActions.removeTab(groupId, tabId);
    expect(railContent('right')).toContain('files');
  });

  it('still closes a second copy of a rail panel', () => {
    const groupId = primaryEdgeGroupId(windowStore, 'right')!;
    windowActions.addTab(groupId, {
      id: 'sessions-copy',
      title: 'Sessions',
      contentType: 'sessions',
    });
    windowActions.removeTab(groupId, 'sessions-copy');
    expect(windowStore.tabGroups[groupId].tabs.some((t) => t.id === 'sessions-copy')).toBe(false);
    expect(railContent('left')).toContain('sessions');
  });

  it('still closes a tab that is not a rail panel', () => {
    const groupId = primaryEdgeGroupId(windowStore, 'right')!;
    const backlinks = windowStore.tabGroups[groupId].tabs.find((t) => t.contentType === 'backlinks')!;
    windowActions.removeTab(groupId, backlinks.id);
    expect(railContent('right')).not.toContain('backlinks');
  });
});
