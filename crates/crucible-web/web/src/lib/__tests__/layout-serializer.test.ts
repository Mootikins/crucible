import { describe, it, expect } from 'vitest';
import { serializeLayout, deserializeLayout } from '../layout-serializer';
import type { WindowManagerState } from '@/types/windowTypes';

function createTestState(): WindowManagerState {
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
    dragState: null,
    flyoutState: null,
    nextZIndex: 1,
  };
}

describe('layout-serializer', () => {
  it('round-trip serialization preserves state', () => {
    const state = createTestState();
    const serialized = serializeLayout(state);
    const deserialized = deserializeLayout(serialized);

    expect(serialized.version).toBe(2);

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

  it('v2 format is preserved on round-trip', () => {
    const state = createTestState();
    const serialized1 = serializeLayout(state);
    const deserialized1 = deserializeLayout(serialized1);
    const serialized2 = serializeLayout(deserialized1);

    expect(serialized2.version).toBe(2);
    expect(serialized2.edgePanels.left.tabGroupId).toBe(serialized1.edgePanels.left.tabGroupId);
    expect((serialized2.edgePanels.left as any).tabs).toBeUndefined();
    expect((serialized2.edgePanels.left as any).position).toBeUndefined();
  });
});
