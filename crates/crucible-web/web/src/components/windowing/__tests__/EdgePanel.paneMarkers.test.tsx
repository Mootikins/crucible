import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { AppDragDropProvider } from './appProviders';
import { EdgeHost } from '../EdgeHost';
import { windowStore, windowActions, setStore } from '@/stores/windowStore';
import { findPaneInLayout } from '@/windowing/model/tree';
import { defaultLayout } from '@/stores/defaultLayout';

/**
 * A rail is a COLUMN of panes, so the ribbon carries one marker per pane, not
 * one per tab — and each marker sits on the TOP EDGE of the pane it controls.
 *
 * That position is MEASURED off the panel, never recomputed from the split
 * ratios. The mirrored version matched the proportions and missed the pixels:
 * every fixed cluster above the strip pushed the bands down by its own height,
 * so the terminal's marker sat 56px below the terminal's tab bar. These tests
 * therefore give the panes distinct, unequal boxes and assert the marker lands
 * on the pane's own top — an assertion the mirrored version cannot pass.
 */

/** jsdom reports every box as 0×0, so the geometry under test has to be told. */
const RIBBON_TOP = 0;
const TOGGLE_BOTTOM = 36;
const BELL_TOP = 964;

const stubGeometry = (container: HTMLElement, tops: Record<string, number>) => {
  const box = (top: number, height: number) =>
    ({ top, bottom: top + height, height, left: 0, right: 40, width: 40, x: 0, y: top }) as DOMRect;

  const ribbon = container.querySelector<HTMLElement>('[data-testid^="edge-collapsed-drop-"]')!;
  ribbon.getBoundingClientRect = () => box(RIBBON_TOP, 1000);

  const toggle = container.querySelector<HTMLElement>('[data-ribbon-ceiling]');
  if (toggle) toggle.getBoundingClientRect = () => box(RIBBON_TOP, TOGGLE_BOTTOM);

  const floorEl = container.querySelector<HTMLElement>('[data-ribbon-floor]');
  if (floorEl) floorEl.getBoundingClientRect = () => box(BELL_TOP, 36);

  const body = container.querySelector<HTMLElement>('[data-edge-panel-body]')!;
  for (const [paneId, top] of Object.entries(tops)) {
    const el = body.querySelector<HTMLElement>(`[data-pane-id="${paneId}"]`);
    if (el) el.getBoundingClientRect = () => box(top, 100);
  }
  window.dispatchEvent(new Event('resize'));
};

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
  // The seed ships the rail closed; these assertions are about what happens
  // INSIDE an open rail.
  windowActions.setEdgePanelCollapsed('right', false);
});

const renderRail = (position: 'left' | 'right') =>
  render(() => (
    <AppDragDropProvider>
      <EdgeHost position={position} />
    </AppDragDropProvider>
  ));

/** Render, then hand the component a real geometry and let it re-measure. */
const renderMeasured = async (tops: Record<string, number> = { 'right-term-pane': 650 }) => {
  const view = renderRail('right');
  stubGeometry(view.container, tops);
  await waitFor(() =>
    expect(
      view.container.querySelectorAll('[data-testid="ribbon-pane-marker-right"]').length,
    ).toBeGreaterThan(0),
  );
  return view;
};

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

/** The absolutely positioned band a marker is placed by. */
const band = (container: HTMLElement, paneId: string) => marker(container, paneId).parentElement!;

const paneBox = (container: HTMLElement, paneId: string) =>
  container.querySelector<HTMLElement>(`div[data-pane-id="${paneId}"]`)!;

/** The split half that sizes a pane — the box the flex rule lands on. */
const paneSlot = (container: HTMLElement, paneId: string) =>
  paneBox(container, paneId).parentElement!;

const termPane = () => findPaneInLayout(windowStore.edgePanels.right.layout, 'right-term-pane');

describe('a marker sits on its pane’s own top edge', () => {
  it('places the band at the pane’s measured top, not at a mirrored ratio', async () => {
    const { container } = await renderMeasured({ 'right-term-pane': 650 });
    expect(band(container, 'right-term-pane').style.top).toBe('650px');
  });

  it('follows the pane when the split moves', async () => {
    const { container } = await renderMeasured({ 'right-term-pane': 650 });
    expect(band(container, 'right-term-pane').style.top).toBe('650px');

    stubGeometry(container, { 'right-term-pane': 300 });
    await waitFor(() =>
      expect(band(container, 'right-term-pane').style.top).toBe('300px'),
    );
  });

  // The topmost pane's top edge is y=0, which is the rail toggle's own 36px,
  // and it has no boundary above it to drag. The toggle IS the control there.
  it('draws no band for the topmost pane, whose edge the rail toggle owns', async () => {
    const { container } = await renderMeasured({ 'right-pane': 0, 'right-term-pane': 650 });
    expect(markers(container, 'right').map((m) => m.dataset.paneId)).toEqual([
      'right-term-pane',
    ]);
  });

  // With one pane the rail's own toggle already is that control; a lone
  // marker beside it would be two buttons for one thing.
  it('renders no markers for a single-pane rail', () => {
    const { container } = renderRail('left');
    expect(markers(container, 'left')).toHaveLength(0);
  });

  it('wears the pane’s active tab icon, so the terminal marker is a terminal', async () => {
    const { container } = await renderMeasured();
    expect(marker(container, 'right-term-pane').getAttribute('title')).toBe(
      'Expand Terminal',
    );
  });
});

