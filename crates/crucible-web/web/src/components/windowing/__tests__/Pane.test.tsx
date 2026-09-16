import { describe, it, expect, beforeEach } from 'vitest';
import { createEffect, type Component } from 'solid-js';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { AppDragDropProvider } from './appProviders';
import { Pane } from '../Pane';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { findFirstPane, generateId } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';
import { shortcutLabel } from '@/lib/keyboard-shortcuts';

// Renders Pane against the real windowStore. An empty pane holds no splash and
// no session composer — that moved into its own New Session tab. It holds one
// quiet affordance: the state it is in, and the keys that fill it. Drawing
// nothing at all read as a rendering failure on a wide screen.

let paneId: string;
let groupId: string;

beforeEach(() => {
  const fresh = defaultLayout();
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
  it('renders the empty-pane affordance, not a composer, when the pane has no tabs', () => {
    const { queryByTestId, getByTestId, container } = render(() => (
      <AppDragDropProvider>
        <Pane paneId={paneId} />
      </AppDragDropProvider>
    ));

    expect(queryByTestId('center-composer')).toBeNull();
    expect(queryByTestId('composer-input')).toBeNull();
    // No tab strip either — nothing to strip.
    expect(container.querySelector('[data-tab-id]')).toBeNull();

    // The pane says what it is and which keys fill it. Before this, a third of
    // a 1280px viewport rendered nothing at all.
    const affordance = getByTestId('empty-pane');
    expect(affordance.textContent).toContain('Open a note');
    expect(affordance.textContent).toContain('Command palette');
  });

  it('prints the chords the app actually listens for', () => {
    const { getByTestId } = render(() => (
      <AppDragDropProvider>
        <Pane paneId={paneId} />
      </AppDragDropProvider>
    ));

    const chords = Array.from(getByTestId('empty-pane').querySelectorAll('kbd')).map(
      (k) => k.textContent,
    );
    // Derived from DEFAULT_SHORTCUTS, not written out here: a hint naming a
    // key nothing is bound to is worse than no hint.
    expect(chords).toEqual([
      shortcutLabel('openNoteSwitcher'),
      shortcutLabel('openCommandPalette'),
    ]);
  });

  it('names the region empty when no other pane holds a tab', () => {
    const { getByTestId } = render(() => (
      <AppDragDropProvider>
        <Pane paneId={paneId} />
      </AppDragDropProvider>
    ));

    expect(getByTestId('empty-pane').getAttribute('data-empty-pane')).toBe('region');
    expect(getByTestId('empty-pane').textContent).toContain('Nothing open');
  });

  it('names the pane empty when a sibling pane still holds work', () => {
    const siblingPaneId = generateId();
    const siblingGroupId = generateId();
    setStore(
      produce((s) => {
        s.tabGroups[siblingGroupId] = {
          id: siblingGroupId,
          tabs: [{ id: 'sibling-tab', title: 'note.md', contentType: 'file' }],
          activeTabId: 'sibling-tab',
        };
        s.layout = {
          id: generateId(),
          type: 'split',
          direction: 'horizontal',
          splitRatio: 0.5,
          first: { id: paneId, type: 'pane', tabGroupId: groupId },
          second: { id: siblingPaneId, type: 'pane', tabGroupId: siblingGroupId },
        };
      }),
    );

    const { getByTestId } = render(() => (
      <AppDragDropProvider>
        <Pane paneId={paneId} />
      </AppDragDropProvider>
    ));

    expect(getByTestId('empty-pane').getAttribute('data-empty-pane')).toBe('pane');
    expect(getByTestId('empty-pane').textContent).toContain('Empty pane');
  });

  it('leaves a rail pane alone — the affordance is the centre tiling only', () => {
    const railPane = findFirstPane(windowStore.edgePanels.left.layout)!;
    // Empty it, so the only reason it could render no affordance is its region.
    setStore(
      produce((s) => {
        s.tabGroups[railPane.tabGroupId!] = {
          id: railPane.tabGroupId!,
          tabs: [],
          activeTabId: null,
        };
      }),
    );
    const { queryByTestId } = render(() => (
      <AppDragDropProvider>
        <Pane paneId={railPane.id} />
      </AppDragDropProvider>
    ));

    // A rail slot is a fixed tool stack; "open a note here" is not an
    // instruction it can honour.
    expect(queryByTestId('empty-pane')).toBeNull();
  });

  it('renders the tab bar once a tab is added', () => {
    const { container } = render(() => (
      <AppDragDropProvider>
        <Pane paneId={paneId} />
      </AppDragDropProvider>
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
      <AppDragDropProvider>
        <Pane paneId={paneId} />
      </AppDragDropProvider>
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
