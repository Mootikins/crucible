import { describe, it, expect } from 'vitest';
import { iconForContentType } from '../tab-icons';
import { serializeLayout, deserializeLayout } from '../layout-serializer';
import { getGlobalRegistry, resetGlobalRegistry } from '../panel-registry';
import type { WindowState } from '@/stores/windowStore';

function createTestState(): WindowState {
  const tabGroupId1 = 'group-1';
  const leftGroupId = 'edge-left-group';
  const rightGroupId = 'edge-right-group';
  const bottomGroupId = 'edge-bottom-group';

  return {
    layout: {
      id: 'pane-1',
      type: 'pane',
      tabGroupId: tabGroupId1,
    },
    tabGroups: {
      [tabGroupId1]: {
        id: tabGroupId1,
        tabs: [
          { id: 'tab-1', title: 'File 1', contentType: 'file' },
          { id: 'tab-2', title: 'File 2', contentType: 'file' },
        ],
        activeTabId: 'tab-1',
      },
      [leftGroupId]: {
        id: leftGroupId,
        tabs: [
          { id: 'explorer-tab', title: 'Explorer', contentType: 'tool' },
          { id: 'search-tab', title: 'Search', contentType: 'tool' },
        ],
        activeTabId: 'explorer-tab',
      },
      [rightGroupId]: {
        id: rightGroupId,
        tabs: [{ id: 'outline-tab', title: 'Outline', contentType: 'tool' }],
        activeTabId: 'outline-tab',
      },
      [bottomGroupId]: {
        id: bottomGroupId,
        tabs: [
          { id: 'terminal-tab', title: 'Terminal', contentType: 'terminal' },
        ],
        activeTabId: 'terminal-tab',
      },
    },
    edgePanels: {
      left: {
        id: 'left-panel',
        tabGroupId: leftGroupId,
        isCollapsed: false,
        width: 250,
      },
      right: {
        id: 'right-panel',
        tabGroupId: rightGroupId,
        isCollapsed: true,
        width: 250,
      },
      bottom: {
        id: 'bottom-panel',
        tabGroupId: bottomGroupId,
        isCollapsed: false,
        height: 200,
      },
    },
    floatingWindows: [],
    activePaneId: 'pane-1',
    focusedRegion: 'center',
    nextZIndex: 1,
  };
}

