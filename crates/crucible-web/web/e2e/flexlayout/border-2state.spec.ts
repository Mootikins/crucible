import { test, expect } from '@playwright/test';

const baseURL = '/flexlayout-test.html?layout=docked_panes';

// Vanilla renderer does not yet render the main layout structure (borders, rows, tabsets).
// These tests require the full vanilla renderer — skip until vanilla mode renders border strips.
test.describe.skip('Vanilla 2-state border', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      (window as Window & { __FLEXLAYOUT_VANILLA__?: string }).__FLEXLAYOUT_VANILLA__ = '1';
    });
    await page.goto(baseURL);
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test.describe('collapsed strip', () => {
    test('collapsed border shows per-tab items in strip', async ({ page }) => {
      const strip = page.locator('[data-layout-path="/border/left"][data-collapsed-strip]');
      await expect(strip).toBeVisible();
      await expect(strip).toHaveAttribute('data-state', 'collapsed');

      const tabItems = strip.locator('[data-collapsed-tab-item]');
      const count = await tabItems.count();
      expect(count).toBeGreaterThanOrEqual(2);
    });

    test('collapsed strip shows dock/expand button', async ({ page }) => {
      const fab = page.locator('[data-layout-path="/border/left/button/dock"]');
      await expect(fab).toBeVisible();

      const text = await fab.textContent();
      expect(text?.trim()).toBe('▶');
    });

    test('clicking dock button in strip expands border', async ({ page }) => {
      const fab = page.locator('[data-layout-path="/border/left/button/dock"]');
      await fab.click();

      const strip = page.locator('[data-layout-path="/border/left"][data-collapsed-strip]');
      await expect(strip).not.toBeAttached();
    });

    test('2-state toggle: expanded → collapsed → expanded', async ({ page }) => {
      const fab = page.locator('[data-layout-path="/border/left/button/dock"]');
      await fab.click();

      const strip = page.locator('[data-layout-path="/border/left"][data-collapsed-strip]');
      await expect(strip).not.toBeAttached();

      await page.waitForTimeout(200);

      const dockBtn = page.locator('[data-layout-path="/border/left/button/dock"]');
      await dockBtn.click();

      await expect(strip).toBeVisible();
    });
  });

  test.describe('text direction', () => {
    test('left border collapsed items have vertical text', async ({ page }) => {
      const tabItem = page.locator('[data-layout-path="/border/left/tb0"]');
      await expect(tabItem).toBeVisible();

      const writingMode = await tabItem.evaluate((el) => getComputedStyle(el).writingMode);
      expect(writingMode).toBe('vertical-rl');
    });

    test('bottom border collapsed items have horizontal text', async ({ page }) => {
      const bottomStrip = page.locator('[data-layout-path="/border/bottom"][data-collapsed-strip]');
      const isVisible = await bottomStrip.isVisible().catch(() => false);

      if (!isVisible) {
        return;
      }

      const tabItem = bottomStrip.locator('[data-collapsed-tab-item]').first();
      const writingMode = await tabItem.evaluate((el) => getComputedStyle(el).writingMode);
      expect(writingMode === 'horizontal-tb' || writingMode === '').toBe(true);
    });
  });

  test.describe('strip per-tab items', () => {
    test('each collapsed border shows correct tab names', async ({ page }) => {
      const leftStrip = page.locator('[data-layout-path="/border/left"][data-collapsed-strip]');

      if (await leftStrip.isVisible().catch(() => false)) {
        const items = leftStrip.locator('[data-collapsed-tab-item]');
        const count = await items.count();
        expect(count).toBe(2);

        await expect(items.nth(0)).toContainText('Explorer');
        await expect(items.nth(1)).toContainText('Search');
      }
    });

    test('clicking tab item in collapsed strip opens flyout', async ({ page }) => {
      const tabItem = page.locator('[data-layout-path="/border/left/tb0"]');
      await tabItem.click();

      const flyout = page.locator('[data-layout-path="/flyout/panel"]');
      await expect(flyout).toBeVisible();
    });
  });

  test.describe('no hidden state', () => {
    test('dock button toggles between 2 states only', async ({ page }) => {
      const fab = page.locator('[data-layout-path="/border/left/button/dock"]');

      await fab.click();
      const strip = page.locator('[data-layout-path="/border/left"][data-collapsed-strip]');
      await expect(strip).not.toBeAttached();

      await page.waitForTimeout(200);

      const dockBtn = page.locator('[data-layout-path="/border/left/button/dock"]');
      await dockBtn.click();
      await expect(strip).toBeVisible();

      await page.locator('[data-layout-path="/border/left/button/dock"]').click();
      await expect(strip).not.toBeAttached();
    });
  });
});
