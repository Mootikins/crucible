import { describe, it, expect, beforeEach } from 'vitest';
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