describe('layout-serializer', () => {
  it('round-trip serialization preserves state', () => {
    const state = createTestState();
    const serialized = serializeLayout(state);
    const deserialized = deserializeLayout(serialized);

    expect(serialized.version).toBe(3);

    expect(deserialized.edgePanels.left.tabGroupId).toBeDefined();
    expect(deserialized.edgePanels.right.tabGroupId).toBeDefined();
    expect(deserialized.edgePanels.bottom.tabGroupId).toBeDefined();

    const leftGroup = deserialized.tabGroups[deserialized.edgePanels.left.tabGroupId];
    expect(leftGroup).toBeDefined();
    expect(leftGroup.tabs.length).toBe(2);
    expect(leftGroup.tabs[0].title).toBe('Explorer');

    for (const tab of leftGroup.tabs) {
      expect((tab as any).panelPosition).toBeUndefined();
    }
  });

  it('v1 migration creates edge tab groups', () => {
    const v1Json = {
      version: 1,
      edgePanels: {
        left: {
          id: 'left-panel',
          position: 'left',
          tabs: [
            { id: 'tab1', title: 'Explorer', contentType: 'tool', panelPosition: 'left' },
            { id: 'tab2', title: 'Search', contentType: 'tool', panelPosition: 'left' },
          ],
          activeTabId: 'tab1',
          isCollapsed: false,
          width: 250,
        },
        right: {
          id: 'right-panel',
          position: 'right',
          tabs: [],
          activeTabId: null,
          isCollapsed: true,
          width: 250,
        },
        bottom: {
          id: 'bottom-panel',
          position: 'bottom',
          tabs: [],
          activeTabId: null,
          isCollapsed: false,
          height: 200,
        },
      },
      layout: { id: 'pane1', type: 'pane', tabGroupId: 'group1' },
      tabGroups: {
        group1: { id: 'group1', tabs: [], activeTabId: null },
      },
      floatingWindows: [],
    };

    const deserialized = deserializeLayout(v1Json as any);

    expect(deserialized.edgePanels.left.tabGroupId).toBeDefined();
    expect((deserialized.edgePanels.left as any).tabs).toBeUndefined();
    expect((deserialized.edgePanels.left as any).position).toBeUndefined();

    const leftGroup = deserialized.tabGroups[deserialized.edgePanels.left.tabGroupId];
    expect(leftGroup).toBeDefined();
    expect(leftGroup.tabs.length).toBe(2);
    expect(leftGroup.tabs[0].title).toBe('Explorer');
    expect(leftGroup.tabs[1].title).toBe('Search');

    expect((leftGroup.tabs[0] as any).panelPosition).toBeUndefined();
    expect((leftGroup.tabs[1] as any).panelPosition).toBeUndefined();
  });

  it('v1 migration preserves activeTabId', () => {
    const v1Json = {
      version: 1,
      edgePanels: {
        left: {
          id: 'left-panel',
          position: 'left',
          tabs: [
            { id: 'tab1', title: 'Explorer', contentType: 'tool', panelPosition: 'left' },
            { id: 'tab2', title: 'Search', contentType: 'tool', panelPosition: 'left' },
          ],
          activeTabId: 'tab2',
          isCollapsed: false,
          width: 250,
        },
        right: {
          id: 'right-panel',
          position: 'right',
          tabs: [],
          activeTabId: null,
          isCollapsed: true,
        },
        bottom: {
          id: 'bottom-panel',
          position: 'bottom',
          tabs: [],
          activeTabId: null,
          isCollapsed: false,
        },
      },
      layout: { id: 'pane1', type: 'pane', tabGroupId: 'group1' },
      tabGroups: {
        group1: { id: 'group1', tabs: [], activeTabId: null },
      },
      floatingWindows: [],
    };

    const deserialized = deserializeLayout(v1Json as any);
    const leftGroup = deserialized.tabGroups[deserialized.edgePanels.left.tabGroupId];

    expect(leftGroup.activeTabId).toBe('tab2');
  });

  it('v1 migration handles empty edge panels', () => {
    const v1Json = {
      version: 1,
      edgePanels: {
        left: {
          id: 'left-panel',
          position: 'left',
          tabs: [],
          activeTabId: null,
          isCollapsed: true,
          width: 250,
        },
        right: {
          id: 'right-panel',
          position: 'right',
          tabs: [],
          activeTabId: null,
          isCollapsed: true,
        },
        bottom: {
          id: 'bottom-panel',
          position: 'bottom',
          tabs: [],
          activeTabId: null,
          isCollapsed: false,
        },
      },
      layout: { id: 'pane1', type: 'pane', tabGroupId: 'group1' },
      tabGroups: {
        group1: { id: 'group1', tabs: [], activeTabId: null },
      },
      floatingWindows: [],
    };

    const deserialized = deserializeLayout(v1Json as any);

    expect(deserialized.edgePanels.left.tabGroupId).toBeDefined();
    expect(deserialized.edgePanels.right.tabGroupId).toBeDefined();
    expect(deserialized.edgePanels.bottom.tabGroupId).toBeDefined();

    const leftGroup = deserialized.tabGroups[deserialized.edgePanels.left.tabGroupId];
    expect(leftGroup.tabs.length).toBe(0);
    expect(leftGroup.activeTabId).toBeNull();
  });

  it('unknown version throws error', () => {
    const badJson = { version: 99 } as any;
    expect(() => deserializeLayout(badJson)).toThrow('Unsupported layout version: 99');
  });

  it('serialized format is preserved on round-trip', () => {
    const state = createTestState();
    const serialized1 = serializeLayout(state);
    const deserialized1 = deserializeLayout(serialized1);
    const serialized2 = serializeLayout(deserialized1);

    expect(serialized2.version).toBe(3);
    expect(serialized2.edgePanels.left.tabGroupId).toBe(serialized1.edgePanels.left.tabGroupId);
    expect((serialized2.edgePanels.left as any).tabs).toBeUndefined();
    expect((serialized2.edgePanels.left as any).position).toBeUndefined();
  });

  it('file tab metadata survives round-trip serialization', () => {
    const tabGroupId = 'group-with-file-tabs';
    const state: WindowState = {
      layout: {
        id: 'pane-1',
        type: 'pane',
        tabGroupId,
      },
      tabGroups: {
        [tabGroupId]: {
          id: tabGroupId,
          tabs: [
            {
              id: 'tab-file-test',
              title: 'test.md',
              contentType: 'file',
              metadata: { filePath: '/path/to/test.md' },
            },
            {
              id: 'tab-file-other',
              title: 'other.ts',
              contentType: 'file',
              metadata: { filePath: '/path/to/other.ts', encoding: 'utf-8' },
            },
          ],
          activeTabId: 'tab-file-test',
        },
      },
      edgePanels: {
        left: {
          id: 'left-panel',
          tabGroupId: 'edge-left-group',
          isCollapsed: false,
          width: 250,
        },
        right: {
          id: 'right-panel',
          tabGroupId: 'edge-right-group',
          isCollapsed: true,
          width: 250,
        },
        bottom: {
          id: 'bottom-panel',
          tabGroupId: 'edge-bottom-group',
          isCollapsed: false,
          height: 200,
        },
      },
      floatingWindows: [],
      activePaneId: 'pane-1',
      focusedRegion: 'center',
      nextZIndex: 1,
    };

    // Serialize and deserialize
    const serialized = serializeLayout(state);
    const deserialized = deserializeLayout(serialized);

    // Verify metadata is preserved
    const group = deserialized.tabGroups[tabGroupId];
    expect(group).toBeDefined();
    expect(group.tabs.length).toBe(2);

    const fileTab1 = group.tabs[0];
    expect(fileTab1.id).toBe('tab-file-test');
    expect(fileTab1.metadata).toBeDefined();
    expect(fileTab1.metadata?.filePath).toBe('/path/to/test.md');

    const fileTab2 = group.tabs[1];
    expect(fileTab2.id).toBe('tab-file-other');
    expect(fileTab2.metadata).toBeDefined();
    expect(fileTab2.metadata?.filePath).toBe('/path/to/other.ts');
    expect(fileTab2.metadata?.encoding).toBe('utf-8');
  });

  it('rehydrates tab icons from content type on deserialize', () => {
    // Icons are components: stripped by serialize, so restore must resolve
    // them again — regression for iconless tabs after a persisted-layout
    // load (previously masked when /api/layout failed with 401).
    const state = createTestState();
    state.tabGroups['group-1'].tabs = [
      { id: 'sessions-tab', title: 'Sessions', contentType: 'sessions', icon: iconForContentType('sessions') },
      { id: 'tab-chat-x', title: 'Chat', contentType: 'chat', metadata: { sessionId: 'x' } },
    ];

    const restored = deserializeLayout(serializeLayout(state));

    const tabs = restored.tabGroups['group-1'].tabs;
    expect(tabs[0].icon).toBe(iconForContentType('sessions'));
    expect(tabs[1].icon).toBe(iconForContentType('chat'));
    expect(tabs[0].icon).toBeTypeOf('function');
  });
});

