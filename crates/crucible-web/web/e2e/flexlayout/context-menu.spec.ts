import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
} from './helpers';

const baseURL = '/flexlayout-test.html';

test.describe('Context Menu', () => {
  test('context menu appears on right-click tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tabButton = findTabButton(page, '/ts0', 0);
    await tabButton.click({ button: 'right' });
    await page.waitForTimeout(200);

    const contextMenu = page.locator('.flexlayout__popup_menu');
    await expect(contextMenu).toBeVisible();
  });

  test('context menu contains built-in items', async ({ page }) => {
    await page.goto(baseURL + '?layout=simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tabButton = findTabButton(page, '/ts0', 0);
    await tabButton.click({ button: 'right' });
    await page.waitForTimeout(200);

    const contextMenu = page.locator('.flexlayout__popup_menu');
    await expect(contextMenu).toBeVisible();

    const menuItems = contextMenu.locator('.flexlayout__popup_menu_item');
    const itemTexts = await menuItems.allTextContents();

    expect(itemTexts).toContain('Close');
    expect(itemTexts).toContain('Close Others');
    expect(itemTexts).toContain('Close All');
  });

  test('context menu "Close Others" action works', async ({ page }) => {
    await page.goto(baseURL + '?layout=simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const initialTabCount = await page.locator('.flexlayout__tab_button').count();
    expect(initialTabCount).toBeGreaterThan(1);

    const tabButton = findTabButton(page, '/ts0', 0);
    await tabButton.click({ button: 'right' });
    await page.waitForTimeout(200);

    const closeOthersItem = page.locator('.flexlayout__popup_menu_item').filter({ hasText: 'Close Others' });
    await closeOthersItem.click();
    await page.waitForTimeout(300);

    const afterTabCount = await page.locator('.flexlayout__tab_button').count();
    expect(afterTabCount).toBe(1);
  });

  test('context menu shows pin/unpin for pinnable tabs', async ({ page }) => {
    await page.goto(baseURL + '?layout=simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tabButton = findTabButton(page, '/ts0', 0);
    await tabButton.click({ button: 'right' });
    await page.waitForTimeout(200);

    const contextMenu = page.locator('.flexlayout__popup_menu');
    const menuItems = contextMenu.locator('.flexlayout__popup_menu_item');
    const itemTexts = await menuItems.allTextContents();

    expect(itemTexts.some(text => text.includes('Pin') || text.includes('Unpin'))).toBe(true);
  });

  test('context menu float action works', async ({ page }) => {
    await page.goto(baseURL + '?layout=simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tabButton = findTabButton(page, '/ts0', 0);
    await tabButton.click({ button: 'right' });
    await page.waitForTimeout(200);

    const floatItem = page.locator('.flexlayout__popup_menu_item').filter({ hasText: 'Float' });
    if (await floatItem.count() > 0) {
      await floatItem.click();
      await page.waitForTimeout(300);

      const floatingWindow = page.locator('.flexlayout__floating_window');
      await expect(floatingWindow).toBeVisible();
    }
  });

  test('context menu closes on outside click', async ({ page }) => {
    await page.goto(baseURL + '?layout=simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tabButton = findTabButton(page, '/ts0', 0);
    await tabButton.click({ button: 'right' });
    await page.waitForTimeout(200);

    const contextMenu = page.locator('.flexlayout__popup_menu');
    await expect(contextMenu).toBeVisible();

    await page.mouse.click(10, 10);
    await page.waitForTimeout(200);

    await expect(contextMenu).not.toBeVisible();
  });

  test('custom context menu items from callback', async ({ page }) => {
    await page.goto(baseURL + '?layout=context_menu_custom');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tabButton = findTabButton(page, '/ts0', 0);
    await tabButton.click({ button: 'right' });
    await page.waitForTimeout(200);

    const contextMenu = page.locator('.flexlayout__popup_menu');
    await expect(contextMenu).toBeVisible();

    const menuItems = contextMenu.locator('.flexlayout__popup_menu_item');
    const itemTexts = await menuItems.allTextContents();

    expect(itemTexts).toContain('Custom Action');
  });
});
