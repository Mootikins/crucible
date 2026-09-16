import { describe, it, expect, beforeEach } from 'vitest';
import { createEffect, type Component, type ParentComponent } from 'solid-js';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { WindowingProvider } from '@/windowing/components/context';
import { renderPanel } from '@/lib/render-panel';
import { appWindowSlots } from '@/components/shell/windowSlots';
import { Pane } from '@/windowing/components/Pane';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { findFirstPane } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';
import { shortcutLabel } from '@/lib/keyboard-shortcuts';

// The window manager's Pane, with the app's renderer and chrome, against the
// app store. The core tests prove the pane mechanics with a neutral renderer;
// these prove what the app gives it: the registry panel, its metadata props,
// and the empty pane's chords.

/** The providers AppShell gives the window manager. */
const AppProviders: ParentComponent = (props) => (
  <WindowingProvider renderContent={renderPanel} slots={appWindowSlots}>
    <DragDropProvider>{props.children}</DragDropProvider>
  </WindowingProvider>
);

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

describe('Pane — the app empty pane', () => {
  it('offers no composer, only the affordance', () => {
    const { queryByTestId, getByTestId } = render(() => (
      <AppProviders>
        <Pane paneId={paneId} />
      </AppProviders>
    ));

    expect(queryByTestId('center-composer')).toBeNull();
    expect(queryByTestId('composer-input')).toBeNull();
    const affordance = getByTestId('empty-pane');
    expect(affordance.textContent).toContain('Open a note');
    expect(affordance.textContent).toContain('Command palette');
  });

  it('prints the chords the app actually listens for', () => {
    const { getByTestId } = render(() => (
      <AppProviders>
        <Pane paneId={paneId} />
      </AppProviders>
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
      <AppProviders>
        <Pane paneId={paneId} />
      </AppProviders>
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

describe('renderPanel', () => {
  it('says so for a content type with no registered panel', () => {
    resetGlobalRegistry();
    windowActions.addTab(groupId, { id: 'stale', title: 'stale', contentType: 'graph' });
    const { getByText } = render(() => (
      <AppProviders>
        <Pane paneId={paneId} />
      </AppProviders>
    ));
    expect(getByText('Unknown content type')).toBeTruthy();
  });
});
