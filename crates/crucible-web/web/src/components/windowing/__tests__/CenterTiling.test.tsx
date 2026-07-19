import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { CenterTiling } from '../CenterTiling';
import { windowStore, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane, generateId } from '@/stores/windowStoreInternals';

// The old test only scraped CenterTiling.tsx to prove the string "Set ratio"
// was absent — a check that never rendered anything. Here we render the real
// tiling region and assert its actual structure: a single pane shows the pane
// content, a split layout emits a real resize splitter, and no dev-only "Set
// ratio" control is ever rendered.

let mainPaneId: string;
let mainGroupId: string;

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
  mainPaneId = pane.id;
  mainGroupId = pane.tabGroupId!;
});

describe('CenterTiling', () => {
  it('renders the pane content and no dev-only "Set ratio" control', () => {
    const { queryByText, getByText } = render(() => (
      <DragDropProvider>
        <CenterTiling />
      </DragDropProvider>
    ));

    // Single-pane layout with an empty tab group → the pane's EmptyState.
    expect(getByText('No session open')).toBeInTheDocument();
    // The removed dev-only ratio buttons must not render.
    expect(queryByText(/Set ratio/i)).toBeNull();
  });

  it('renders a resize splitter for a split layout (real tiling structure)', () => {
    const secondPaneId = generateId();
    const secondGroupId = generateId();
    setStore(
      produce((s) => {
        s.tabGroups[secondGroupId] = { id: secondGroupId, tabs: [], activeTabId: null };
        s.layout = {
          id: generateId(),
          type: 'split',
          direction: 'horizontal',
          splitRatio: 0.5,
          first: { id: mainPaneId, type: 'pane', tabGroupId: mainGroupId },
          second: { id: secondPaneId, type: 'pane', tabGroupId: secondGroupId },
        };
      }),
    );

    const { container } = render(() => (
      <DragDropProvider>
        <CenterTiling />
      </DragDropProvider>
    ));

    const splitter = container.querySelector('[data-testid="resize-splitter"]');
    expect(splitter).toBeTruthy();
    expect(splitter?.getAttribute('data-split-id')).toBeTruthy();
    // Both panes mounted around the divider.
    expect(container.querySelectorAll('[data-testid="resize-splitter"]').length).toBe(1);
  });
});
