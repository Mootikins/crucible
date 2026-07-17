import { it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider, useDragDropContext } from '@thisbeyond/solid-dnd';
import { TabBar } from '../TabBar';
import { EdgePanel } from '../EdgePanel';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane, generateId } from '@/stores/windowStoreInternals';
import type { DragSource } from '@/types/windowTypes';

// Regression: updateTab replaces the tab OBJECT on every write (dirty flag,
// title). Rows keyed by object identity remount, and a remounting row
// re-registers its solid-dnd draggable under the same id — the OLD row's
// cleanup then deletes the NEW registration, leaving the tab silently
// undraggable (and warning "Cannot remove nonexistent draggable" at unmount).
// TabStrip keys rows by tab.id so object churn never remounts them.

let registry: () => Record<string, unknown>;
const Probe = () => {
  const ctx = useDragDropContext()!;
  registry = () => ctx[0].draggables as unknown as Record<string, unknown>;
  return null;
};

let paneId: string;
let groupId: string;

beforeEach(() => {
  const fresh = createInitialState();
  setStore(produce((s) => {
    s.layout = fresh.layout;
    s.tabGroups = fresh.tabGroups;
    s.edgePanels = fresh.edgePanels;
    s.floatingWindows = [];
    s.activePaneId = fresh.activePaneId;
    s.focusedRegion = 'center';
    s.nextZIndex = 100;
  }));
  const pane = findFirstPane(windowStore.layout)!;
  paneId = pane.id;
  groupId = pane.tabGroupId!;
  windowActions.addTab(groupId, {
    id: 'tab-note',
    title: 'note.md',
    contentType: 'file',
  });
});

it('updateTab (dirty-flag/title sync) keeps the tab draggable registered', () => {
  render(() => (
    <DragDropProvider>
      <Probe />
      <TabBar mode="center" groupId={groupId} paneId={paneId} />
    </DragDropProvider>
  ));

  const id = `tab:${groupId}:tab-note`;
  expect(registry()[id]).toBeTruthy();

  // What the editor's dirty-sync does on every clean↔dirty transition.
  windowActions.updateTab(groupId, 'tab-note', { isModified: true });
  expect(registry()[id]).toBeTruthy();

  // And what the daemon's title push does.
  windowActions.updateTab(groupId, 'tab-note', { title: 'renamed.md' });
  expect(registry()[id]).toBeTruthy();
});

it('the row still re-renders tab fields updated in place (dirty dot, title)', () => {
  const { container } = render(() => (
    <DragDropProvider>
      <TabBar mode="center" groupId={groupId} paneId={paneId} />
    </DragDropProvider>
  ));

  const row = () => container.querySelector('[data-tab-id="tab-note"]')!;
  expect(row().textContent).toContain('note.md');

  windowActions.updateTab(groupId, 'tab-note', { title: 'renamed.md' });
  expect(row().textContent).toContain('renamed.md');
});

it('a genuinely new tab id still mounts its own draggable', () => {
  render(() => (
    <DragDropProvider>
      <Probe />
      <TabBar mode="center" groupId={groupId} paneId={paneId} />
    </DragDropProvider>
  ));

  const otherId = generateId();
  windowActions.addTab(groupId, { id: otherId, title: 'other', contentType: 'file' });
  expect(registry()[`tab:${groupId}:${otherId}`]).toBeTruthy();
});

// Regression: a layout restore (server /api/layout) swaps tab-group ids under
// surviving components. solid-dnd draggable data is a registration-time
// snapshot, so without a keyed remount every later drag carries the dead
// boot-time sourceGroupId and moveTab silently no-ops — this is why dragging
// tabs out of collapsed edge strips (and expanded edge bars) did nothing in
// the shipped app.
const dragData = (id: string) => {
  const data = (registry()[id] as { data?: DragSource } | undefined)?.data;
  return data?.type === 'tab' ? data : undefined;
};

const swapEdgeGroupId = (position: 'left' | 'bottom'): string => {
  const oldGid = windowStore.edgePanels[position].tabGroupId;
  const newGid = generateId();
  setStore(produce((s) => {
    s.tabGroups[newGid] = { ...s.tabGroups[oldGid]!, id: newGid };
    delete s.tabGroups[oldGid];
    s.edgePanels[position].tabGroupId = newGid;
  }));
  return newGid;
};

it('collapsed strip icons re-register their draggable with the live group id after a restore', () => {
  setStore(produce((s) => { s.edgePanels.bottom.isCollapsed = true; }));
  const gid = windowStore.edgePanels.bottom.tabGroupId;
  windowActions.addTab(gid, { id: 'strip-tab', title: 'Terminal', contentType: 'terminal' });

  render(() => (
    <DragDropProvider>
      <Probe />
      <EdgePanel position="bottom" />
    </DragDropProvider>
  ));

  const id = 'edgetab-collapsed:bottom:strip-tab';
  expect(dragData(id)?.sourceGroupId).toBe(windowStore.edgePanels.bottom.tabGroupId);

  const newGid = swapEdgeGroupId('bottom');
  expect(dragData(id)?.sourceGroupId).toBe(newGid);
});

it('expanded edge tab bars re-register draggables with the live group id after a restore', () => {
  setStore(produce((s) => { s.edgePanels.left.isCollapsed = false; }));
  const gid = windowStore.edgePanels.left.tabGroupId;
  windowActions.addTab(gid, { id: 'edge-tab', title: 'Files', contentType: 'files' });

  render(() => (
    <DragDropProvider>
      <Probe />
      <TabBar mode="edge" position="left" />
    </DragDropProvider>
  ));

  const id = 'edgetab:left:edge-tab';
  expect(dragData(id)?.sourceGroupId).toBe(windowStore.edgePanels.left.tabGroupId);

  const newGid = swapEdgeGroupId('left');
  expect(dragData(id)?.sourceGroupId).toBe(newGid);
});
