import { test, expect, type Page } from '@playwright/test';
import { findPath, findTabButton } from './helpers';

const baseURL = '/flexlayout-test.html?layout=docked_panes';

async function clickFlyoutTab(page: Page, path: string) {
  await page.evaluate((targetPath) => {
    const el = document.querySelector(`[data-layout-path="${targetPath}"]`) as HTMLElement | null;
    el?.click();
  }, path);
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    (window as Window & { __FLEXLAYOUT_VANILLA__?: string }).__FLEXLAYOUT_VANILLA__ = '1';
  });
  await page.goto(baseURL);
  await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
});

test.describe('Vanilla flyout overlay', () => {
  test('clicking collapsed border tab opens flyout overlay', async ({ page }) => {
    const mainBefore = await findPath(page, '/main').boundingBox();
    expect(mainBefore).toBeTruthy();

    await clickFlyoutTab(page, '/border/left/tb0');

    const flyout = page.locator('[data-layout-path="/flyout/panel"]');
    await expect(flyout).toBeVisible();
    await expect(findPath(page, '/border/left/t0')).toContainText('File explorer');

    const mainAfter = await findPath(page, '/main').boundingBox();
    expect(mainAfter).toBeTruthy();
    expect(mainAfter!.width).toBeCloseTo(mainBefore!.width, 1);
  });

  test('clicking outside flyout dismisses it', async ({ page }) => {
    await clickFlyoutTab(page, '/border/bottom/tb0');
    await expect(page.locator('[data-layout-path="/flyout/panel"]')).toBeVisible();

    await page.locator('[data-layout-path="/flyout/backdrop"]').click();
    await expect(page.locator('[data-layout-path="/flyout/panel"]')).toHaveCount(0);
  });

  test('clicking different collapsed tab swaps flyout content in place', async ({ page }) => {
    await clickFlyoutTab(page, '/border/bottom/tb0');
    const flyout = page.locator('[data-layout-path="/flyout/panel"]');
    await expect(flyout).toBeVisible();
    const before = await flyout.boundingBox();
    expect(before).toBeTruthy();
    await expect(findPath(page, '/border/bottom/t0')).toContainText('Integrated terminal');

    await clickFlyoutTab(page, '/border/bottom/tb2');
    await expect(flyout).toBeVisible();
    await expect(findPath(page, '/border/bottom/t2')).toContainText('Problems panel');

    const after = await flyout.boundingBox();
    expect(after).toBeTruthy();
    expect(after!.x).toBeCloseTo(before!.x, 1);
    expect(after!.y).toBeCloseTo(before!.y, 1);
    expect(after!.width).toBeCloseTo(before!.width, 1);
    expect(after!.height).toBeCloseTo(before!.height, 1);
  });

  test('flyout opens correctly from all border edges', async ({ page }) => {
    const layout = await findPath(page, '/').boundingBox();
    expect(layout).toBeTruthy();

    const cases: Array<{ edge: 'left' | 'right' | 'top' | 'bottom'; expected: string }> = [
      { edge: 'left', expected: 'File explorer' },
      { edge: 'right', expected: 'Properties panel' },
      { edge: 'top', expected: 'Toolbar' },
      { edge: 'bottom', expected: 'Integrated terminal' },
    ];

    for (const c of cases) {
      await clickFlyoutTab(page, `/border/${c.edge}/tb0`);
      const flyout = page.locator('[data-layout-path="/flyout/panel"]');
      await expect(flyout).toBeVisible();
      const box = await flyout.boundingBox();
      expect(box).toBeTruthy();

      if (c.edge === 'left') {
        expect(box!.x).toBeGreaterThan(layout!.x);
      } else if (c.edge === 'right') {
        expect(box!.x + box!.width).toBeLessThan(layout!.x + layout!.width);
      } else if (c.edge === 'top') {
        expect(box!.y).toBeGreaterThan(layout!.y);
      } else {
        expect(box!.y + box!.height).toBeLessThan(layout!.y + layout!.height);
      }

      await expect(page.locator('.flexlayout__tab').filter({ hasText: c.expected }).first()).toBeVisible();
    }
  });

  test('z-order is main content < flyout < floating panels', async ({ page }) => {
    await clickFlyoutTab(page, '/border/left/tb0');

    const zValues = await page.evaluate(() => {
      const main = document.querySelector('.flexlayout__layout_main') as HTMLElement | null;
      const flyout = document.querySelector('[data-layout-path="/flyout/panel"]') as HTMLElement | null;
      return {
        main: Number.parseInt(getComputedStyle(main!).zIndex || '0', 10) || 0,
        flyout: Number.parseInt(getComputedStyle(flyout!).zIndex || '0', 10) || 0,
      };
    });

    expect(zValues.flyout).toBeGreaterThan(zValues.main);
    expect(zValues.flyout).toBeLessThan(1000);
  });
});
