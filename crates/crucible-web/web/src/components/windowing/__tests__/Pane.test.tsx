import { describe, it, expect, beforeEach } from 'vitest';
import { createEffect, type Component } from 'solid-js';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { Pane } from '../Pane';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane } from '@/stores/windowStoreInternals';

// Renders Pane against the real windowStore. An empty pane is VOID — the
// session composer moved into its own New Session tab, so a pane with no tabs
// renders no splash, no hint, and no tab bar (it stays a drop target only).

let paneId: string;
let groupId: string;

beforeEach(() => {
  const fresh = createInitialState();
  setStore(
    produce((s) => {
      s.layout = fresh.layout;
      s.tabGroups = fresh.tabGroups;
      s.edgePanels = fresh.edgePanels;
      s.floatingWindows = [];
      s.activePaneId = fresh.activePaneId;
      s.focusedRegion = 'center';
      s.nextZIndex = 100;
    }),
  );
  const pane = findFirstPane(windowStore.layout)!;
  paneId = pane.id;
  groupId = pane.tabGroupId!;
});

describe('Pane — empty center', () => {
  it('renders nothing but the drop surface when the pane has no tabs', () => {
    const { queryByTestId, container } = render(() => (
      <DragDropProvider>
        <Pane paneId={paneId} />
      </DragDropProvider>
    ));

    expect(queryByTestId('center-composer')).toBeNull();
    expect(queryByTestId('composer-input')).toBeNull();
    // No tab strip either — nothing to strip.
    expect(container.querySelector('[data-tab-id]')).toBeNull();
    expect(container.textContent?.trim()).toBe('');
  });

  it('renders the tab bar once a tab is added', () => {
    const { container } = render(() => (
      <DragDropProvider>
        <Pane paneId={paneId} />
      </DragDropProvider>
    ));

    expect(container.querySelector('[data-tab-id="note-tab"]')).toBeNull();

    windowActions.addTab(groupId, {
      id: 'note-tab',
      title: 'note.md',
      contentType: 'file',
    });

    expect(container.querySelector('[data-tab-id="note-tab"]')).toBeTruthy();
  });
});

describe('Pane — tab metadata reaches a MOUNTED panel', () => {
  /**
   * Retargeting an open draft rewrites `workspace` on the SAME tab, so the
   * pane must deliver that write to a panel it has already mounted.
   *
   * The pane deliberately does not re-render on tab-object churn — that would
   * remount the panel and discard in-progress editor edits — so the fix is
   * getters, and the risk on both sides is real: a frozen prop, or a remount.
   * This asserts both.
   */
  let renders = 0;

  const Probe: Component<{ workspace?: string }> = (props) => {
    renders += 1;
    return (
      <div>
        <span data-testid="probe-workspace">{props.workspace ?? 'unset'}</span>
        <input data-testid="probe-input" />
      </div>
    );
  };

  beforeEach(() => {
    renders = 0;
    resetGlobalRegistry();
    getGlobalRegistry().register('chat-draft', 'New Session', Probe as Component, 'center');
    windowActions.addTab(groupId, {
      id: 'tab-draft-1',
      title: 'New Session',
      contentType: 'chat-draft',
      metadata: { draftTabId: 'tab-draft-1', workspace: '/home/me/crucible' },
    });
  });

  const renderPane = () =>
    render(() => (
      <DragDropProvider>
        <Pane paneId={paneId} />
      </DragDropProvider>
    ));

  it('passes the metadata a tab was created with', () => {
    const { getByTestId } = renderPane();
    expect(getByTestId('probe-workspace').textContent).toBe('/home/me/crucible');
  });

  it('delivers a later write to the already-mounted panel', () => {
    const { getByTestId } = renderPane();

    windowActions.updateTab(groupId, 'tab-draft-1', {
      metadata: { draftTabId: 'tab-draft-1', workspace: '/home/me/atlas' },
    });

    // Without this the second "New session in <project>" silently kept the
    // first project: the props were an untracked snapshot taken at mount.
    expect(getByTestId('probe-workspace').textContent).toBe('/home/me/atlas');
  });

  it('delivers a write to a key the tab mounted with as undefined', () => {
    windowActions.updateTab(groupId, 'tab-draft-1', {
      metadata: { draftTabId: 'tab-draft-1', workspace: undefined },
    });
    const { getByTestId } = renderPane();
    expect(getByTestId('probe-workspace').textContent).toBe('unset');

    windowActions.updateTab(groupId, 'tab-draft-1', {
      metadata: { draftTabId: 'tab-draft-1', workspace: '/home/me/atlas' },
    });

    // Props are keyed from the metadata PRESENT at mount. A key declared with
    // an undefined value still gets a channel; a key omitted entirely does
    // not, and no later write can ever reach the panel.
    expect(getByTestId('probe-workspace').textContent).toBe('/home/me/atlas');
  });

  it('does not re-notify a panel when an UNRELATED tab field churns', () => {
    let seen = 0;
    const Counting: Component<{ workspace?: string }> = (props) => {
      createEffect(() => {
        props.workspace;
        seen += 1;
      });
      return <span data-testid="probe-workspace">{props.workspace ?? 'unset'}</span>;
    };
    resetGlobalRegistry();
    getGlobalRegistry().register('chat-draft', 'New Session', Counting as Component, 'center');

    renderPane();
    const before = seen;

    // `updateTab` replaces the whole tabs array, and the editor writes
    // `isModified` back on every keystroke. A prop that tracked the array
    // instead of its own value fed panels that write in response back into
    // themselves — the first version of this hung the app outright.
    windowActions.updateTab(groupId, 'tab-draft-1', { isModified: true });
    windowActions.updateTab(groupId, 'tab-draft-1', { isModified: false });

    expect(seen).toBe(before);
  });

  it('does not remount the panel to do it', () => {
    const { getByTestId } = renderPane();
    const before = renders;
    (getByTestId('probe-input') as HTMLInputElement).value = 'unsent draft';

    windowActions.updateTab(groupId, 'tab-draft-1', {
      metadata: { draftTabId: 'tab-draft-1', workspace: '/home/me/atlas' },
    });

    // A remount would discard whatever the user had typed — the reason the
    // pane reads tab fields untracked in the first place.
    expect(renders).toBe(before);
    expect((getByTestId('probe-input') as HTMLInputElement).value).toBe('unsent draft');
  });
});
