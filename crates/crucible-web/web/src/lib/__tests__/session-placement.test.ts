// Sessions dock in the RIGHT EDGE PANEL (the collapsible sidebar) — the
// center tiling stays the editing surface. Pins the placement rules of
// openSessionInChat/sessionPane.
import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore } from '@/stores/windowStore';
import type { LayoutNode, TabGroup } from '@/types/windowTypes';
import { openSessionInChat, sessionPane } from '../session-actions';

function resetLayout(
  layout: LayoutNode,
  tabGroups: Record<string, TabGroup>,
  rightGroupId: string | null = 'g-right',
) {
  setStore(
    produce((s) => {
      s.layout = layout;
      s.tabGroups = tabGroups;
      s.activePaneId = null;
      s.edgePanels.right.tabGroupId = rightGroupId as string;
      s.edgePanels.right.isCollapsed = true;
    })
  );
}

const baseState = (): [LayoutNode, Record<string, TabGroup>] => [
  { id: 'pane-1', type: 'pane', tabGroupId: 'g-editor' },
  {
    'g-editor': {
      id: 'g-editor',
      tabs: [{ id: 'tab-file-a', title: 'a.md', contentType: 'file' }],
      activeTabId: 'tab-file-a',
    },
    'g-right': { id: 'g-right', tabs: [], activeTabId: null },
  },
];

describe('session placement (right edge panel)', () => {
  beforeEach(() => resetLayout(...baseState()));

  it('docks a new session in the right edge panel, expanded and active', () => {
    openSessionInChat('s1', 'My Session');

    // The center tiling is untouched — no split, editor tabs in place.
    expect(windowStore.layout.type).toBe('pane');
    expect(windowStore.tabGroups['g-editor'].tabs.map((t) => t.id)).toEqual(['tab-file-a']);

    const right = windowStore.tabGroups['g-right'];
    expect(right.tabs.map((t) => t.id)).toEqual(['tab-chat-s1']);
    expect(right.activeTabId).toBe('tab-chat-s1');
    expect(windowStore.edgePanels.right.isCollapsed).toBe(false);
  });

  it('stacks further sessions in the same right panel group', () => {
    openSessionInChat('s1', 'One');
    openSessionInChat('s2', 'Two');
    expect(windowStore.tabGroups['g-right'].tabs.map((t) => t.id)).toEqual([
      'tab-chat-s1',
      'tab-chat-s2',
    ]);
    expect(windowStore.tabGroups['g-right'].activeTabId).toBe('tab-chat-s2');
  });

  it('re-opening a session focuses its tab and re-expands the panel', () => {
    openSessionInChat('s1', 'One');
    setStore(produce((s) => { s.edgePanels.right.isCollapsed = true; }));
    openSessionInChat('s1', 'One again');
    expect(windowStore.tabGroups['g-right'].tabs).toHaveLength(1);
    expect(windowStore.edgePanels.right.isCollapsed).toBe(false);
  });

  it('a session tab the user moved to a center pane focuses in place', () => {
    resetLayout(
      { id: 'pane-1', type: 'pane', tabGroupId: 'g-editor' },
      {
        'g-editor': {
          id: 'g-editor',
          tabs: [
            { id: 'tab-file-a', title: 'a.md', contentType: 'file' },
            { id: 'tab-chat-s1', title: 'One', contentType: 'chat', metadata: { sessionId: 's1' } },
          ],
          activeTabId: 'tab-file-a',
        },
        'g-right': { id: 'g-right', tabs: [], activeTabId: null },
      },
    );
    openSessionInChat('s1', 'One');
    // Focused where the user put it; NOT re-docked right.
    expect(windowStore.tabGroups['g-editor'].activeTabId).toBe('tab-chat-s1');
    expect(windowStore.tabGroups['g-right'].tabs).toHaveLength(0);
  });

  it('sessionPane reports the right edge panel group', () => {
    expect(sessionPane()?.groupId).toBe('g-right');
  });

  it('falls back to the first center group when the layout has no right group', () => {
    const [layout, groups] = baseState();
    delete groups['g-right'];
    resetLayout(layout, groups, null);
    openSessionInChat('s1', 'One');
    expect(windowStore.tabGroups['g-editor'].tabs.some((t) => t.id === 'tab-chat-s1')).toBe(true);
  });
});
