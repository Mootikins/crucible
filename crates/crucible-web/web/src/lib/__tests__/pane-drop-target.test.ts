import { describe, it, expect, vi, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';

type DropConfig = {
  element: HTMLElement;
  onDragEnter: (a: { source: { data: Record<string, unknown> } }) => void;
  onDragLeave: () => void;
  onDrop: (a: {
    source: { data: Record<string, unknown> };
    location: { current: { dropTargets: { element: Element; data: Record<string, unknown> }[]; input: { clientX: number; clientY: number } } };
  }) => void;
};

const captured = vi.hoisted(() => ({ config: null as DropConfig | null }));
vi.mock('@atlaskit/pragmatic-drag-and-drop/element/adapter', () => ({
  draggable: () => () => {},
  dropTargetForElements: (config: DropConfig) => {
    captured.config = config;
    return () => {};
  },
}));

import { attachPaneDropTarget, DROP_OVER_ATTR } from '@/lib/file-dnd';
import { windowStore, setStore } from '@/stores/windowStore';
import { defaultLayout } from '@/stores/defaultLayout';
import { findFirstPane } from '@/windowing/model/tree';

const file = {
  type: 'fileNode',
  rootId: 'kiln:/k',
  rootKind: 'kiln',
  rootPath: '/k',
  relPath: 'notes/a.md',
  absPath: '/k/notes/a.md',
  name: 'a.md',
  isDir: false,
};

const drop = (el: HTMLElement) =>
  captured.config!.onDrop({
    source: { data: file },
    location: { current: { dropTargets: [{ element: el, data: { zone: 'pane' } }], input: { clientX: 0, clientY: 0 } } },
  });

beforeEach(() => {
  captured.config = null;
  const fresh = defaultLayout();
  setStore(produce((s) => Object.assign(s, fresh, { activePaneId: null })));
});

describe('attachPaneDropTarget', () => {
  it('marks the element while a file hovers it', () => {
    const el = document.createElement('div');
    attachPaneDropTarget(el, () => null);
    captured.config!.onDragEnter({ source: { data: file } });
    expect(el.hasAttribute(DROP_OVER_ATTR)).toBe(true);
    captured.config!.onDragLeave();
    expect(el.hasAttribute(DROP_OVER_ATTR)).toBe(false);
  });

  it('opens a file dropped on a closed rail in that rail, and opens the rail', () => {
    setStore(produce((s) => { s.edgePanels.left.mode = 'strip'; }));
    const railPane = findFirstPane(windowStore.edgePanels.left.layout)!;
    const el = document.createElement('div');
    attachPaneDropTarget(el, () => railPane.tabGroupId);
    drop(el);
    const group = windowStore.tabGroups[railPane.tabGroupId!]!;
    expect(group.tabs.find((t) => t.id === group.activeTabId)?.metadata?.filePath).toBe('/k/notes/a.md');
    expect(windowStore.edgePanels.left.mode).toBe('docked');
    expect(windowStore.activePaneId).toBe(railPane.id);
  });

  it('focuses the centre pane that takes the file', () => {
    const pane = findFirstPane(windowStore.layout)!;
    const el = document.createElement('div');
    attachPaneDropTarget(el, () => pane.tabGroupId);
    drop(el);
    expect(windowStore.activePaneId).toBe(pane.id);
    expect(windowStore.tabGroups[pane.tabGroupId!]!.tabs.map((t) => t.metadata?.filePath)).toContain('/k/notes/a.md');
  });
});

describe('attachPaneDropTarget with no group', () => {
  it('does nothing, even when a tab already shows the file', () => {
    const pane = findFirstPane(windowStore.layout)!;
    const groupId = pane.tabGroupId!;
    setStore(
      produce((s) => {
        s.tabGroups[groupId]!.tabs = [
          { id: 'open-file', title: 'a.md', contentType: 'file', metadata: { filePath: file.absPath } },
          { id: 'other', title: 'Other', contentType: 'file' },
        ];
        s.tabGroups[groupId]!.activeTabId = 'other';
        s.edgePanels.left.mode = 'strip';
      }),
    );
    const before = JSON.stringify(windowStore);
    const el = document.createElement('div');
    attachPaneDropTarget(el, () => null);
    drop(el);
    // A rail with no group has no place for the file, so the drop changes nothing.
    expect(JSON.stringify(windowStore)).toBe(before);
  });
});
