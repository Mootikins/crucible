import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { EdgeHost } from '../EdgeHost';
import { windowActions } from '@/windowing/store';
import { CoreProviders, configureRails } from './fixtures';

/**
 * The ribbon publishes the geometry that a theme needs to put the trailing tab
 * cluster at the top edge of its pane. The position is a look, so the theme
 * owns it. The component measures the DOM and writes custom properties on the
 * ribbon. The default theme keeps the cluster pinned to the far end.
 */

const RIBBON_TOP = 100;
const TRAILING_TOP = 964;
const TAIL_TOP = 1000;

const box = (top: number, height: number) =>
  ({ top, bottom: top + height, height, left: 0, right: 40, width: 40, x: 0, y: top }) as DOMRect;

/** jsdom lays nothing out, so the test gives each element its box. */
const stubGeometry = (container: HTMLElement, trailingPaneTop: number) => {
  const ribbon = container.querySelector<HTMLElement>('[data-testid="edge-collapsed-drop-right"]')!;
  ribbon.getBoundingClientRect = () => box(RIBBON_TOP, 1036);
  ribbon.querySelector<HTMLElement>('[data-ribbon-ceiling]')!.getBoundingClientRect = () =>
    box(RIBBON_TOP, 36);
  // Three leading tabs of 40px under the toggle.
  ribbon.querySelector<HTMLElement>('.wm-ribbon-leading')!.getBoundingClientRect = () =>
    box(RIBBON_TOP + 36, 120);
  ribbon.querySelector<HTMLElement>('.wm-ribbon-trailing')!.getBoundingClientRect = () =>
    box(RIBBON_TOP + TRAILING_TOP, 40);
  ribbon.querySelector<HTMLElement>('.wm-ribbon-tail')!.getBoundingClientRect = () =>
    box(RIBBON_TOP + TAIL_TOP, 36);
  const body = container.querySelector<HTMLElement>('[data-edge-panel-body="right"]')!;
  body.querySelector<HTMLElement>('[data-pane-id="right-pane"]')!.getBoundingClientRect = () =>
    box(RIBBON_TOP, trailingPaneTop);
  body.querySelector<HTMLElement>('[data-pane-id="right-term-pane"]')!.getBoundingClientRect =
    () => box(RIBBON_TOP + trailingPaneTop, 300);
  window.dispatchEvent(new Event('resize'));
};

const ribbonOf = (container: HTMLElement) =>
  container.querySelector<HTMLElement>('[data-testid="edge-collapsed-drop-right"]')!;

const renderRail = () =>
  render(() => (
    <CoreProviders>
      <EdgeHost position="right" />
    </CoreProviders>
  ));

beforeEach(() => {
  configureRails();
  windowActions.setEdgePanelCollapsed('right', false);
  windowActions.setPaneCollapsed('right-term-pane', false);
});

describe('the ribbon publishes its geometry', () => {
  it('writes the top of the trailing pane, from the ribbon top', async () => {
    const { container } = renderRail();
    stubGeometry(container, 650);
    await waitFor(() =>
      expect(ribbonOf(container).style.getPropertyValue('--wm-trailing-pane-top')).toBe('650px'),
    );
  });

  it('follows the pane when the split moves', async () => {
    const { container } = renderRail();
    stubGeometry(container, 650);
    await waitFor(() =>
      expect(ribbonOf(container).style.getPropertyValue('--wm-trailing-pane-top')).toBe('650px'),
    );
    // Let the first frame of the mount pass, so that only the move can
    // measure the new geometry.
    await new Promise((r) => requestAnimationFrame(r));
    stubGeometry(container, 300);
    await waitFor(() =>
      expect(ribbonOf(container).style.getPropertyValue('--wm-trailing-pane-top')).toBe('300px'),
    );
  });

  it('writes the bounds that a theme clamps to', async () => {
    const { container } = renderRail();
    stubGeometry(container, 650);
    const ribbon = ribbonOf(container);
    await waitFor(() => expect(ribbon.style.getPropertyValue('--wm-ribbon-ceiling')).toBe('156px'));
    expect(ribbon.style.getPropertyValue('--wm-ribbon-floor')).toBe(`${TAIL_TOP}px`);
    expect(ribbon.style.getPropertyValue('--wm-ribbon-trailing-height')).toBe('40px');
  });

  it('writes no pane top for a rail of one pane', async () => {
    const { container } = render(() => (
      <CoreProviders>
        <EdgeHost position="left" />
      </CoreProviders>
    ));
    const ribbon = container.querySelector<HTMLElement>('[data-testid="edge-collapsed-drop-left"]')!;
    ribbon.getBoundingClientRect = () => box(0, 1000);
    window.dispatchEvent(new Event('resize'));
    await waitFor(() => expect(ribbon.style.getPropertyValue('--wm-ribbon-floor')).not.toBe(''));
    expect(ribbon.style.getPropertyValue('--wm-trailing-pane-top')).toBe('');
    expect(ribbon.querySelector('.wm-ribbon-trailing')).toBeNull();
  });
});

describe('the default keeps the trailing cluster at the far end', () => {
  it('pins the cluster to the floor in the flow, with no position of its own', () => {
    const { container } = renderRail();
    const trailing = ribbonOf(container).querySelector<HTMLElement>('.wm-ribbon-trailing')!;
    expect(trailing.hasAttribute('data-ribbon-floor')).toBe(true);
    expect(trailing.classList.contains('mt-auto')).toBe(true);
    expect(getComputedStyle(trailing).position).not.toBe('absolute');
    expect(trailing.style.top).toBe('');
  });
});

describe('a theme that puts the cluster at its pane', () => {
  let sheet: HTMLStyleElement;
  beforeEach(() => {
    // The rule that a theme writes. jsdom does not resolve var(), so the test
    // reads the published value on the ribbon.
    sheet = document.createElement('style');
    sheet.textContent = `
      .wm-ribbon-trailing { position: absolute; top: var(--wm-trailing-pane-top); }
      .wm-ribbon-tail { margin-top: auto; }
    `;
    document.head.appendChild(sheet);
  });
  afterEach(() => sheet.remove());

  it('reads the published pane top', async () => {
    const { container } = renderRail();
    const trailing = ribbonOf(container).querySelector<HTMLElement>('.wm-ribbon-trailing')!;
    expect(getComputedStyle(trailing).position).toBe('absolute');
    expect(getComputedStyle(trailing).top).toBe('var(--wm-trailing-pane-top)');
    stubGeometry(container, 650);
    await waitFor(() =>
      expect(ribbonOf(container).style.getPropertyValue('--wm-trailing-pane-top')).toBe('650px'),
    );
  });

  // The cluster floats over the rail now, at the pane top. The pinned tail
  // is the real far end, so the pane markers stop at the tail, not at the
  // cluster.
  it('lets the pane markers reach the tail', async () => {
    const { container } = renderRail();
    stubGeometry(container, 650);
    // The cluster now sits on the marker's own pixels.
    ribbonOf(container).querySelector<HTMLElement>('.wm-ribbon-trailing')!.getBoundingClientRect =
      () => box(RIBBON_TOP + 650, 40);
    window.dispatchEvent(new Event('resize'));
    await waitFor(() =>
      expect(
        container.querySelector('[data-testid="ribbon-pane-marker-right"][data-pane-id="right-term-pane"]'),
      ).not.toBeNull(),
    );
  });
});
