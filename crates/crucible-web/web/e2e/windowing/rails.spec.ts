import { test, expect, type Page } from '@playwright/test';
import { getCenter, getCenterOf } from '../helpers/geometry';
import { act, centreTab, groupIds, openHarness, pointerDrag, readStore, tabIds } from './harness';

/**
 * The rails: collapse and expand, the empty rail, tab moves between a rail
 * and the centre, the slide under reduced motion, and the side swap.
 */

test.beforeEach(async ({ page }) => openHarness(page));

const modeOf = async (page: Page, side: 'left' | 'right') => (await readStore(page)).edgePanels[side].mode;

/** A point in the centre pane body, under the centre tab bar. */
async function centreDropPoint(page: Page): Promise<{ x: number; y: number }> {
  const box = (await centreTab(page, 'tab-alpha').boundingBox())!;
  return { x: box.x + box.width / 2, y: box.y + box.height + 40 };
}

test.describe('collapse and the empty rail', () => {
  test('collapses and re-expands left edge panel via store action', async ({ page }) => {
    const bar = page.getByTestId('edge-tabbar-left');
    await expect(bar).toBeVisible();

    await act(page, 'setEdgePanelCollapsed', 'left', true);
    await expect(page.getByTestId('edge-collapsed-drop-left')).toBeVisible();
    await expect(bar).not.toBeVisible();
    expect(await modeOf(page, 'left')).toBe('strip');

    await act(page, 'setEdgePanelCollapsed', 'left', false);
    await expect(bar).toBeVisible();
    expect(await modeOf(page, 'left')).toBe('docked');
  });

  test('shows valid collapsed strip state when edge panel has no tabs', async ({ page }) => {
    // The neutral policy lets every tab close, so a close empties the rail.
    const [left] = await groupIds(page, 'left');
    await act(page, 'removeTab', left, 'tab-left');

    expect(await tabIds(page, 'left')).toEqual([]);
    await expect(page.getByTestId('edge-collapsed-drop-left')).toBeVisible();
    await expect(page.getByTestId('edge-tabbar-left')).not.toBeVisible();
    // The ribbon stays, and it has no tab button for an empty rail.
    await expect(
      page.locator('[data-testid="edge-collapsed-drop-left"] [data-testid="collapsed-tab-button-left"]'),
    ).toHaveCount(0);
  });
});

test.describe('cross-zone tab drag and drop', () => {
  // The seed gives the left rail one tab. A second one lets a spec move a
  // tab out without emptying the rail.
  test.beforeEach(async ({ page }) => {
    const [left] = await groupIds(page, 'left');
    await act(page, 'addTab', left, { id: 'tab-l2', title: 'Two', contentType: 'beta' });
    await act(page, 'setActiveTab', left, 'tab-left');
    await expect(page.getByTestId('edge-tab-left-tab-l2')).toBeVisible();
  });

  test('drag edge tab from expanded left panel to center pane', async ({ page }) => {
    const from = await getCenter(page, '[data-testid="edge-tab-left-tab-l2"]');
    await pointerDrag(page, from, await centreDropPoint(page), 50);

    await expect(page.getByTestId('edge-tab-left-tab-l2')).not.toBeVisible();
    await expect(centreTab(page, 'tab-l2')).toBeVisible();
    expect(await tabIds(page, 'center')).toContain('tab-l2');
    expect(await tabIds(page, 'left')).toEqual(['tab-left']);
  });

  test('drag center tab to left edge panel', async ({ page }) => {
    const from = await getCenterOf(page, centreTab(page, 'tab-beta'));
    const to = await getCenter(page, '[data-testid="edge-tabbar-left"]');

    await pointerDrag(page, from, to, 30);

    await expect(page.getByTestId('edge-tab-left-tab-beta')).toBeVisible();
    await expect(centreTab(page, 'tab-beta')).not.toBeVisible();
    expect(await tabIds(page, 'left')).toContain('tab-beta');
    expect(await tabIds(page, 'center')).toEqual(['tab-alpha']);
  });

  test('dragging last tab out of edge panel auto-collapses it', async ({ page }) => {
    const [left] = await groupIds(page, 'left');
    const [centre] = await groupIds(page, 'center');
    // Empty the rail down to one tab, so the drag below moves the last one.
    await act(page, 'moveTab', left, centre, 'tab-l2');
    await expect(centreTab(page, 'tab-l2')).toBeVisible();

    const bar = page.getByTestId('edge-tabbar-left');
    await expect(bar).toBeVisible();

    const from = await getCenter(page, '[data-testid="edge-tab-left-tab-left"]');
    await pointerDrag(page, from, await centreDropPoint(page), 50);

    await expect(bar).not.toBeVisible();
    expect(await tabIds(page, 'left')).toEqual([]);
    expect(await modeOf(page, 'left')).toBe('strip');
  });

  test('drag center tab onto collapsed right panel expands it', async ({ page }) => {
    expect(await modeOf(page, 'right')).toBe('strip');
    const from = await getCenterOf(page, centreTab(page, 'tab-beta'));
    const to = await getCenter(page, '[data-testid="edge-collapsed-drop-right"]');

    await pointerDrag(page, from, to);

    await expect(page.getByTestId('edge-tabbar-right').first()).toBeVisible();
    await expect(page.getByTestId('edge-tab-right-tab-beta')).toBeVisible();
    expect(await modeOf(page, 'right')).toBe('docked');
    expect(await tabIds(page, 'right')).toContain('tab-beta');
  });

  test('edge tab drag creates drag overlay with tab title', async ({ page }) => {
    const from = await getCenter(page, '[data-testid="edge-tab-left-tab-l2"]');

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x + 30, from.y + 30, { steps: 5 });

    // While the pointer is down, the drag overlay shows the tab title.
    await expect(page.locator('text="Two"').last()).toBeVisible();

    await page.mouse.up();

    // The release lands inside the same bar, which may reorder it. The tab
    // stays in the rail either way.
    await expect(page.getByTestId('edge-tab-left-tab-l2')).toBeVisible();
    expect((await tabIds(page, 'left')).sort()).toEqual(['tab-l2', 'tab-left']);
  });
});

