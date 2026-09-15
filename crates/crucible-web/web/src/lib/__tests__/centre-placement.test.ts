import { describe, it, expect, vi, beforeEach } from 'vitest';

const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));
vi.mock('@/lib/recent-files', () => ({ recordRecentFile: vi.fn(), recentFiles: () => [] }));

import { openFileInEditor } from '@/lib/file-actions';
import { openSessionInChat } from '@/lib/session-actions';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState } from '@/stores/windowStoreInternals';
import { edgeCenterPane } from '@/lib/panel-actions';

const groupOf = (tabId: string) =>
  Object.values(windowStore.tabGroups).find((g) => g.tabs.some((t) => t.id === tabId))?.id ?? null;

beforeEach(() => {
  setStore(createInitialState());
  device.compact = false;
});

// The centre reads sessions | editor, with each next to its own rail.
describe('centre placement follows the rails', () => {
  it('opens a session in the pane next to the sessions rail (left by default)', () => {
    openSessionInChat('s1', 'One');
    expect(groupOf('tab-chat-s1')).toBe(edgeCenterPane('left')!.groupId);
  });

  it('opens a file in the pane next to the files rail, never on the chat', () => {
    openSessionInChat('s1', 'One');
    openFileInEditor('/k/Note.md');
    const fileGroup = groupOf('tab-file-/k/Note.md') ?? groupOf(Object.values(windowStore.tabGroups).flatMap((g) => g.tabs).find((t) => t.contentType === 'file')!.id);
    expect(fileGroup).toBe(edgeCenterPane('right')!.groupId);
    expect(fileGroup).not.toBe(groupOf('tab-chat-s1'));
  });

  it('after a swap, a new session opens next to the sessions rail on the RIGHT', () => {
    // An editor in the centre first, so the two edges are different panes.
    openFileInEditor('/k/Note.md');
    windowActions.swapSidePanels();
    openSessionInChat('s2', 'Two');
    expect(groupOf('tab-chat-s2')).toBe(edgeCenterPane('right')!.groupId);
    expect(groupOf('tab-chat-s2')).not.toBe(edgeCenterPane('left')!.groupId);
  });
});

describe('reopening a session brings it back beside the sessions rail', () => {
  it('moves a session tab out of another centre pane', () => {
    openFileInEditor('/k/Note.md');
    openSessionInChat('s1', 'One');
    const home = groupOf('tab-chat-s1')!;
    // Drag it onto the editor pane (the files side).
    const editorGroup = edgeCenterPane('right')!.groupId!;
    windowActions.moveTab(home, editorGroup, 'tab-chat-s1');
    expect(groupOf('tab-chat-s1')).toBe(editorGroup);

    openSessionInChat('s1', 'One');
    const back = groupOf('tab-chat-s1')!;
    expect(back).toBe(edgeCenterPane('left')!.groupId);
    // Its own pane again: no file shares it, and the editor sits to its right.
    expect(windowStore.tabGroups[back].tabs.some((t) => t.contentType === 'file')).toBe(false);
    expect(edgeCenterPane('right')!.groupId).not.toBe(back);
    // Still exactly one tab for the session.
    const count = Object.values(windowStore.tabGroups).flatMap((g) => g.tabs).filter((t) => t.id === 'tab-chat-s1').length;
    expect(count).toBe(1);
  });

  it('moves a session tab out of a rail', () => {
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
    expect(back).toBe(edgeCenterPane('left')!.groupId);
    expect(back).not.toBe(rail);
    expect(windowStore.tabGroups[back].tabs.some((t) => t.contentType === 'file')).toBe(false);
  });
});

describe('a stacked centre has no left or right', () => {
  it('treats the top pane of a vertical split as both edges', () => {
    openFileInEditor('/k/Note.md');
    const pane = edgeCenterPane('left')!;
    // Split the only pane top/bottom; the tree now has two panes but one column.
    windowActions.openTabInNewPane(pane.paneId, 'bottom', { id: 'tab-file-/k/Other.md', title: 'Other', contentType: 'file' });
    expect(edgeCenterPane('left')!.paneId).toBe(edgeCenterPane('right')!.paneId);
    openSessionInChat('s3', 'Three');
    // The session still gets its own pane on the sessions side of that column.
    expect(windowStore.tabGroups[groupOf('tab-chat-s3')!].tabs.every((t) => t.contentType === 'chat')).toBe(true);
  });
});
