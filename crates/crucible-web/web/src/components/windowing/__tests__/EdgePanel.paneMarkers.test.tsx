import { describe, it, expect, beforeEach } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { EdgePanel } from '../EdgePanel';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { createInitialState, findPaneInLayout } from '@/stores/windowStoreInternals';

/**
 * A rail is a COLUMN of panes, so the ribbon carries one marker per pane, not
 * one per tab. The marker bands mirror the panel's own split tree — same
 * order, same flex rule — which is what puts a pane's marker beside the pane
 * it controls.
 */

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
  // The seed ships the rail closed; these assertions are about what happens
  // INSIDE an open rail.
  windowActions.setEdgePanelCollapsed('right', false);
});

const renderRail = (position: 'left' | 'right') =>
  render(() => (
    <DragDropProvider>
      <EdgePanel position={position} />
    </DragDropProvider>
  ));

const markers = (container: HTMLElement, position: 'left' | 'right') =>
  Array.from(
    container.querySelectorAll<HTMLButtonElement>(
      `[data-testid="ribbon-pane-marker-${position}"]`,
    ),
  );

const marker = (container: HTMLElement, paneId: string) =>
  container.querySelector<HTMLButtonElement>(
    `button[data-testid="ribbon-pane-marker-right"][data-pane-id="${paneId}"]`,
  )!;

/** The pane's own box in the panel body (the marker is a button, not a div). */
const paneBox = (container: HTMLElement, paneId: string) =>
  container.querySelector<HTMLElement>(`div[data-pane-id="${paneId}"]`)!;

/** The split half that sizes a pane — the box the flex rule lands on. */
const paneSlot = (container: HTMLElement, paneId: string) =>
  paneBox(container, paneId).parentElement!;

const termPane = () => findPaneInLayout(windowStore.edgePanels.right.layout, 'right-term-pane');

describe('EdgeRibbon — one marker per pane', () => {
  it('renders a marker for every pane of a split rail, in tree order', () => {
    const { container } = renderRail('right');
    expect(markers(container, 'right').map((m) => m.dataset.paneId)).toEqual([
      'right-pane',
      'right-term-pane',
    ]);
  });

  // With one pane the rail's own toggle already is that control; a lone
  // marker beside it would be two buttons for one thing.
  it('renders no markers for a single-pane rail', () => {
    const { container } = renderRail('left');
    expect(markers(container, 'left')).toHaveLength(0);
  });

  it('wears the pane’s active tab icon, so the terminal marker is a terminal', () => {
    const { container } = renderRail('right');
    expect(marker(container, 'right-term-pane').getAttribute('title')).toBe(
      'Expand Terminal',
    );
    expect(marker(container, 'right-pane').getAttribute('title')).toBe('Collapse Files');
  });
});

describe('EdgeRibbon markers — clicking one collapses its pane', () => {
  it('expands the terminal pane and collapses it again', () => {
    const { container } = renderRail('right');
    expect(marker(container, 'right-term-pane').dataset.collapsed).toBe('true');

    fireEvent.click(marker(container, 'right-term-pane'));
    expect(termPane()?.collapsed).toBe(false);
    expect(marker(container, 'right-term-pane').dataset.collapsed).toBe('false');

    fireEvent.click(marker(container, 'right-term-pane'));
    expect(termPane()?.collapsed).toBe(true);
  });

  // The rail stays open — a pane marker is not the rail toggle.
  it('leaves the rail itself open', () => {
    const { container } = renderRail('right');
    fireEvent.click(marker(container, 'right-term-pane'));
    expect(windowStore.edgePanels.right.isCollapsed).toBe(false);
  });

  it('refuses to collapse the last expanded pane', () => {
    const { container } = renderRail('right');
    fireEvent.click(marker(container, 'right-pane'));
    expect(
      findPaneInLayout(windowStore.edgePanels.right.layout, 'right-pane')?.collapsed,
    ).not.toBe(true);
  });
});

describe('a collapsed pane takes its tab strip and no more', () => {
  it('sizes the collapsed pane to the strip and the sibling to the rest', () => {
    const { container } = renderRail('right');
    expect(paneSlot(container, 'right-term-pane').style.flex).toBe('0 0 36px');
    // The tree keeps the rest, whatever the stored ratio says.
    expect(paneSlot(container, 'right-pane').style.flex).toBe('0.65 1 0px');

    fireEvent.click(marker(container, 'right-term-pane'));
    expect(paneSlot(container, 'right-term-pane').style.flex).toBe(
      `${1 - 0.65} 1 0px`,
    );
  });

  // Same rule on both sides of the window: the ribbon band shrinks with its
  // pane, or the marker drifts away from what it points at.
  it('shrinks the marker band with its pane', () => {
    const { container } = renderRail('right');
    const band = () => marker(container, 'right-term-pane').parentElement!;
    expect(band().style.flex).toBe(paneSlot(container, 'right-term-pane').style.flex);
    expect(band().style.flex).toBe('0 0 36px');

    fireEvent.click(marker(container, 'right-term-pane'));
    expect(band().style.flex).toBe(paneSlot(container, 'right-term-pane').style.flex);
    expect(band().style.flex).toBe(`${1 - 0.65} 1 0px`);
  });

  // `splitRatio` is the size the pane opens back to, so the splitter must not
  // be draggable while it cannot move anything visible.
  it('locks the splitter while a side is collapsed', () => {
    const { container } = renderRail('right');
    const splitter = () =>
      container.querySelector<HTMLElement>('[data-testid="resize-splitter"]')!;
    expect(splitter().dataset.locked).toBe('true');
    expect(splitter().className).not.toContain('cursor-row-resize');

    fireEvent.click(marker(container, 'right-term-pane'));
    expect(splitter().dataset.locked).toBeUndefined();
    expect(splitter().className).toContain('cursor-row-resize');
  });
});

describe('the collapsed pane is its own affordance', () => {
  it('opens when the bar itself is clicked', () => {
    const { container } = renderRail('right');
    fireEvent.click(paneBox(container, 'right-term-pane'));
    expect(termPane()?.collapsed).toBe(false);
  });

  // The rail is open and the tab's PANE is what is tucked away. Collapsing
  // the whole rail here would hide the tabs the user can plainly see.
  it('opens the pane when a ribbon tab of a collapsed pane is clicked', () => {
    const { container } = renderRail('right');
    const terminalTab = Array.from(
      container.querySelectorAll<HTMLButtonElement>(
        '[data-testid="collapsed-tab-button-right"]',
      ),
    ).find((b) => b.getAttribute('title') === 'Terminal')!;

    fireEvent.click(terminalTab);
    expect(termPane()?.collapsed).toBe(false);
    expect(windowStore.edgePanels.right.isCollapsed).toBe(false);
  });
});
