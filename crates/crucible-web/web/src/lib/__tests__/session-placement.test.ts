import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import type { LayoutNode, TabGroup } from '@/types/windowTypes';
import { openSessionInChat, sessionPane } from '../session-actions';
import { collectLeafGroupIds, collectPanes } from '@/windowing/model/tree';

/** Centre tab groups, left to right. */
const centreGroups = () => collectLeafGroupIds(windowStore.layout);

/** The group holding a session tab, or null. */
const groupWithTab = (tabId: string) =>
  Object.values(windowStore.tabGroups).find((g) => g.tabs.some((t) => t.id === tabId)) ?? null;

function resetLayout(layout?: LayoutNode, tabGroups?: Record<string, TabGroup>) {
  setStore(
    produce((s) => {
      s.layout = layout ?? { id: 'pane-editor', type: 'pane', tabGroupId: 'g-editor' };
      s.tabGroups = tabGroups ?? {
        'g-editor': {
          id: 'g-editor',
          tabs: [{ id: 'tab-file-a', title: 'a.md', contentType: 'file' }],
          activeTabId: 'tab-file-a',
        },
        'g-sessions-list': {
          id: 'g-sessions-list',
          tabs: [{ id: 'sessions-tab', title: 'Sessions', contentType: 'sessions' }],
          activeTabId: 'sessions-tab',
        },
        'g-files': {
          id: 'g-files',
          tabs: [{ id: 'files-tab', title: 'Files', contentType: 'files' }],
          activeTabId: 'files-tab',
        },
      };
      s.activePaneId = null;
      // The rails: session LIST on one, file tree on the other. Neither is
      // where a conversation goes.
      s.edgePanels.left.layout = { id: 'left-pane', type: 'pane', tabGroupId: 'g-sessions-list' };
      s.edgePanels.right.layout = { id: 'right-pane', type: 'pane', tabGroupId: 'g-files' };
    })
  );
}

describe('session placement (conversation rail)', () => {
  beforeEach(() => resetLayout());

  it('splits a populated conversation rail and preserves the editor', () => {
    openSessionInChat('s1', 'My Session');

    expect(windowStore.layout.type).toBe('pane');
    const [firstGroup, secondGroup] = collectLeafGroupIds(windowStore.edgePanels.right.layout);
    expect(windowStore.tabGroups[firstGroup].tabs.map((t) => t.id)).toEqual(['tab-chat-s1']);
    expect(windowStore.tabGroups[secondGroup].tabs.map((t) => t.id)).toEqual(['files-tab']);
    expect(windowStore.tabGroups['g-editor'].tabs.map((t) => t.id)).toEqual(['tab-file-a']);
  });

  it('preserves existing navigation tabs', () => {
    openSessionInChat('s1', 'My Session');
    // The two defects this arrangement ends: a session used to land in a rail
    // and cover whichever of these was there.
    expect(windowStore.tabGroups['g-files'].tabs.map((t) => t.id)).toEqual(['files-tab']);
    expect(windowStore.tabGroups['g-sessions-list'].tabs.map((t) => t.id)).toEqual(['sessions-tab']);
  });

  it('stacks a second session in the SAME pane, not a third column', () => {
    openSessionInChat('s1', 'One');
    openSessionInChat('s2', 'Two');

    expect(centreGroups()).toHaveLength(1);
    const pane = groupWithTab('tab-chat-s1')!;
    expect(pane.tabs.map((t) => t.id)).toEqual(['tab-chat-s1', 'tab-chat-s2']);
    expect(pane.activeTabId).toBe('tab-chat-s2');
  });

  it('re-opening a session focuses its tab where it already is', () => {
    openSessionInChat('s1', 'One');
    openSessionInChat('s2', 'Two');
    openSessionInChat('s1', 'One again');

    const pane = groupWithTab('tab-chat-s1')!;
    expect(pane.tabs).toHaveLength(2);
    expect(pane.activeTabId).toBe('tab-chat-s1');
  });

  it('activates a moved session in place without replacing the editor', () => {
    resetLayout(
      { id: 'pane-editor', type: 'pane', tabGroupId: 'g-editor' },
      {
        'g-editor': {
          id: 'g-editor',
          tabs: [
            { id: 'tab-file-a', title: 'a.md', contentType: 'file' },
            { id: 'tab-chat-s1', title: 'One', contentType: 'chat', metadata: { sessionId: 's1' } },
          ],
          activeTabId: 'tab-file-a',
        },
      },
    );

    openSessionInChat('s1', 'One');
    expect(windowStore.layout.type).toBe('pane');
    expect(windowStore.tabGroups['g-editor'].tabs.map((t) => t.id)).toEqual(['tab-file-a', 'tab-chat-s1']);
    expect(windowStore.tabGroups['g-editor'].activeTabId).toBe('tab-chat-s1');
  });

  it('adds to an existing session pane rather than splitting again', () => {
    openSessionInChat('s1', 'One');
    const before = windowStore.layout;
    openSessionInChat('s2', 'Two');
    // Same split, same shape: `sessionPane` found the pane by role.
    expect(centreGroups()).toHaveLength(1);
    expect(windowStore.layout.type).toBe(before.type);
  });

  it('reports no session pane before one exists', () => {
    expect(sessionPane()).toBeNull();
    openSessionInChat('s1', 'One');
    expect(sessionPane()).not.toBeNull();
  });

  it('never mistakes the navigation list for a conversation pane', () => {
    // The session LIST is chrome, not a conversation, and it lives in a rail.
    // Matching on it would send every session back into the sidebar.
    windowActions.addTab('g-sessions-list', {
      id: 'tab-chat-rail',
      title: 'stray',
      contentType: 'chat',
    });
    expect(sessionPane()).toBeNull();
  });
});

it('reuses a conversation grouped with supporting tabs without changing the layout', () => {
  resetLayout();
  openSessionInChat('s1', 'One');
  const group = groupWithTab('tab-chat-s1')!;
  windowActions.addTab(group.id, { id: 'context-backlinks', title: 'Backlinks', contentType: 'backlinks' });
  windowActions.addTab(group.id, { id: 'context-activity', title: 'Activity', contentType: 'activity' });
  windowActions.setActiveTab(group.id, 'tab-chat-s1');
  const before = JSON.stringify(windowStore.edgePanels.right.layout);
  openSessionInChat('s2', 'Two');
  expect(JSON.stringify(windowStore.edgePanels.right.layout)).toBe(before);
  expect(groupWithTab('tab-chat-s2')?.id).toBe(group.id);
  expect(windowStore.tabGroups[group.id].activeTabId).toBe('tab-chat-s2');
  expect(windowStore.tabGroups[group.id].tabs.map(t => t.id)).toEqual(['tab-chat-s1', 'context-backlinks', 'context-activity', 'tab-chat-s2']);
});

it.each(['existing', 'new'])('reveals a folded conversation for an %s session selection', kind => {
  resetLayout();
  openSessionInChat('fold-a', 'First');
  const group = groupWithTab('tab-chat-fold-a')!;
  const pane = collectPanes(windowStore.edgePanels.right.layout).find(p => p.tabGroupId === group.id)!;
  windowActions.setPaneCollapsed(pane.id, true);
  expect(pane.collapsed).toBe(true);
  openSessionInChat(kind === 'existing' ? 'fold-a' : 'fold-b', 'Selected');
  expect(pane.collapsed).toBe(false);
});
