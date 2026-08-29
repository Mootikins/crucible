// A session is a PEER OF THE EDITOR: it opens as its own pane in the centre
// tiling, to the LEFT of the editor, sharing the main area with it. Pins the
// placement rules of openSessionInChat / sessionPane / openTabBesideEditor.
//
// It used to dock in an edge panel — first the right rail (where it covered
// the file tree), then the left (where it covered the session list). Cursor's
// agents window is the model: a nav rail with the session list, then the
// conversation and the editor side by side, then the file tree beyond the
// editor. A conversation has a composer and needs a working surface's width,
// which is what a rail cannot give it.
import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import type { LayoutNode, TabGroup } from '@/types/windowTypes';
import { openSessionInChat, sessionPane } from '../session-actions';
import { collectLeafGroupIds } from '@/stores/windowStoreInternals';

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

describe('session placement (a pane beside the editor)', () => {
  beforeEach(() => resetLayout());

  it('splits the centre and puts the session LEFT of the editor', () => {
    openSessionInChat('s1', 'My Session');

    expect(windowStore.layout.type).toBe('split');
    const [firstGroup, secondGroup] = centreGroups();
    // Left of the editor: the conversation is what you read and steer from,
    // and the file it changes sits to its right.
    expect(windowStore.tabGroups[firstGroup].tabs.map((t) => t.id)).toEqual(['tab-chat-s1']);
    expect(windowStore.tabGroups[secondGroup].tabs.map((t) => t.id)).toEqual(['tab-file-a']);
  });

  it('leaves both rails alone', () => {
    openSessionInChat('s1', 'My Session');
    // The two defects this arrangement ends: a session used to land in a rail
    // and cover whichever of these was there.
    expect(windowStore.tabGroups['g-files'].tabs.map((t) => t.id)).toEqual(['files-tab']);
    expect(windowStore.tabGroups['g-sessions-list'].tabs.map((t) => t.id)).toEqual(['sessions-tab']);
  });

  it('stacks a second session in the SAME pane, not a third column', () => {
    openSessionInChat('s1', 'One');
    openSessionInChat('s2', 'Two');

    expect(centreGroups()).toHaveLength(2);
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

  it('honours a session the user dragged into the editor pane', () => {
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
    // Focused where the user put it; the centre is NOT split behind their back.
    expect(windowStore.layout.type).toBe('pane');
    expect(windowStore.tabGroups['g-editor'].activeTabId).toBe('tab-chat-s1');
  });

  it('adds to an existing session pane rather than splitting again', () => {
    openSessionInChat('s1', 'One');
    const before = windowStore.layout;
    openSessionInChat('s2', 'Two');
    // Same split, same shape: `sessionPane` found the pane by role.
    expect(centreGroups()).toHaveLength(2);
    expect(windowStore.layout.type).toBe(before.type);
  });

  it('reports no session pane before one exists', () => {
    expect(sessionPane()).toBeNull();
    openSessionInChat('s1', 'One');
    expect(sessionPane()).not.toBeNull();
  });

  it('never mistakes a RAIL for the session pane', () => {
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
