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

/** Every tab of the restored store, in no particular order. */
const allTabs = () => Object.values(windowStore.tabGroups).flatMap((g) => g.tabs);

/**
 * A layout saved while settings was still a PAGE.
 *
 * Settings is a dialog now. `registerPanels` registers no `settings` content
 * type, so the tab this layout carries has nothing to render and the centre
 * strip draws "Unknown content type" on every load.
 *
 * The pane holds nothing else, which is the case that needs the prune: drop
 * the tab alone and the user gets an empty pane beside their work that no
 * action can close.
 */
const SETTINGS_ONLY: SerializedLayout = {
  version: 8,
  layout: {
    id: 'root-split',
    type: 'split',
    direction: 'horizontal',
    splitRatio: 0.5,
    first: { id: 'chat-pane', type: 'pane', tabGroupId: 'chat-group' },
    second: { id: 'settings-pane', type: 'pane', tabGroupId: 'settings-group' },
  },
  tabGroups: {
    'chat-group': {
      id: 'chat-group',
      tabs: [
        { id: 'chat-1', title: 'Test Session', contentType: 'chat', metadata: { sessionId: 'x' } },
      ],
      activeTabId: 'chat-1',
    },
    'settings-group': {
      id: 'settings-group',
      tabs: [{ id: 'tab-settings', title: 'Settings', contentType: 'settings' }],
      activeTabId: 'tab-settings',
    },
  },
  edgePanels: defaultLayout().edgePanels,
  floatingWindows: [],
} as unknown as SerializedLayout;

/** The same tab, beside work the user still wants. */
const SETTINGS_BESIDE_WORK: SerializedLayout = {
  ...SETTINGS_ONLY,
  layout: { id: 'only-pane', type: 'pane', tabGroupId: 'mixed-group' },
  tabGroups: {
    'mixed-group': {
      id: 'mixed-group',
      tabs: [
        { id: 'tab-settings', title: 'Settings', contentType: 'settings' },
        { id: 'file-1', title: 'Note.md', contentType: 'file' },
      ],
      activeTabId: 'tab-settings',
    },
  },
} as unknown as SerializedLayout;

describe('restoring a layout that still holds a settings tab', () => {
  beforeEach(resetStore);

  it('drops the tab', () => {
    windowActions.importLayout(SETTINGS_ONLY);

    expect(allTabs().find((t) => t.contentType === 'settings')).toBeUndefined();
  });

  it('takes the pane the tab emptied with it', () => {
    windowActions.importLayout(SETTINGS_ONLY);

    const paneIds = collectPanes(windowStore.layout).map((p) => p.id);
    expect(paneIds).not.toContain('settings-pane');
    expect(paneIds).toContain('chat-pane');
    // A split with one child is not a split.
    expect(windowStore.layout.type).toBe('pane');
  });

  it('leaves no pane pointing at a tab group that is gone', () => {
    windowActions.importLayout(SETTINGS_ONLY);

    for (const pane of collectPanes(windowStore.layout)) {
      if (pane.tabGroupId === null) continue;
      expect(windowStore.tabGroups[pane.tabGroupId], pane.id).toBeDefined();
    }
    expect(windowStore.tabGroups['settings-group']).toBeUndefined();
  });

  it('keeps the work beside it and moves the active tab', () => {
    windowActions.importLayout(SETTINGS_BESIDE_WORK);

    const group = windowStore.tabGroups['mixed-group'];
    expect(group.tabs.map((t) => t.id)).toEqual(['file-1']);
    expect(group.activeTabId).toBe('file-1');
    expect(collectPanes(windowStore.layout).map((p) => p.id)).toEqual(['only-pane']);
  });

  // A layout STORED at this version skips every migration above, so this one
  // is the first to touch it and may assume no part of the shape is there.
  it('takes a payload that carries no edge panels at all', () => {
    const partial = {
      version: 8,
      layout: SETTINGS_ONLY.layout,
      tabGroups: SETTINGS_ONLY.tabGroups,
      floatingWindows: [],
    } as unknown as SerializedLayout;

    expect(() => windowActions.importLayout(partial)).not.toThrow();
    expect(allTabs().find((t) => t.contentType === 'settings')).toBeUndefined();
    expect(windowStore.edgePanels.left).toBeDefined();
    expect(windowStore.edgePanels.right).toBeDefined();
  });

  it('collapses a settings-only layout to one empty pane, not to nothing', () => {
    const settingsAlone = {
      ...SETTINGS_ONLY,
      layout: { id: 'only-pane', type: 'pane', tabGroupId: 'settings-group' },
      tabGroups: {
        'settings-group': SETTINGS_ONLY.tabGroups['settings-group'],
      },
    } as unknown as SerializedLayout;

    windowActions.importLayout(settingsAlone);

    // "Nothing open" is a legitimate state; a layout with no pane at all is not.
    expect(collectPanes(windowStore.layout).length).toBe(1);
    expect(collectPanes(windowStore.layout)[0]!.tabGroupId).toBeNull();
    expect(windowStore.activePaneId).not.toBeNull();
  });
});