describe('the pinned clusters keep their pixels', () => {
  // A marker under the notification bell is a control the user cannot reach,
  // and nudging it clear is what put every marker on the wrong pane before.
  it('drops a band that would fall under the bell rather than nudging it', async () => {
    const { container } = await renderMeasured({ 'right-term-pane': 650 });
    expect(markers(container, 'right')).toHaveLength(1);

    stubGeometry(container, { 'right-term-pane': BELL_TOP - 10 });
    await waitFor(() => expect(markers(container, 'right')).toHaveLength(0));
  });

  it('keeps a band that ends exactly on the bell', async () => {
    const { container } = await renderMeasured({ 'right-term-pane': BELL_TOP - 36 });
    expect(markers(container, 'right')).toHaveLength(1);
  });
});

describe('EdgeRibbon markers — clicking one collapses its pane', () => {
  it('expands the terminal pane and collapses it again', async () => {
    const { container } = await renderMeasured();
    expect(marker(container, 'right-term-pane').dataset.collapsed).toBe('true');

    fireEvent.click(marker(container, 'right-term-pane'));
    expect(termPane()?.collapsed).toBe(false);
    await waitFor(() =>
      expect(marker(container, 'right-term-pane').dataset.collapsed).toBe('false'),
    );

    fireEvent.click(marker(container, 'right-term-pane'));
    expect(termPane()?.collapsed).toBe(true);
  });

  // The rail stays open — a pane marker is not the rail toggle.
  it('leaves the rail itself open', async () => {
    const { container } = await renderMeasured();
    fireEvent.click(marker(container, 'right-term-pane'));
    expect(windowStore.edgePanels.right.mode).toBe('docked');
  });

  it('refuses to collapse the last expanded pane', () => {
    windowActions.setPaneCollapsed('right-pane', true);
    expect(
      findPaneInLayout(windowStore.edgePanels.right.layout, 'right-pane')?.collapsed,
    ).not.toBe(true);
  });
});

