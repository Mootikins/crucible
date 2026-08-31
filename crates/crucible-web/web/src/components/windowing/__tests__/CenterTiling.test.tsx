import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { CenterTiling } from '../CenterTiling';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findFirstPane, generateId } from '@/stores/windowStoreInternals';
import { EMPTY_PANE_PX } from '@/lib/pane-collapse';

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
    // An empty pane is void, so give it a tab to have content at all.
    windowActions.addTab(mainGroupId, {
      id: 'tiling-tab',
      title: 'note.md',
      contentType: 'file',
    });
    const { queryByText, container } = render(() => (
      <DragDropProvider>
        <CenterTiling />
      </DragDropProvider>
    ));

    // Single-pane layout → that pane's tab strip.
    expect(container.querySelector('[data-tab-id="tiling-tab"]')).toBeTruthy();
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

/**
 * A pane with no tabs used to hold its full ratio against nothing: at 1280px
 * the centre gave an empty pane 460px it could not use while the chat beside
 * it squeezed a tool card into 459px. The empty side now keeps the affordance
 * strip and the side with content takes the rest — and `splitRatio` survives
 * untouched, so the pane returns to its own size when a tab lands in it.
 */
describe('CenterTiling — an empty side yields its width', () => {
  let secondPaneId: string;
  let secondGroupId: string;

  const splitLayout = () => {
    secondPaneId = generateId();
    secondGroupId = generateId();
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
  };

  const renderTiling = () =>
    render(() => (
      <DragDropProvider>
        <CenterTiling />
      </DragDropProvider>
    ));

  /** The two flex children the splitter sits between. */
  const sides = (container: HTMLElement) => {
    const splitter = container.querySelector<HTMLElement>('[data-testid="resize-splitter"]')!;
    return {
      splitter,
      first: splitter.previousElementSibling as HTMLElement,
      second: splitter.nextElementSibling as HTMLElement,
    };
  };

  it('gives the empty side a fixed strip and the rest to the side with content', () => {
    splitLayout();
    windowActions.addTab(mainGroupId, {
      id: 'held-tab',
      title: 'note.md',
      contentType: 'file',
    });

    const { container } = renderTiling();
    const { first, second, splitter } = sides(container);

    // `1 1 0px`, NOT the 0.5 ratio: flexbox keeps the remainder when the grow
    // factors sum to under 1, which left 348px of dead centre beside the
    // yielding pane. The ratio is preserved in the store, not in the factor.
    expect(first.style.flex).toBe('1 1 0px');
    expect(second.style.flex).toBe(`0 0 ${EMPTY_PANE_PX}px`);
    // The ratio is what the pane opens back to, so the splitter goes inert
    // rather than silently rewriting a size nothing is showing.
    expect(splitter.getAttribute('data-locked')).toBe('true');
    expect(windowStore.layout.type === 'split' && windowStore.layout.splitRatio).toBe(0.5);
  });

  it('restores the ratio the moment a tab lands in the empty pane', () => {
    splitLayout();
    windowActions.addTab(mainGroupId, { id: 'held-tab', title: 'note.md', contentType: 'file' });

    const { container } = renderTiling();
    windowActions.addTab(secondGroupId, { id: 'landed', title: 'other.md', contentType: 'file' });

    const { first, second, splitter } = sides(container);
    expect(first.style.flex).toBe('0.5 1 0px');
    expect(second.style.flex).toBe('0.5 1 0px');
    expect(splitter.getAttribute('data-locked')).toBeNull();
  });

  it('leaves two empty panes at their ratio — neither has a better claim', () => {
    splitLayout();

    const { container } = renderTiling();
    const { first, second, splitter } = sides(container);

    expect(first.style.flex).toBe('0.5 1 0px');
    expect(second.style.flex).toBe('0.5 1 0px');
    expect(splitter.getAttribute('data-locked')).toBeNull();
  });
});
