import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore, windowActions } from '@/stores/windowStore';
import { findTabByFilePath, openFileInEditor } from '../file-actions';
import type { Tab, EdgePanelPosition, TabGroup, LayoutNode } from '@/types/windowTypes';

// -- Helpers (same pattern as windowStore.reorder.test.ts) ----------------

function resetToState(overrides: Partial<{
  tabGroups: Record<string, TabGroup>;
  edgePanels: Record<EdgePanelPosition, {
    id: string;
    tabGroupId: string;
    isCollapsed: boolean;
    width?: number;
    height?: number;
  }>;
  layout: LayoutNode;
  activePaneId: string | null;
  focusedRegion: 'left' | 'right' | 'bottom' | 'center';
}>) {
  setStore(
    produce((s) => {
      if (overrides.tabGroups !== undefined) s.tabGroups = overrides.tabGroups;
      if (overrides.edgePanels !== undefined) s.edgePanels = overrides.edgePanels as any;
      if (overrides.layout !== undefined) s.layout = overrides.layout;
      if (overrides.activePaneId !== undefined) s.activePaneId = overrides.activePaneId;
      if (overrides.focusedRegion !== undefined) s.focusedRegion = overrides.focusedRegion;
      s.dragState = null;
      s.flyoutState = null;
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

const makeEdgePanel = (position: EdgePanelPosition, tabGroupId: string, isCollapsed = false) => ({
  id: `${position}-panel`,
  tabGroupId,
  isCollapsed,
  ...(position === 'bottom' ? { height: 200 } : { width: 250 }),
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
      bottom: makeEdgePanel('bottom', 'bottom-group'),
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