describe('layout v2→v3 migration prunes removed content types', () => {
  const Dummy = () => null;

  // A v2 layout persisted before the placeholder panels were deleted: the
  // left group mixes a live panel (sessions) with ghosts (explorer/search),
  // and the right group is nothing but a ghost (outline).
  const v2WithGhosts = () => ({
    version: 2 as const,
    layout: { id: 'p', type: 'pane' as const, tabGroupId: 'center' },
    tabGroups: {
      center: { id: 'center', tabs: [{ id: 'home', title: 'Home', contentType: 'home' }], activeTabId: 'home' },
      left: {
        id: 'left',
        tabs: [
          { id: 'sessions-tab', title: 'Sessions', contentType: 'sessions' },
          { id: 'explorer-tab', title: 'Explorer', contentType: 'explorer' },
          { id: 'search-tab', title: 'Search', contentType: 'search' },
        ],
        activeTabId: 'explorer-tab',
      },
      right: { id: 'right', tabs: [{ id: 'outline-tab', title: 'Outline', contentType: 'outline' }], activeTabId: 'outline-tab' },
      orphan: { id: 'orphan', tabs: [{ id: 'output-tab', title: 'Output', contentType: 'output' }], activeTabId: 'output-tab' },
    },
    edgePanels: {
      left: { id: 'left-panel', tabGroupId: 'left', isCollapsed: false, width: 250 },
      right: { id: 'right-panel', tabGroupId: 'right', isCollapsed: true, width: 250 },
      bottom: { id: 'bottom-panel', tabGroupId: 'center', isCollapsed: true, height: 200 },
    },
    floatingWindows: [],
  });

  function withRegistry(fn: () => void) {
    resetGlobalRegistry();
    const reg = getGlobalRegistry();
    for (const id of ['home', 'sessions', 'terminal', 'chat']) {
      reg.register(id, id, Dummy, 'center', '•');
    }
    try {
      fn();
    } finally {
      resetGlobalRegistry();
    }
  }

  it('drops tabs whose content type is no longer registered', () => {
    withRegistry(() => {
      const restored = deserializeLayout(v2WithGhosts() as never);
      const left = restored.tabGroups['left'].tabs.map((t) => t.contentType);
      expect(left).toEqual(['sessions']);
    });
  });

  it('fixes an activeTabId that pointed at a pruned tab', () => {
    withRegistry(() => {
      const restored = deserializeLayout(v2WithGhosts() as never);
      // was 'explorer-tab' (pruned) → falls back to the first surviving tab
      expect(restored.tabGroups['left'].activeTabId).toBe('sessions-tab');
    });
  });

  it('keeps a referenced group that emptied out (edge ref stays valid)', () => {
    withRegistry(() => {
      const restored = deserializeLayout(v2WithGhosts() as never);
      const right = restored.tabGroups['right'];
      expect(right).toBeDefined();
      expect(right.tabs).toEqual([]);
      expect(right.activeTabId).toBeNull();
      expect(restored.edgePanels.right.tabGroupId).toBe('right');
    });
  });

  it('drops an emptied group that nothing references', () => {
    withRegistry(() => {
      const restored = deserializeLayout(v2WithGhosts() as never);
      expect(restored.tabGroups['orphan']).toBeUndefined();
    });
  });

  it('does not prune when the registry is empty (defensive)', () => {
    resetGlobalRegistry();
    const restored = deserializeLayout(v2WithGhosts() as never);
    // Nothing registered → every tab is treated as unknown-but-kept.
    expect(restored.tabGroups['left'].tabs.length).toBe(3);
  });
});
