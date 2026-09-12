import { describe, it, expect, vi, beforeEach } from 'vitest';

const device = vi.hoisted(() => ({ compact: true }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));
vi.mock('@/lib/recent-files', () => ({ recordRecentFile: vi.fn(), recentFiles: () => [] }));

import { openFileInEditor, openFileAtLine, closeTabsUnder } from '@/lib/file-actions';
import { openSessionInChat } from '@/lib/session-actions';
import { openDraftSession, closeDraftTab } from '@/lib/draft-session';
import { tabStack, tabStackActions } from '@/stores/tabStackStore';
import { windowStore } from '@/stores/windowStore';
import { currentOpenFilePath } from '@/components/files/file-tree-a11y';

const ids = () => tabStack.tabs.map((t) => t.id);
const desktopHolds = (id: string) =>
  Object.values(windowStore.tabGroups).some((g) => g.tabs.some((t) => t.id === id));

beforeEach(() => {
  localStorage.clear();
  tabStackActions.reset();
  device.compact = true;
});

/** Every shared entry point must land in the phone's stack, not the desktop's. */
describe('the shared openers on the compact shell', () => {
  it('opens a file, and focuses it rather than opening it twice', () => {
    openFileInEditor('/kiln/a.md', 'A');
    openFileInEditor('/kiln/a.md', 'A');
    expect(ids()).toEqual(['tab-file-/kiln/a.md']);
    expect(desktopHolds('tab-file-/kiln/a.md')).toBe(false);
  });

  it('opens a file at a line, and scrolls one already open', () => {
    openFileAtLine('/kiln/b.md', 12);
    expect(tabStack.tabs[0].metadata).toMatchObject({ scrollToLine: 12 });
    openFileAtLine('/kiln/b.md', 40);
    expect(ids()).toHaveLength(1);
    expect(tabStack.tabs[0].metadata).toMatchObject({ scrollToLine: 40 });
  });

  it('opens a session, and focuses an open one', () => {
    openSessionInChat('s1', 'First');
    openSessionInChat('s1', 'First');
    expect(ids()).toEqual(['tab-chat-s1']);
  });

  it('keeps one draft, and retargets it at another project', () => {
    openDraftSession({ workspace: '/work/one' });
    const draftId = ids()[0];
    openDraftSession({ workspace: '/work/two' });
    expect(ids()).toEqual([draftId]);
    expect(tabStack.tabs[0].metadata).toMatchObject({ workspace: '/work/two' });
    closeDraftTab(draftId);
    expect(ids()).toEqual([]);
  });

  // The tree highlights the file on screen; on a phone that is the active tab.
  it('tells the file tree which file is on screen', () => {
    expect(currentOpenFilePath()).toBeNull();
    openFileInEditor('/kiln/c.md');
    expect(currentOpenFilePath()).toBe('/kiln/c.md');
  });

  it('closes the tabs under a deleted folder', () => {
    openFileInEditor('/kiln/notes/x.md');
    openFileInEditor('/kiln/other.md');
    closeTabsUnder('/kiln/notes', true);
    expect(ids()).toEqual(['tab-file-/kiln/other.md']);
  });
});
