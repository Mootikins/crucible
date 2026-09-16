import { test, expect, type Page } from '@playwright/test';
import { getCenterOf } from '../helpers/geometry';
import { act, centreTab, groupIds, openHarness, pointerDrag, readStore, tabIds } from './harness';

/**
 * Tabs inside one region: the active tab, a close through the store, the
 * empty centre, and a reorder by drag inside one tab bar.
 */

test.beforeEach(async ({ page }) => openHarness(page));

/** The ids in the left rail's tab bar, in DOM order. */
function leftBarOrder(page: Page): Promise<string[]> {
  return page
    .locator('[data-testid="edge-tabbar-left"] [data-tab-id]')
    .evaluateAll((els) => els.map((el) => el.getAttribute('data-tab-id') ?? ''));
}

/**
 * Give the left rail three tabs, `tab-left` first, and keep `tab-left`
 * active. `addTab` activates what it adds, and a reorder spec wants the
 * body to stay as it was.
 */
async function seedLeftStrip(page: Page): Promise<void> {
  const [left] = await groupIds(page, 'left');
  await act(page, 'addTab', left, { id: 'tab-l2', title: 'Two', contentType: 'beta' });
  await act(page, 'addTab', left, { id: 'tab-l3', title: 'Three', contentType: 'beta' });
  await act(page, 'setActiveTab', left, 'tab-left');
  await expect(page.getByTestId('edge-tab-left-tab-l3')).toBeVisible();
  expect(await leftBarOrder(page)).toEqual(['tab-left', 'tab-l2', 'tab-l3']);
}

test.describe('tabs in the centre', () => {
  test('clicking tabs updates active tab in the center group', async ({ page }) => {
    const [centre] = await groupIds(page, 'center');
    const activeOf = async () => (await readStore(page)).tabGroups[centre!]!.activeTabId;

    await centreTab(page, 'tab-beta').click();
    await expect.poll(activeOf).toBe('tab-beta');
    await expect(page.getByTestId('content-beta')).toBeVisible();

    await centreTab(page, 'tab-alpha').click();
    await expect.poll(activeOf).toBe('tab-alpha');
    await expect(page.getByTestId('content-alpha')).toBeVisible();
  });

  test('closing a tab via store action updates center tab DOM', async ({ page }) => {
    await expect(page.locator('[data-tab-id^="tab-"]:not([data-testid^="edge-tab-"])')).toHaveCount(2);
    const [centre] = await groupIds(page, 'center');

    await act(page, 'removeTab', centre, 'tab-beta');

    await expect(centreTab(page, 'tab-beta')).toHaveCount(0);
    await expect(page.locator('[data-tab-id^="tab-"]:not([data-testid^="edge-tab-"])')).toHaveCount(1);
    expect(await tabIds(page, 'center')).toEqual(['tab-alpha']);
  });

  test('shows center empty state after all center tabs are removed', async ({ page }) => {
    // Closing a pane's last tab collapses that pane out of the tree, so the
    // centre ends as one pane whose group no longer holds a tab. The pane
    // then draws its empty state and no tab bar.
    for (const id of await groupIds(page, 'center')) {
      for (const tab of (await readStore(page)).tabGroups[id]!.tabs) {
        await act(page, 'removeTab', id, tab.id);
      }
    }

    expect(await tabIds(page, 'center')).toEqual([]);
    const empty = page.getByTestId('empty-pane');
    await expect(empty).toHaveCount(1);
    await expect(empty).toHaveAttribute('data-empty-pane', 'region');
    await expect(empty).toContainText('Nothing open');
    await expect(page.locator('[data-tab-id]:not([data-testid^="edge-tab-"])')).toHaveCount(0);
    // The rails keep their tabs.
    await expect(page.getByTestId('edge-tab-left-tab-left')).toBeVisible();
  });
});