/**
 * The rail opens with a 200ms tween in JavaScript, not CSS: a CSS width
 * transition runs on the main thread while the inner translate runs on the
 * compositor, and under load the two tear apart. `index.css` zeroes every
 * CSS animation under `prefers-reduced-motion`, so the JavaScript tween has
 * to read the media query itself.
 *
 * This gate samples a position inside the moving frame on twenty frames in a
 * row and requires that it never moves. A repeat count only shows that a race
 * is rare; this shows that it is absent.
 */
test.describe('the edge panel under a reduced-motion preference', () => {
  test.use({ reducedMotion: 'reduce' });

  test('opens without moving its controls across frames', async ({ page }) => {
    // Sample from INSIDE the animating frame: the rail's tab body. The ribbon
    // sits outside it and never moves, so it would pass with the tween on.
    const lefts = await page.evaluate(async () => {
      const store = (window as unknown as Record<string, any>).__windowStore;
      const actions = (window as unknown as Record<string, any>).__windowActions;
      // Toggle in the same turn that the sampling starts in: the tween's
      // first frame is what a click would otherwise race.
      if (store.edgePanels.right.mode !== 'docked') actions.toggleEdgePanel('right');

      const probe = () => {
        const el = document.querySelector(
          '[data-testid="edge-host-right"] [data-testid="content-gamma"]',
        );
        return el ? Math.round(el.getBoundingClientRect().left) : -1;
      };
      const seen: number[] = [];
      for (let i = 0; i < 20; i++) {
        seen.push(probe());
        await new Promise((r) => requestAnimationFrame(() => r(null)));
      }
      return seen;
    });

    expect(lefts.filter((n) => n < 0), 'the probe was not in the page').toEqual([]);
    const distinct = [...new Set(lefts)];
    expect(
      distinct.length,
      `the control moved across frames (positions seen: ${distinct.join(', ')}). ` +
        'The panel still tweens under a reduced-motion preference.',
    ).toBe(1);
  });
});

test.describe('swap side panels', () => {
  test('Ctrl+Shift+\\ mirrors the two rails', async ({ page }) => {
    expect(await tabIds(page, 'left')).toEqual(['tab-left']);
    expect(await tabIds(page, 'right')).toEqual(['tab-right']);

    await page.keyboard.press('Control+Shift+\\');

    await expect.poll(() => tabIds(page, 'left')).toEqual(['tab-right']);
    expect(await tabIds(page, 'right')).toEqual(['tab-left']);
  });

  test('the ribbon button runs the same action', async ({ page }) => {
    await page.getByTestId('ribbon-cmd-swap-sides').click();
    await expect.poll(() => tabIds(page, 'left')).toEqual(['tab-right']);
    expect(await tabIds(page, 'right')).toEqual(['tab-left']);
  });

  test('swapping back restores the original sides', async ({ page }) => {
    const before = await readStore(page);
    await page.keyboard.press('Control+Shift+\\');
    await expect.poll(() => tabIds(page, 'left')).toEqual(['tab-right']);
    await page.keyboard.press('Control+Shift+\\');
    await expect.poll(() => tabIds(page, 'left')).toEqual(['tab-left']);
    const after = await readStore(page);
    expect(after.edgePanels.left.mode).toBe(before.edgePanels.left.mode);
    expect(after.edgePanels.right.mode).toBe(before.edgePanels.right.mode);
  });

  // Ctrl+\ splits a pane and must keep doing so. The swap took the adjacent
  // chord so that this binding did not have to move.
  test('leaves Ctrl+\\ splitting panes', async ({ page }) => {
    await page.keyboard.press('Control+\\');
    await expect.poll(async () => (await readStore(page)).layout.type).toBe('split');
    expect(await tabIds(page, 'left')).toEqual(['tab-left']);
    expect(await tabIds(page, 'right')).toEqual(['tab-right']);
  });
});