describe('the boundary above a marker drags the split', () => {
  const handle = (container: HTMLElement) =>
    container.querySelector<HTMLElement>('[data-testid="ribbon-split-handle-right"]')!;

  it('draws a boundary rule above the marker and the tab bar’s rule below it', async () => {
    const { container } = await renderMeasured();
    // Below: the same hairline underline a TabBar draws.
    expect(marker(container, 'right-term-pane').className).toContain('border-b');
    // Above: the boundary, and it names the split it moves. The attribute is
    // deliberately NOT `data-split-id`: that name belongs to the splitter in
    // the panel, and sharing it made the drag measure itself against this 36px
    // band instead of against the split.
    expect(handle(container).dataset.boundarySplitId).toBe('right-split');
    expect(handle(container).dataset.splitId).toBeUndefined();
  });

  it('is locked while a side is collapsed, and live once both are open', async () => {
    const { container } = await renderMeasured();
    expect(handle(container).dataset.locked).toBe('true');
    expect(handle(container).className).not.toContain('cursor-row-resize');

    fireEvent.click(marker(container, 'right-term-pane'));
    await waitFor(() => expect(handle(container).dataset.locked).toBeUndefined());
    expect(handle(container).className).toContain('cursor-row-resize');
  });

  it('commits a new split ratio from a drag on the ribbon', async () => {
    const { container } = await renderMeasured();
    fireEvent.click(marker(container, 'right-term-pane')); // unlock: both sides open
    await waitFor(() => expect(handle(container).dataset.locked).toBeUndefined());

    // The ratio is measured against the SPLIT's container in the panel, not
    // against the ribbon — the ribbon is only where the pointer happens to be.
    const splitter = container.querySelector<HTMLElement>('[data-testid="resize-splitter"]')!;
    splitter.parentElement!.getBoundingClientRect = () =>
      ({ top: 0, bottom: 1000, height: 1000, left: 0, right: 300, width: 300, x: 0, y: 0 }) as DOMRect;

    const el = handle(container);
    // The band is given a deliberately wrong box. It is not what proves the
    // container lookup is right — the attribute test above is, because the bug
    // was a NAME collision and jsdom's document order hid it either way. This
    // only makes the drag notice if the lookup ever reads the band's own box.
    el.parentElement!.getBoundingClientRect = () =>
      ({ top: 650, bottom: 686, height: 36, left: 0, right: 40, width: 40, x: 0, y: 650 }) as DOMRect;
    el.setPointerCapture = vi.fn();
    el.releasePointerCapture = vi.fn();
    fireEvent(el, new PointerEvent('pointerdown', { button: 0, clientY: 650, bubbles: true }));
    fireEvent(document, new PointerEvent('pointermove', { clientY: 450, bubbles: true }));
    fireEvent(document, new PointerEvent('pointerup', { clientY: 450, bubbles: true }));

    const root = windowStore.edgePanels.right.layout;
    expect(root.type === 'split' ? root.splitRatio : null).toBeCloseTo(0.45, 5);
  });

  it('survives the re-measure its own drag causes', async () => {
    const { container } = await renderMeasured();
    fireEvent.click(marker(container, 'right-term-pane'));
    await waitFor(() => expect(handle(container).dataset.locked).toBeUndefined());

    const splitter = container.querySelector<HTMLElement>('[data-testid="resize-splitter"]')!;
    splitter.parentElement!.getBoundingClientRect = () =>
      ({ top: 0, bottom: 1000, height: 1000, left: 0, right: 300, width: 300, x: 0, y: 0 }) as DOMRect;

    const el = handle(container);
    el.setPointerCapture = vi.fn();
    el.releasePointerCapture = vi.fn();
    fireEvent(el, new PointerEvent('pointerdown', { button: 0, clientY: 650, bubbles: true }));

    // Each move commits a ratio, which re-measures, which rebuilds the bands.
    // An UNKEYED list drops this row between the two moves and runs its
    // cleanup, so the second move lands on nothing — in the browser that was a
    // 300px gesture moving the pane by 25px.
    fireEvent(document, new PointerEvent('pointermove', { clientY: 550, bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
    fireEvent(document, new PointerEvent('pointermove', { clientY: 450, bubbles: true }));
    fireEvent(document, new PointerEvent('pointerup', { clientY: 450, bubbles: true }));

    const root = windowStore.edgePanels.right.layout;
    expect(root.type === 'split' ? root.splitRatio : null).toBeCloseTo(0.45, 5);
  });
});

describe('a collapsed pane takes its tab strip and no more', () => {
  it('sizes the collapsed pane to the strip and the sibling to the rest', async () => {
    const { container } = await renderMeasured();
    expect(paneSlot(container, 'right-term-pane').style.flex).toBe('0 0 36px');
    // The tree keeps the rest, whatever the stored ratio says — and `1`, not
    // the stored 0.65, is what makes that true. Flexbox hands out free space
    // in proportion to the grow factors and KEEPS the remainder when they sum
    // to under 1, so `0.65` beside a fixed 36px strip left 35% of the rail as
    // a hole. This assertion used to spell the bug and the comment above it
    // the intent.
    expect(paneSlot(container, 'right-pane').style.flex).toBe('1 1 0px');

    fireEvent.click(marker(container, 'right-term-pane'));
    expect(paneSlot(container, 'right-term-pane').style.flex).toBe(
      `${1 - 0.65} 1 0px`,
    );
  });

  // `splitRatio` is the size the pane opens back to, so the splitter must not
  // be draggable while it cannot move anything visible.
  it('locks the splitter while a side is collapsed', async () => {
    const { container } = await renderMeasured();
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
    expect(windowStore.edgePanels.right.mode).toBe('docked');
  });
});

/**
 * A ribbon button opens a pane, so it renders on the same half of the rail as
 * the pane it opens.
 *
 * The terminal is the case that surfaced this: it lives in the BOTTOM pane of
 * the right panel, and its rail button rendered in one run from the top with
 * every other leaf button. The control sat as far from its own pane as the
 * rail allows.
 */
describe('a ribbon button sits on the same half as its pane', () => {
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
  });

  it('renders a trailing pane’s button inside the floor cluster', () => {
    setStore(
      produce((s) => {
        s.tabGroups['top-group'] = {
          id: 'top-group',
          tabs: [{ id: 'files-tab', title: 'Files', contentType: 'files' }],
          activeTabId: 'files-tab',
        };
        s.tabGroups['bottom-group'] = {
          id: 'bottom-group',
          tabs: [{ id: 'term-tab', title: 'Terminal', contentType: 'terminal' }],
          activeTabId: 'term-tab',
        };
        s.edgePanels.right.layout = {
          id: 'right-root',
          type: 'split',
          direction: 'vertical',
          splitRatio: 0.6,
          first: { id: 'right-top', type: 'pane', tabGroupId: 'top-group' },
          second: { id: 'right-bottom', type: 'pane', tabGroupId: 'bottom-group' },
        };
        s.edgePanels.right.mode = 'docked';
      }),
    );

    const { container, unmount } = render(() => (
      <AppDragDropProvider>
        <EdgeHost position="right" />
      </AppDragDropProvider>
    ));

    const floor = container.querySelector('[data-ribbon-floor]');
    expect(floor, 'the ribbon claims a floor').toBeTruthy();

    const term = container.querySelector('[data-testid="ribbon-tab-term-tab"]')
      ?? Array.from(container.querySelectorAll('button')).find(
        (b) => (b.getAttribute('title') ?? '').includes('Terminal'),
      );
    expect(term, 'the terminal has a ribbon button').toBeTruthy();
    expect(floor!.contains(term!), 'terminal button is in the floor cluster').toBe(true);

    // And the top pane's button is NOT — it stays in the leading run.
    const files = Array.from(container.querySelectorAll('button')).find(
      (b) => (b.getAttribute('title') ?? '').includes('Files'),
    );
    if (files) expect(floor!.contains(files)).toBe(false);

    unmount();
  });
});
