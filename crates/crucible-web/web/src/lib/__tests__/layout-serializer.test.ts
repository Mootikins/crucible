import { describe, it, expect } from 'vitest';
import { iconForContentType } from '../tab-icons';
import { serializeLayout, deserializeLayout } from '../layout-serializer';
import { getGlobalRegistry, resetGlobalRegistry } from '../panel-registry';
import type { WindowState } from '@/stores/windowStore';
import type { LayoutNode } from '@/types/windowTypes';

const paneLayout = (id: string, tabGroupId: string): LayoutNode => ({
  id,
  type: 'pane',
  tabGroupId,
});

/** Group id of a single-pane edge panel (state or serialized shape). */
const panelGroupId = (panel: { layout: LayoutNode }): string | null =>
  panel.layout.type === 'pane' ? panel.layout.tabGroupId : null;

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
        layout: paneLayout('left-pane', leftGroupId),
        isCollapsed: false,
        width: 250,
      },
      right: {
        id: 'right-panel',
        layout: paneLayout('right-pane', rightGroupId),
        isCollapsed: true,
        width: 250,
      },
      bottom: {
        id: 'bottom-panel',
        layout: paneLayout('bottom-pane', bottomGroupId),
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

    expect(serialized.version).toBe(6);

    expect(panelGroupId(deserialized.edgePanels.left)!).toBeDefined();
    expect(panelGroupId(deserialized.edgePanels.right)!).toBeDefined();
    expect(panelGroupId(deserialized.edgePanels.bottom)!).toBeDefined();

    const leftGroup = deserialized.tabGroups[panelGroupId(deserialized.edgePanels.left)!];
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

    expect(panelGroupId(deserialized.edgePanels.left)!).toBeDefined();
    expect((deserialized.edgePanels.left as any).tabs).toBeUndefined();
    expect((deserialized.edgePanels.left as any).position).toBeUndefined();

    const leftGroup = deserialized.tabGroups[panelGroupId(deserialized.edgePanels.left)!];
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
    const leftGroup = deserialized.tabGroups[panelGroupId(deserialized.edgePanels.left)!];

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

    expect(panelGroupId(deserialized.edgePanels.left)!).toBeDefined();
    expect(panelGroupId(deserialized.edgePanels.right)!).toBeDefined();
    expect(panelGroupId(deserialized.edgePanels.bottom)!).toBeDefined();

    const leftGroup = deserialized.tabGroups[panelGroupId(deserialized.edgePanels.left)!];
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

    expect(serialized2.version).toBe(6);
    expect(panelGroupId(serialized2.edgePanels.left)).toBe(panelGroupId(serialized1.edgePanels.left));
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
          layout: paneLayout('left-pane', 'edge-left-group'),
          isCollapsed: false,
          width: 250,
        },
        right: {
          id: 'right-panel',
          layout: paneLayout('right-pane', 'edge-right-group'),
          isCollapsed: true,
          width: 250,
        },
        bottom: {
          id: 'bottom-panel',
          layout: paneLayout('bottom-pane', 'edge-bottom-group'),
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
    expect(tabs[0].icon).toBeTypeOf('function');
    // The chat tab migrates from the center group to the right panel group
    // on restore — icon must still rehydrate wherever it lands.
    const chatTab = Object.values(restored.tabGroups)
      .flatMap((g) => g.tabs)
      .find((t) => t.id === 'tab-chat-x');
    expect(chatTab?.icon).toBe(iconForContentType('chat'));
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
      reg.register(id, id, Dummy, 'center');
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
      expect(panelGroupId(restored.edgePanels.right)!).toBe('right');
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

describe('legacy generic chat tabs are pruned on every restore', () => {
  const v3 = () => ({
    version: 3 as const,
    layout: { id: 'p', type: 'pane' as const, tabGroupId: 'center' },
    tabGroups: {
      center: {
        id: 'center',
        tabs: [
          { id: 'tab-home', title: 'Home', contentType: 'home' },
          // Pre-WS-220 generic Chat panel: no sessionId — renders the active
          // session wherever it is docked, defeating right-pane placement.
          { id: 'tab-chat', title: 'Chat', contentType: 'chat' },
          // Session-bound chat tab: must survive.
          {
            id: 'tab-chat-abc',
            title: 'My Session',
            contentType: 'chat',
            metadata: { sessionId: 'abc' },
          },
        ],
        activeTabId: 'tab-chat',
      },
    },
    edgePanels: {
      left: { id: 'left-panel', tabGroupId: 'center', isCollapsed: false, width: 250 },
      right: { id: 'right-panel', tabGroupId: 'center', isCollapsed: true, width: 250 },
      bottom: { id: 'bottom-panel', tabGroupId: 'center', isCollapsed: true, height: 200 },
    },
    floatingWindows: [],
  });

  it('drops session-less chat tabs and fixes activeTabId', () => {
    const restored = deserializeLayout(v3() as never);
    const ids = restored.tabGroups['center'].tabs.map((t) => t.id);
    // The session-bound tab is then MIGRATED to the right panel group
    // (which here is 'center' itself for left/right/bottom — see the
    // dedicated migration suite below for the real shape).
    expect(ids).toContain('tab-home');
    expect(ids).not.toContain('tab-chat');
    expect(restored.tabGroups['center'].activeTabId).toBe('tab-home');
  });

  it('v3→v4 bumps a narrow right panel to chat-worthy width, leaves wider ones alone', () => {
    const narrow = deserializeLayout(v3() as never);
    expect(narrow.edgePanels.right.width).toBe(520);
    // Left panel is not a session dock — untouched.
    expect(narrow.edgePanels.left.width).toBe(250);

    const wide = v3();
    wide.edgePanels.right.width = 800;
    const restored = deserializeLayout(wide as never);
    expect(restored.edgePanels.right.width).toBe(800);
  });
});

describe('center chat tabs migrate to the right edge panel on restore', () => {
  const v3Split = () => ({
    version: 3 as const,
    layout: {
      id: 'root',
      type: 'split' as const,
      direction: 'horizontal' as const,
      first: { id: 'p-editor', type: 'pane' as const, tabGroupId: 'g-editor' },
      second: { id: 'p-chat', type: 'pane' as const, tabGroupId: 'g-chat' },
    },
    tabGroups: {
      'g-editor': {
        id: 'g-editor',
        tabs: [{ id: 'tab-file-a', title: 'a.md', contentType: 'file' }],
        activeTabId: 'tab-file-a',
      },
      // The center-split era chat pane: sessions used to open here.
      'g-chat': {
        id: 'g-chat',
        tabs: [
          { id: 'tab-chat-s1', title: 'One', contentType: 'chat', metadata: { sessionId: 's1' } },
        ],
        activeTabId: 'tab-chat-s1',
      },
      'g-right': { id: 'g-right', tabs: [], activeTabId: null },
      'g-left': { id: 'g-left', tabs: [], activeTabId: null },
      'g-bottom': { id: 'g-bottom', tabs: [], activeTabId: null },
    },
    edgePanels: {
      left: { id: 'left-panel', tabGroupId: 'g-left', isCollapsed: false, width: 250 },
      right: { id: 'right-panel', tabGroupId: 'g-right', isCollapsed: true, width: 250 },
      bottom: { id: 'bottom-panel', tabGroupId: 'g-bottom', isCollapsed: true, height: 200 },
    },
    floatingWindows: [],
  });

  it('moves the chat tab right and collapses the emptied center pane', () => {
    const restored = deserializeLayout(v3Split() as never);
    // Chat tab landed in the right panel group and became its active tab.
    // `files-tab` trails it: v5→v6 seeds the file tree into the right panel.
    expect(restored.tabGroups['g-right'].tabs.map((t) => t.id)).toEqual([
      'files-tab',
      'tab-chat-s1',
    ]);
    expect(restored.tabGroups['g-right'].activeTabId).toBe('tab-chat-s1');
    // The emptied chat pane collapsed: the layout is the editor pane alone.
    expect(restored.layout.type).toBe('pane');
    expect((restored.layout as { tabGroupId?: string }).tabGroupId).toBe('g-editor');
    // The orphaned group is gone.
    expect(restored.tabGroups['g-chat']).toBeUndefined();
  });

  it('leaves non-chat tabs in place and keeps mixed panes alive', () => {
    const json = v3Split();
    json.tabGroups['g-chat'].tabs.push({
      id: 'tab-file-b',
      title: 'b.md',
      contentType: 'file',
    } as never);
    const restored = deserializeLayout(json as never);
    expect(restored.layout.type).toBe('split');
    expect(restored.tabGroups['g-chat'].tabs.map((t) => t.id)).toEqual(['tab-file-b']);
    expect(restored.tabGroups['g-chat'].activeTabId).toBe('tab-file-b');
    expect(restored.tabGroups['g-right'].tabs.map((t) => t.id)).toEqual([
      'files-tab',
      'tab-chat-s1',
    ]);
  });

  it('chat tabs already in the right panel group stay put', () => {
    const json = v3Split();
    json.tabGroups['g-right'].tabs = [
      { id: 'tab-chat-s9', title: 'Nine', contentType: 'chat', metadata: { sessionId: 's9' } },
    ] as never;
    (json.tabGroups['g-right'] as { activeTabId: string | null }).activeTabId = 'tab-chat-s9';
    const restored = deserializeLayout(json as never);
    // v5→v6 seeds `files-tab` before the chat docking runs, so the newly
    // docked session lands after it.
    expect(restored.tabGroups['g-right'].tabs.map((t) => t.id)).toEqual([
      'tab-chat-s9',
      'files-tab',
      'tab-chat-s1',
    ]);
    expect(restored.tabGroups['g-right'].activeTabId).toBe('tab-chat-s9');
  });
});

describe('v5 edge panels with split layout trees', () => {
  const splitEdgeState = (): WindowState => {
    const state = createTestState();
    state.tabGroups['edge-right-b'] = {
      id: 'edge-right-b',
      tabs: [{ id: 'tab-right-b', title: 'B', contentType: 'tool' }],
      activeTabId: 'tab-right-b',
    };
    state.edgePanels.right.layout = {
      id: 'right-split',
      type: 'split',
      direction: 'vertical',
      splitRatio: 0.3,
      first: paneLayout('right-pane-a', 'edge-right-group'),
      second: paneLayout('right-pane-b', 'edge-right-b'),
    };
    return state;
  };

  it('a split edge tree round-trips: both leaf groups, ratio, and direction survive', () => {
    const restored = deserializeLayout(serializeLayout(splitEdgeState()));
    const right = restored.edgePanels.right.layout;
    expect(right.type).toBe('split');
    if (right.type !== 'split') return;
    expect(right.direction).toBe('vertical');
    expect(right.splitRatio).toBe(0.3);
    expect(right.first).toMatchObject({ type: 'pane', tabGroupId: 'edge-right-group' });
    expect(right.second).toMatchObject({ type: 'pane', tabGroupId: 'edge-right-b' });
    expect(restored.tabGroups['edge-right-group']).toBeDefined();
    expect(restored.tabGroups['edge-right-b']).toBeDefined();
  });

  it('chat docking targets the first RESOLVABLE right-panel leaf when leaf[0] is missing', () => {
    const state = splitEdgeState();
    // First leaf references a group that no longer exists; the session-bound
    // chat tab in center must still migrate right (into the second leaf).
    delete state.tabGroups['edge-right-group'];
    state.tabGroups['group-1'].tabs.push({
      id: 'tab-chat-z',
      title: 'Z',
      contentType: 'chat',
      metadata: { sessionId: 'z' },
    });
    const restored = deserializeLayout(serializeLayout(state));
    expect(restored.tabGroups['edge-right-b'].tabs.map((t) => t.id)).toContain('tab-chat-z');
    expect(restored.tabGroups['group-1'].tabs.some((t) => t.id === 'tab-chat-z')).toBe(false);
  });

  it('a v5 layout with a null edge tree degrades to an empty pane instead of crashing', () => {
    const serialized = serializeLayout(createTestState());
    (serialized.edgePanels.right as { layout: unknown }).layout = null;
    const restored = deserializeLayout(serialized);
    expect(restored.edgePanels.right.layout).toMatchObject({ type: 'pane', tabGroupId: null });
  });

  it('a v5 layout with MISSING edge panel entries synthesizes collapsed defaults', () => {
    // Regression: a minimal/truncated payload ({ edgePanels: {} }) restored
    // a store with edgePanels[pos] === undefined — every panel, ribbon, and
    // composer read then crashed ("can't access property 'layout'") and the
    // whole shell bricked (no pane collapse/expand, dead chip popouts).
    const restored = deserializeLayout({
      version: 5,
      layout: { id: 'p', type: 'pane', tabGroupId: null },
      tabGroups: {},
      edgePanels: {},
      floatingWindows: [],
    } as never);
    for (const pos of ['left', 'right', 'bottom'] as const) {
      expect(restored.edgePanels[pos]).toBeDefined();
      expect(restored.edgePanels[pos].isCollapsed).toBe(true);
      expect(restored.edgePanels[pos].layout).toMatchObject({ type: 'pane', tabGroupId: null });
    }
  });
});

// The Navigator was one left panel whose 'files' and 'sessions' scopes were
// mutually exclusive. v6 splits it into Sessions + Search (left) and Files
// (right) so both surfaces are on screen at once.
describe('v5→v6 splits the Navigator into Sessions / Search / Files', () => {
  interface SeedTab {
    id: string;
    title: string;
    contentType: string;
  }
  interface V5Opts {
    leftTabs: SeedTab[];
    leftActive?: string | null;
    rightTabs?: SeedTab[];
    rightActive?: string | null;
    version?: number;
  }

  const DEFAULT_RIGHT: SeedTab[] = [
    { id: 'backlinks-tab', title: 'Backlinks', contentType: 'backlinks' },
  ];

  const v5Layout = (opts: V5Opts) => {
    const rightTabs = opts.rightTabs ?? DEFAULT_RIGHT;
    return {
      version: opts.version ?? 5,
      layout: paneLayout('pane-1', 'group-1'),
      tabGroups: {
        'group-1': { id: 'group-1', tabs: [], activeTabId: null },
        'edge-left-group': {
          id: 'edge-left-group',
          tabs: opts.leftTabs,
          activeTabId:
            opts.leftActive === undefined ? (opts.leftTabs[0]?.id ?? null) : opts.leftActive,
        },
        'edge-right-group': {
          id: 'edge-right-group',
          tabs: rightTabs,
          activeTabId:
            opts.rightActive === undefined ? (rightTabs[0]?.id ?? null) : opts.rightActive,
        },
      },
      edgePanels: {
        left: {
          id: 'left-panel',
          layout: paneLayout('left-pane', 'edge-left-group'),
          isCollapsed: false,
        },
        right: {
          id: 'right-panel',
          layout: paneLayout('right-pane', 'edge-right-group'),
          isCollapsed: false,
        },
        bottom: {
          id: 'bottom-panel',
          layout: paneLayout('bottom-pane', 'edge-bottom-group'),
          isCollapsed: false,
        },
      },
      floatingWindows: [],
    } as never;
  };

  const navigatorTab: SeedTab = {
    id: 'navigator-tab',
    title: 'Navigator',
    contentType: 'navigator',
  };
  const types = (group: { tabs: { contentType: string }[] }) =>
    group.tabs.map((t) => t.contentType);

  it('rewrites a Navigator tab to Sessions IN PLACE, keeping the user’s docking', () => {
    // Docked in the RIGHT panel, not the left: the migration follows where the
    // user put it instead of moving the session list to a default corner.
    const restored = deserializeLayout(
      v5Layout({
        leftTabs: [{ id: 'skills-tab', title: 'Skills', contentType: 'skills' }],
        rightTabs: [navigatorTab],
      }),
    );
    expect(
      restored.tabGroups['edge-right-group'].tabs.find((t) => t.id === 'navigator-tab'),
    ).toMatchObject({ contentType: 'sessions', title: 'Sessions' });
    expect(types(restored.tabGroups['edge-left-group'])).toEqual(['skills']);
  });

  it('adds Files to the right panel, and seeds Search nowhere', () => {
    const restored = deserializeLayout(v5Layout({ leftTabs: [navigatorTab] }));
    expect(types(restored.tabGroups['edge-left-group'])).toEqual(['sessions']);
    expect(types(restored.tabGroups['edge-right-group'])).toEqual(['backlinks', 'files']);
    for (const group of Object.values(restored.tabGroups)) {
      expect(group.tabs.map((t) => t.contentType)).not.toContain('search');
    }
  });

  it('never claims the active tab, so a session can still dock into an empty right panel', () => {
    const restored = deserializeLayout(
      v5Layout({ leftTabs: [navigatorTab], rightTabs: [], rightActive: null }),
    );
    expect(types(restored.tabGroups['edge-right-group'])).toEqual(['files']);
    expect(restored.tabGroups['edge-right-group'].activeTabId).toBeNull();
  });

  it('appends Files rather than prepending it, so a pruned active tab cannot promote it', () => {
    // A session-less `chat` tab is pruned on every restore. As tabs[0], Files
    // would inherit the fallback and the panel would open on the file tree.
    const restored = deserializeLayout(
      v5Layout({
        leftTabs: [navigatorTab],
        rightTabs: [
          { id: 'tab-chat-legacy', title: 'Chat', contentType: 'chat' },
          { id: 'backlinks-tab', title: 'Backlinks', contentType: 'backlinks' },
        ],
        rightActive: 'tab-chat-legacy',
      }),
    );
    expect(restored.tabGroups['edge-right-group'].activeTabId).toBe('backlinks-tab');
  });

  it('leaves the active tab alone — a migration must not change what you look at', () => {
    const restored = deserializeLayout(
      v5Layout({
        leftTabs: [navigatorTab, { id: 'plugins-tab', title: 'Plugins', contentType: 'plugins' }],
        leftActive: 'plugins-tab',
        rightActive: 'backlinks-tab',
      }),
    );
    expect(restored.tabGroups['edge-left-group'].activeTabId).toBe('plugins-tab');
    expect(restored.tabGroups['edge-right-group'].activeTabId).toBe('backlinks-tab');
  });

  it('keeps ONE session list when a layout holds both a sessions and a navigator tab', () => {
    // A layout written before the Navigator absorbed the separate tabs can
    // hold both; rewriting each would put two identical lists in one strip.
    const restored = deserializeLayout(
      v5Layout({
        leftTabs: [
          { id: 'sessions-tab', title: 'Sessions', contentType: 'sessions' },
          navigatorTab,
        ],
      }),
    );
    expect(types(restored.tabGroups['edge-left-group'])).toEqual(['sessions']);
  });

  it('seeds Files into the first RESOLVABLE right leaf when leaf[0] is missing', () => {
    // Same shape the WS-220 chat docking already handles: a split right panel
    // whose first leaf names a group that is gone. Taking leaf[0] blindly
    // yields `undefined`, and the optional-chained push then drops the Files
    // tab with no error at all.
    const base = v5Layout({ leftTabs: [navigatorTab] }) as never as {
      tabGroups: Record<string, { id: string; tabs: SeedTab[]; activeTabId: string | null }>;
      edgePanels: { right: { layout: unknown } };
    };
    base.tabGroups['edge-right-b'] = {
      id: 'edge-right-b',
      tabs: [{ id: 'tab-right-b', title: 'B', contentType: 'tool' }],
      activeTabId: 'tab-right-b',
    };
    base.edgePanels.right.layout = {
      id: 'right-split',
      type: 'split',
      direction: 'vertical',
      splitRatio: 0.3,
      first: paneLayout('right-pane-a', 'gone-group'),
      second: paneLayout('right-pane-b', 'edge-right-b'),
    };

    const restored = deserializeLayout(base as never);
    expect(types(restored.tabGroups['edge-right-b'])).toEqual(['tool', 'files']);
  });

  it('does not re-add a tab the user closed after migrating', () => {
    // Why the split rides the VERSION chain: run it on every load and closing
    // Files is undone at the next startup.
    const migrated = deserializeLayout(v5Layout({ leftTabs: [navigatorTab] }));
    expect(types(migrated.tabGroups['edge-right-group'])).toContain('files');

    const afterClose = deserializeLayout(
      v5Layout({
        version: 6,
        leftTabs: [{ id: 'sessions-tab', title: 'Sessions', contentType: 'sessions' }],
      }),
    );
    expect(types(afterClose.tabGroups['edge-right-group'])).not.toContain('files');
  });
});
