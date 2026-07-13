import { it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider, useDragDropContext } from '@thisbeyond/solid-dnd';
import { TabBar } from '../TabBar';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane, generateId } from '@/stores/windowStoreInternals';

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
    s.flyoutState = null;
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
