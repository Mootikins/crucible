import { test, expect } from '@playwright/test';
import { findPath, checkTab, dragSplitter } from './helpers';

test.describe('Fix #3 HIGH: TabButton renderContent is reactive', () => {
    test('onRenderTab leading icon persists after actions', async ({ page }) => {
        await page.goto('/flexlayout-test.html?layout=test_with_onRenderTab');

        const tab = findPath(page, '/ts1/tb0');
        await expect(tab.locator('.flexlayout__tab_button_leading img')).toBeVisible();
        await expect(tab.locator('img[src="images/folder.svg"]')).toBeVisible();

        await findPath(page, '/ts2/tb0').click();
        await findPath(page, '/ts1/tb0').click();

        await expect(tab.locator('.flexlayout__tab_button_leading img')).toBeVisible();
        await expect(tab.locator('img[src="images/folder.svg"]')).toBeVisible();
    });
});

test.describe('Fix #7 MEDIUM: Popup menu features', () => {
    async function openOverflowPopup(page: any) {
        await page.goto('/flexlayout-test.html?layout=test_with_borders');
        await findPath(page, '/ts0/tabstrip').click();
        await page.locator('[data-id=add-active]').click();
        await page.locator('[data-id=add-active]').click();

        const splitter = findPath(page, '/s0');
        await dragSplitter(page, splitter, false, -1000);
        await dragSplitter(page, splitter, false, 150);

        await expect(findPath(page, '/ts0/button/overflow')).toBeVisible();
        await findPath(page, '/ts0/button/overflow').click();
        await expect(findPath(page, '/popup-menu')).toBeVisible();
    }

    test('popup menu closes on Escape key', async ({ page }) => {
        await openOverflowPopup(page);
        await page.keyboard.press('Escape');
        await expect(findPath(page, '/popup-menu')).not.toBeVisible();
    });

    test('popup menu items show tab name text', async ({ page }) => {
        await openOverflowPopup(page);
        const items = page.locator('[data-layout-path^="/popup-menu/tb"]');
        const count = await items.count();
        expect(count).toBeGreaterThan(0);
        for (let i = 0; i < count; i++) {
            const text = await items.nth(i).textContent();
            expect(text?.trim().length).toBeGreaterThan(0);
        }
    });
});

test.describe('Fix #2 & #4: Layout context correctness', () => {
    test('layout functions correctly after model reload', async ({ page }) => {
        await page.goto('/flexlayout-test.html?layout=test_two_tabs');

        await checkTab(page, '/ts0', 0, true, 'One');
        await checkTab(page, '/ts1', 0, true, 'Two');

        await page.locator('[data-id=reload]').click();

        await checkTab(page, '/ts0', 0, true, 'One');
        await checkTab(page, '/ts1', 0, true, 'Two');

        await findPath(page, '/ts0/tabstrip').click();
        await findPath(page, '/ts1/tb0').click();

        await checkTab(page, '/ts0', 0, true, 'One');
    });
});
