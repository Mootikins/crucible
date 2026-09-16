import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore } from '@/stores/windowStore';
import { findTabByFilePath, openFileInEditor } from '../file-actions';
import type { Tab, EdgeMode, EdgePanelPosition, TabGroup, LayoutNode } from '@/types/windowTypes';

// -- Helpers (same pattern as windowStore.reorder.test.ts) ----------------

function resetToState(overrides: Partial<{
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, {
    id: string;
    layout: LayoutNode;
    mode: EdgeMode;
    width?: number;
    height?: number;
  }>;
  layout: LayoutNode;
  activePaneId: string | null;
  focusedRegion: 'left' | 'right' | 'center';
}>) {
  setStore(
    produce((s) => {
      if (overrides.tabGroups !== undefined) s.tabGroups = overrides.tabGroups;
      if (overrides.edgePanels !== undefined) s.edgePanels = overrides.edgePanels as any;
      if (overrides.layout !== undefined) s.layout = overrides.layout;
      if (overrides.activePaneId !== undefined) s.activePaneId = overrides.activePaneId;
      if (overrides.focusedRegion !== undefined) s.focusedRegion = overrides.focusedRegion;
    })
  );
}

const makeTab = (id: string, title = id, contentType: Tab['contentType'] = 'file', metadata?: Record<string, unknown>): Tab => ({
  id,
  title,
  contentType,
  ...(metadata ? { metadata } : {}),
});

const makeTabGroup = (id: string, tabs: Tab[], activeTabId: string | null = tabs[0]?.id ?? null): TabGroup => ({
  id,
  tabs,
  activeTabId,
});

const makeEdgePanel = (position: EdgePanelPosition, tabGroupId: string, collapsed = false) => ({
  id: `${position}-panel`,
  layout: { id: `${position}-pane`, type: 'pane' as const, tabGroupId },
  mode: collapsed ? ('strip' as const) : ('docked' as const),
  width: 250,
});

const simpleLayout = (paneId: string, groupId: string): LayoutNode => ({
  id: paneId,
  type: 'pane' as const,
  tabGroupId: groupId,
});

function setupDefaultState(extraTabs: Tab[] = []) {
  resetToState({
    tabGroups: {
      'center-group': makeTabGroup('center-group', extraTabs),
      'left-group': makeTabGroup('left-group', []),
      'right-group': makeTabGroup('right-group', []),
      'bottom-group': makeTabGroup('bottom-group', []),
    },
    edgePanels: {
      left: makeEdgePanel('left', 'left-group'),
      right: makeEdgePanel('right', 'right-group'),
    },
    layout: simpleLayout('pane-1', 'center-group'),
    activePaneId: 'pane-1',
    focusedRegion: 'center',
  });
}

// -------------------------------------------------------------------------

describe('findTabByFilePath', () => {
  beforeEach(() => {
    setupDefaultState();
  });

  it('returns null when no tabs exist', () => {
    expect(findTabByFilePath('/docs/readme.md')).toBeNull();
  });

  it('returns null when filePath does not match any tab', () => {
    setupDefaultState([
      makeTab('tab-file-a', 'a.md', 'file', { filePath: '/docs/a.md' }),
    ]);
    expect(findTabByFilePath('/docs/not-found.md')).toBeNull();
  });

  it('finds tab by metadata.filePath', () => {
    setupDefaultState([
      makeTab('tab-file-a', 'a.md', 'file', { filePath: '/docs/a.md' }),
      makeTab('tab-file-b', 'b.md', 'file', { filePath: '/docs/b.md' }),
    ]);

    const result = findTabByFilePath('/docs/b.md');
    expect(result).not.toBeNull();
    expect(result!.groupId).toBe('center-group');
    expect(result!.tab.id).toBe('tab-file-b');
    expect(result!.tab.metadata?.filePath).toBe('/docs/b.md');
  });

  it('ignores tabs without metadata', () => {
    setupDefaultState([
      makeTab('tab-plain', 'plain', 'tool'),
      makeTab('tab-file-a', 'a.md', 'file', { filePath: '/docs/a.md' }),
    ]);

    const result = findTabByFilePath('/docs/a.md');
    expect(result).not.toBeNull();
    expect(result!.tab.id).toBe('tab-file-a');
  });
});

