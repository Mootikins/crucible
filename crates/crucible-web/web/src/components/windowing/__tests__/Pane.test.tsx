import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { Pane } from '../Pane';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane } from '@/stores/windowStoreInternals';

// The old test read Pane.tsx as a string and asserted it "contains" the text
// EmptyState / import { EmptyState }. That passes even if the Show/fallback
// wiring is broken. Here we render Pane against the real windowStore and assert
// the EmptyState actually appears when the pane has no tabs — and disappears
// once a tab is added.

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

describe('Pane — empty state', () => {
  it('renders the EmptyState (no tabs) with its call-to-action', () => {
    const { getByText, getByRole } = render(() => (
      <DragDropProvider>
        <Pane paneId={paneId} />
      </DragDropProvider>
    ));

    // EmptyState's copy + action button.
    expect(getByText('No session open')).toBeInTheDocument();
    expect(getByRole('button', { name: /New Session/i })).toBeInTheDocument();
  });

  it('replaces the EmptyState with the tab bar once a tab is added', () => {
    const { queryByText, container } = render(() => (
      <DragDropProvider>
        <Pane paneId={paneId} />
      </DragDropProvider>
    ));

    expect(queryByText('No session open')).toBeInTheDocument();

    windowActions.addTab(groupId, {
      id: 'note-tab',
      title: 'note.md',
      contentType: 'file',
    });

    expect(queryByText('No session open')).toBeNull();
    // The tab strip now renders the tab row.
    expect(container.querySelector('[data-tab-id="note-tab"]')).toBeTruthy();
  });
});
