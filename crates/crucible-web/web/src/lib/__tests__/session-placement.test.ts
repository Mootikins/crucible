// Sessions open in the RIGHT pane of the center tiling (the center stays the
// editing surface). Pins the placement rules of openSessionInChat/sessionPane.
import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore } from '@/stores/windowStore';
import type { LayoutNode, TabGroup } from '@/types/windowTypes';
import { openSessionInChat, sessionPane } from '../session-actions';

function resetLayout(layout: LayoutNode, tabGroups: Record<string, TabGroup>) {
  setStore(
    produce((s) => {
      s.layout = layout;
      s.tabGroups = tabGroups;
      s.activePaneId = null;
    })
  );
}

const singlePane = (): [LayoutNode, Record<string, TabGroup>] => [
  { id: 'pane-1', type: 'pane', tabGroupId: 'g-editor' },
  {
    'g-editor': {
      id: 'g-editor',
      tabs: [{ id: 'tab-file-a', title: 'a.md', contentType: 'file' }],
      activeTabId: 'tab-file-a',
    },
  },
];

describe('session placement (right pane)', () => {
  beforeEach(() => resetLayout(...singlePane()));

  it('splits a single pane and puts the chat tab on the RIGHT', () => {
    openSessionInChat('s1', 'My Session');

    const layout = windowStore.layout;
    expect(layout.type).toBe('split');
    if (layout.type !== 'split') return;
    expect(layout.direction).toBe('horizontal');

    // editor tabs stayed in the left pane
    const leftGroup = windowStore.tabGroups[(layout.first as { tabGroupId: string }).tabGroupId];
    expect(leftGroup.tabs.map((t) => t.id)).toEqual(['tab-file-a']);

    // chat tab landed in the right pane and is active there
    const rightGroup = windowStore.tabGroups[(layout.second as { tabGroupId: string }).tabGroupId];
    expect(rightGroup.tabs.map((t) => t.contentType)).toEqual(['chat']);
    expect(rightGroup.activeTabId).toBe('tab-chat-s1');
  });

  it('reuses the existing right pane instead of splitting again', () => {
    openSessionInChat('s1', 'One');
    const before = windowStore.layout;
    openSessionInChat('s2', 'Two');

    // no additional split — same root node shape
    expect(windowStore.layout).toBe(before);
    const layout = windowStore.layout;
    if (layout.type !== 'split') throw new Error('expected split');
    const rightGroup = windowStore.tabGroups[(layout.second as { tabGroupId: string }).tabGroupId];
    expect(rightGroup.tabs.map((t) => t.id)).toEqual(['tab-chat-s1', 'tab-chat-s2']);
  });

  it('focuses an existing session tab without touching the layout', () => {
    openSessionInChat('s1', 'One');
    const layoutAfterFirst = windowStore.layout;
    openSessionInChat('s1', 'One again');
    expect(windowStore.layout).toBe(layoutAfterFirst);
    const layout = windowStore.layout;
    if (layout.type !== 'split') throw new Error('expected split');
    const rightGroup = windowStore.tabGroups[(layout.second as { tabGroupId: string }).tabGroupId];
    expect(rightGroup.tabs).toHaveLength(1);
  });

  it('sessionPane is stable once a horizontal split exists', () => {
    const first = sessionPane();
    const second = sessionPane();
    expect(first?.groupId).toBe(second?.groupId);
  });
});