test.describe('tab reorder within one bar', () => {
  test.beforeEach(async ({ page }) => seedLeftStrip(page));

  test('reorder edge tab: drag first tab past third tab', async ({ page }) => {
    const first = page.getByTestId('edge-tab-left-tab-left');
    const third = page.getByTestId('edge-tab-left-tab-l3');

    const from = await getCenterOf(page, first);
    const thirdBox = (await third.boundingBox())!;
    // Keep the pointer inside the rail's bar. A release over the centre tab
    // bar is a move to another bar, not this test's reorder.
    const barBox = (await page.getByTestId('edge-tabbar-left').boundingBox())!;
    const to = {
      x: Math.min(thirdBox.x + thirdBox.width - 2, barBox.x + barBox.width - 8),
      y: thirdBox.y + thirdBox.height / 2,
    };

    await pointerDrag(page, from, to, 20);

    await expect.poll(async () => (await leftBarOrder(page)).indexOf('tab-left')).toBeGreaterThan(0);
    await expect.poll(async () => (await tabIds(page, 'left')).indexOf('tab-left')).toBeGreaterThan(0);
  });

  test('reorder edge tab: drag last tab to first position', async ({ page }) => {
    const last = page.getByTestId('edge-tab-left-tab-l3');
    const first = page.getByTestId('edge-tab-left-tab-left');
    await last.scrollIntoViewIfNeeded();

    const from = await getCenterOf(page, last);
    const firstBox = (await first.boundingBox())!;
    // Clamp the drop point inside the tab strip. The bar's left edge holds
    // controls outside the reorder bounds, and a release there cancels.
    const stripBox = await first.evaluate((el) => {
      const r = el.parentElement!.getBoundingClientRect();
      return { x: r.x };
    });
    const to = { x: Math.max(firstBox.x + 2, stripBox.x + 8), y: firstBox.y + firstBox.height / 2 };

    await pointerDrag(page, from, to, 25);

    await expect
      .poll(async () => {
        const order = await leftBarOrder(page);
        return order.indexOf('tab-l3') < order.indexOf('tab-left');
      })
      .toBe(true);
    expect((await tabIds(page, 'left'))[0]).toBe('tab-l3');
  });

  test('reorder edge tab within left panel', async ({ page }) => {
    const from = await getCenterOf(page, page.getByTestId('edge-tab-left-tab-left'));
    const secondBox = (await page.getByTestId('edge-tab-left-tab-l2').boundingBox())!;
    const to = { x: secondBox.x + secondBox.width - 2, y: secondBox.y + secondBox.height / 2 };

    await pointerDrag(page, from, to);

    await expect
      .poll(async () => {
        const order = await tabIds(page, 'left');
        return order.indexOf('tab-left') > order.indexOf('tab-l2');
      })
      .toBe(true);
  });

  test('insert indicator appears during edge tab reorder drag', async ({ page }) => {
    const from = await getCenterOf(page, page.getByTestId('edge-tab-left-tab-left'));
    const secondBox = (await page.getByTestId('edge-tab-left-tab-l2').boundingBox())!;
    const to = { x: secondBox.x + secondBox.width / 2, y: secondBox.y + secondBox.height / 2 };

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 20 });

    const indicator = page.locator('[class*="bg-primary"][class*="rounded-full"][class*="h-5"]');
    await expect(indicator.first()).toBeVisible();

    await page.mouse.up();
    await expect(indicator).toHaveCount(0);
  });

  test('no insert indicator during cross-zone drag', async ({ page }) => {
    const from = await getCenterOf(page, page.getByTestId('edge-tab-left-tab-l2'));
    const to = await getCenterOf(page, centreTab(page, 'tab-alpha'));

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 10 });

    // The drag overlay names the tab. It proves the drag is past its
    // threshold, so the absence check below is not vacuous.
    await expect(page.locator('text="Two"').last()).toBeVisible();

    // The reorder indicator is TabBar's 2px by 20px bar.
    await expect(page.locator('[class*="w-0.5"][class*="h-5"][class*="bg-primary"]')).toHaveCount(0);

    await page.mouse.up();
  });
});