describe('openFileInEditor', () => {
  beforeEach(() => {
    setupDefaultState();
  });

  it('creates a new tab with correct contentType and metadata', () => {
    openFileInEditor('/docs/readme.md', 'readme.md');

    const group = windowStore.tabGroups['center-group']!;
    expect(group.tabs).toHaveLength(1);

    const tab = group.tabs[0]!;
    expect(tab.id).toBe('tab-file-/docs/readme.md');
    expect(tab.title).toBe('readme.md');
    expect(tab.contentType).toBe('file');
    expect(tab.metadata).toEqual({ filePath: '/docs/readme.md' });
  });

  it('deduplicates: same filePath activates existing tab instead of creating new', () => {
    openFileInEditor('/docs/readme.md', 'readme.md');
    openFileInEditor('/docs/readme.md', 'readme.md');

    const group = windowStore.tabGroups['center-group']!;
    expect(group.tabs).toHaveLength(1);
    expect(group.activeTabId).toBe('tab-file-/docs/readme.md');
  });

  it('creates separate tabs for different filePaths', () => {
    openFileInEditor('/docs/a.md', 'a.md');
    openFileInEditor('/docs/b.md', 'b.md');

    const group = windowStore.tabGroups['center-group']!;
    expect(group.tabs).toHaveLength(2);
    expect(group.tabs[0]!.metadata?.filePath).toBe('/docs/a.md');
    expect(group.tabs[1]!.metadata?.filePath).toBe('/docs/b.md');
  });

  it('activates existing tab on duplicate open', () => {
    // Open two files, then re-open first
    openFileInEditor('/docs/a.md', 'a.md');
    openFileInEditor('/docs/b.md', 'b.md');

    const groupBefore = windowStore.tabGroups['center-group']!;
    expect(groupBefore.activeTabId).toBe('tab-file-/docs/b.md');

    openFileInEditor('/docs/a.md', 'a.md');

    const groupAfter = windowStore.tabGroups['center-group']!;
    expect(groupAfter.tabs).toHaveLength(2);
    expect(groupAfter.activeTabId).toBe('tab-file-/docs/a.md');
  });
});

describe('openFileInEditor — beside the conversation, never on top of it', () => {
  /** Centre split: a chat pane on the LEFT, the editor on the right — the
   * arrangement a session creates when it opens. */
  const centreWithChat = () =>
    setStore(
      produce((s) => {
        s.layout = {
          id: 'root',
          type: 'split',
          direction: 'horizontal',
          splitRatio: 0.4,
          first: { id: 'pane-chat', type: 'pane', tabGroupId: 'g-chat' },
          second: { id: 'pane-editor', type: 'pane', tabGroupId: 'g-editor' },
        };
        s.tabGroups = {
          'g-chat': {
            id: 'g-chat',
            tabs: [
              { id: 'tab-chat-s1', title: 'One', contentType: 'chat', metadata: { sessionId: 's1' } },
            ],
            activeTabId: 'tab-chat-s1',
          },
          'g-editor': {
            id: 'g-editor',
            tabs: [{ id: 'tab-file-a', title: 'a.md', contentType: 'file', metadata: { filePath: '/a.md' } }],
            activeTabId: 'tab-file-a',
          },
        };
        s.activePaneId = null;
      }),
    );

  it('opens into the editor pane, not the first leaf', () => {
    centreWithChat();
    openFileInEditor('/b.md', 'b.md');

    // The chat pane is the FIRST centre leaf now, so "first" was the wrong
    // rule: a file opened from the Files rail landed on top of the session.
    expect(windowStore.tabGroups['g-chat'].tabs.map((t) => t.id)).toEqual(['tab-chat-s1']);
    expect(windowStore.tabGroups['g-editor'].tabs.map((t) => t.id)).toContain('tab-file-/b.md');
  });

  it('reuses an empty pane rather than stacking onto the conversation', () => {
    centreWithChat();
    setStore(produce((s) => {
      s.tabGroups['g-editor'] = { id: 'g-editor', tabs: [], activeTabId: null };
    }));
    openFileInEditor('/b.md', 'b.md');
    expect(windowStore.tabGroups['g-editor'].tabs).toHaveLength(1);
    expect(windowStore.tabGroups['g-chat'].tabs).toHaveLength(1);
  });

  it('opens its own pane on the files side when the centre holds only conversations', () => {
    setStore(
      produce((s) => {
        s.layout = { id: 'pane-chat', type: 'pane', tabGroupId: 'g-chat' };
        s.tabGroups = {
          'g-chat': {
            id: 'g-chat',
            tabs: [{ id: 'tab-chat-s1', title: 'One', contentType: 'chat' }],
            activeTabId: 'tab-chat-s1',
          },
        };
      }),
    );
    openFileInEditor('/b.md', 'b.md');
    // A file never lands on a chat. It gets a new pane on the files side
    // (right by default) of the conversation, and the chat keeps its one tab.
    expect(windowStore.tabGroups['g-chat'].tabs.map((t) => t.id)).toEqual(['tab-chat-s1']);
    const holder = Object.values(windowStore.tabGroups).find((g) =>
      g.tabs.some((t) => t.id === 'tab-file-/b.md'),
    );
    expect(holder).toBeDefined();
    expect(windowStore.layout.type).toBe('split');
    if (windowStore.layout.type === 'split') {
      expect(windowStore.layout.direction).toBe('horizontal');
      expect(windowStore.layout.second.type === 'pane' && windowStore.layout.second.tabGroupId).toBe(holder!.id);
    }
  });
});
