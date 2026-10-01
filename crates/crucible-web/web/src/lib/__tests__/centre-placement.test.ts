import { describe, it, expect, vi, beforeEach } from 'vitest';

const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));
vi.mock('@/lib/recent-files', () => ({ recordRecentFile: vi.fn(), recentFiles: () => [] }));

import { openFileInEditor } from '@/lib/file-actions';
import { openSessionInChat } from '@/lib/session-actions';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { collectLeafGroupIds, edgeLeaf } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';
import type { EdgePanelPosition } from '@/windowing/model/types';

/** The centre leaf at one edge of the live layout. */
const edgeCenterPane = (side: EdgePanelPosition) => edgeLeaf(windowStore.layout, side);

const groupOf = (tabId: string) =>
  Object.values(windowStore.tabGroups).find((g) => g.tabs.some((t) => t.id === tabId))?.id ?? null;

beforeEach(() => {
  setStore(defaultLayout());
  device.compact = false;
});

// The centre reads sessions | editor, with each next to its own rail.
describe('centre placement follows the rails', () => {
  it('opens a session in the conversation rail', () => {
    openSessionInChat('s1', 'One');
    expect(collectLeafGroupIds(windowStore.edgePanels.right.layout)).toContain(groupOf('tab-chat-s1'));
  });

  it('reserves the centre for documents', () => {
    // A fresh shell has ONE empty centre pane, and that pane already IS the
    // pane next to the sessions rail. A split there would draw an empty
    // editor pane beside the chat; the editor pane appears when a file opens.
    openSessionInChat('s1', 'One');
    expect(collectLeafGroupIds(windowStore.layout)).toHaveLength(1);
    expect(collectLeafGroupIds(windowStore.edgePanels.right.layout)).toContain(groupOf('tab-chat-s1'));

    openFileInEditor('/k/Note.md');
    expect(collectLeafGroupIds(windowStore.layout)).toHaveLength(1);
  });

  it('opens a file in the pane next to the files rail, never on the chat', () => {
    openSessionInChat('s1', 'One');
    openFileInEditor('/k/Note.md');
    const fileGroup = groupOf('tab-file-/k/Note.md') ?? groupOf(Object.values(windowStore.tabGroups).flatMap((g) => g.tabs).find((t) => t.contentType === 'file')!.id);
    expect(fileGroup).toBe(edgeCenterPane('right').groupId);
    expect(fileGroup).not.toBe(groupOf('tab-chat-s1'));
  });

  it('after a swap, the conversation rail moves to the left', () => {
    // An editor in the centre first, so the two edges are different panes.
    openFileInEditor('/k/Note.md');
    windowActions.swapSidePanels();
    openSessionInChat('s2', 'Two');
    expect(collectLeafGroupIds(windowStore.edgePanels.left.layout)).toContain(groupOf('tab-chat-s2'));
    expect(groupOf('tab-chat-s2')).not.toBe(edgeCenterPane('left').groupId);
  });
});

describe('reopening a session preserves its user placement', () => {
  it('activates a session moved to the editor pane', () => {
    openFileInEditor('/k/Note.md');
    openSessionInChat('s1', 'One');
    const home = groupOf('tab-chat-s1')!;
    // Drag it onto the editor pane (the files side).
    const editorGroup = edgeCenterPane('right').groupId!;
    windowActions.moveTab(home, editorGroup, 'tab-chat-s1');
    expect(groupOf('tab-chat-s1')).toBe(editorGroup);

    openSessionInChat('s1', 'One');
    const back = groupOf('tab-chat-s1')!;
    expect(back).toBe(editorGroup);
    expect(windowStore.tabGroups[back].activeTabId).toBe('tab-chat-s1');
    // Still exactly one tab for the session.
    const count = Object.values(windowStore.tabGroups).flatMap((g) => g.tabs).filter((t) => t.id === 'tab-chat-s1').length;
    expect(count).toBe(1);
  });

  it('activates a session moved to the navigation rail', () => {
    openFileInEditor('/k/Note.md');
    openSessionInChat('s1', 'One');
    const home = groupOf('tab-chat-s1')!;
    const rail = Object.keys(windowStore.tabGroups).find((id) =>
      windowStore.tabGroups[id].tabs.some((t) => t.contentType === 'sessions'),
    )!;
    windowActions.moveTab(home, rail, 'tab-chat-s1');
    expect(groupOf('tab-chat-s1')).toBe(rail);

    openSessionInChat('s1', 'One');
    const back = groupOf('tab-chat-s1')!;
    expect(back).toBe(rail);
    expect(windowStore.tabGroups[back].activeTabId).toBe('tab-chat-s1');
    expect(windowStore.tabGroups[back].tabs.some((t) => t.contentType === 'file')).toBe(false);
  });
});

describe('a stacked centre has no left or right', () => {
  it('treats the top pane of a vertical split as both edges', () => {
    openFileInEditor('/k/Note.md');
    const pane = edgeCenterPane('left');
    // Split the only pane top/bottom; the tree now has two panes but one column.
    windowActions.openTabInNewPane(pane.paneId, 'bottom', { id: 'tab-file-/k/Other.md', title: 'Other', contentType: 'file' });
    expect(edgeCenterPane('left').paneId).toBe(edgeCenterPane('right').paneId);
    openSessionInChat('s3', 'Three');
    // The session still gets its own pane on the sessions side of that column.
    expect(windowStore.tabGroups[groupOf('tab-chat-s3')!].tabs.every((t) => t.contentType === 'chat')).toBe(true);
  });
});
