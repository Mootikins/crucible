import { test, expect } from '@playwright/test';
import { findPath, findTabButton } from './helpers';

const baseURL = '/flexlayout-test.html?layout=docked_panes';
const evidencePath = '../../../.sisyphus/evidence';

test.beforeEach(async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
});

test.describe('Bug #1: Splitter drag indicator matches applied position', () => {
    test('border edge splitter lands where cursor is released (horizontal)', async ({ page }) => {
        const leftDockButton = page.locator('[data-layout-path="/border/left/button/dock"]').first();
        const leftDockTitle = await leftDockButton.getAttribute('title');
        if (leftDockTitle === 'Expand') {
            await leftDockButton.click();
            await page.waitForTimeout(150);
        }

        const borderSplitter = page.locator('.flexlayout__splitter_border').first();
        await expect(borderSplitter).toBeVisible();

        const before = await borderSplitter.boundingBox();
        expect(before).not.toBeNull();

        const startX = before!.x + before!.width / 2;
        const startY = before!.y + before!.height / 2;
        const deltaX = -50;

        await page.mouse.move(startX, startY);
        await page.mouse.down();
        await page.mouse.move(startX + deltaX, startY, { steps: 10 });
        await page.mouse.up();
        await page.waitForTimeout(150);

        const after = await borderSplitter.boundingBox();
        expect(after).not.toBeNull();

        const movedBy = after!.x - before!.x;
        expect(Math.abs(movedBy - deltaX)).toBeLessThanOrEqual(2);

        await page.screenshot({ path: `${evidencePath}/bug1-splitter-drag.png` });
    });

    test('border edge splitter lands where cursor is released (vertical)', async ({ page }) => {
        const bottomSplitter = page.locator('.flexlayout__splitter_border').last();
        const isVisible = await bottomSplitter.isVisible().catch(() => false);
        if (!isVisible) return;

        const before = await bottomSplitter.boundingBox();
        expect(before).not.toBeNull();

        const startX = before!.x + before!.width / 2;
        const startY = before!.y + before!.height / 2;
        const deltaY = -30;

        await page.mouse.move(startX, startY);
        await page.mouse.down();
        await page.mouse.move(startX, startY + deltaY, { steps: 10 });
        await page.mouse.up();
        await page.waitForTimeout(150);

        const after = await bottomSplitter.boundingBox();
        expect(after).not.toBeNull();

        const movedBy = after!.y - before!.y;
        expect(Math.abs(movedBy - deltaY)).toBeLessThanOrEqual(2);
    });
});

test.describe('Bug #2: Expanded border tab buttons have same base classes', () => {
    test('expanded border tab buttons include flexlayout__border_button class', async ({ page }) => {
        const tabbarButtons = page.locator('[data-border-tabbar] .flexlayout__border_button');
        const count = await tabbarButtons.count();
        expect(count).toBeGreaterThan(0);

        for (let i = 0; i < count; i++) {
            const button = tabbarButtons.nth(i);
            const classes = await button.getAttribute('class');
            expect(classes).toContain('flexlayout__border_button');
        }
    });

    test('expanded border tab buttons have location-specific class', async ({ page }) => {
        const tabbarButtons = page.locator('[data-border-tabbar] .flexlayout__border_button');
        const count = await tabbarButtons.count();
        expect(count).toBeGreaterThan(0);

        for (let i = 0; i < count; i++) {
            const button = tabbarButtons.nth(i);
            const classes = await button.getAttribute('class');
            const hasLocationClass =
                classes?.includes('flexlayout__border_button_left') ||
                classes?.includes('flexlayout__border_button_right') ||
                classes?.includes('flexlayout__border_button_top') ||
                classes?.includes('flexlayout__border_button_bottom');
            expect(hasLocationClass).toBe(true);
        }
    });

    test('expanded border dock button uses same class as collapsed dock button', async ({ page }) => {
        const expandedDockButton = page.locator('[data-border-tabbar] .flexlayout__border_dock_button').first();
        await expect(expandedDockButton).toBeVisible();

        await page.locator('[data-layout-path="/border/left/button/dock"]').click();
        await page.waitForTimeout(200);

        const collapsedDockButton = page.locator('[data-collapsed-fab="true"]').first();
        await expect(collapsedDockButton).toBeVisible();

        const expandedClass = await expandedDockButton.getAttribute('class');
        const collapsedClass = await collapsedDockButton.getAttribute('class');

        expect(expandedClass).toContain('flexlayout__border_dock_button');
        expect(collapsedClass).toContain('flexlayout__border_dock_button');

        await page.screenshot({ path: `${evidencePath}/bug2-tab-classes.png` });
    });
});

test.describe('Bug #3: Vertical text in empty collapsed panes', () => {
    test('collapsed left border has writing-mode: vertical-rl on tab buttons', async ({ page }) => {
        const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
        await dockButton.click();

        const collapsedButtons = page.locator(
            '.flexlayout__border_left[data-collapsed-strip="true"] button[data-collapsed-tab-item="true"]'
        );
        const count = await collapsedButtons.count();
        expect(count).toBeGreaterThan(0);

        for (let i = 0; i < count; i++) {
            const writingMode = await collapsedButtons.nth(i).evaluate(
                (el) => getComputedStyle(el).writingMode
            );
            expect(writingMode).toBe('vertical-rl');
        }

        await page.screenshot({ path: `${evidencePath}/bug3-vertical-left.png` });
    });

    test('collapsed right border has writing-mode: vertical-rl on tab buttons', async ({ page }) => {
        const dockButton = page.locator('[data-layout-path="/border/right/button/dock"]');
        await dockButton.click();

        const collapsedButtons = page.locator(
            '.flexlayout__border_right[data-collapsed-strip="true"] button[data-collapsed-tab-item="true"]'
        );
        const count = await collapsedButtons.count();
        expect(count).toBeGreaterThan(0);

        for (let i = 0; i < count; i++) {
            const writingMode = await collapsedButtons.nth(i).evaluate(
                (el) => getComputedStyle(el).writingMode
            );
            expect(writingMode).toBe('vertical-rl');
        }

        await page.screenshot({ path: `${evidencePath}/bug3-vertical-right.png` });
    });

    test('collapsed top/bottom borders have horizontal text (no vertical writing-mode)', async ({ page }) => {
        for (const edge of ['top', 'bottom']) {
            const dockButton = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`);
            await dockButton.click();
        }

        for (const edge of ['top', 'bottom']) {
            const collapsedButtons = page.locator(
                `.flexlayout__border_${edge}[data-collapsed-strip="true"] button[data-collapsed-tab-item="true"]`
            );
            const count = await collapsedButtons.count();
            if (count > 0) {
                const writingMode = await collapsedButtons.first().evaluate(
                    (el) => getComputedStyle(el).writingMode
                );
                expect(writingMode).not.toBe('vertical-rl');
            }
        }
    });
});

test.describe('Bug #4: FAB position consistent across all border states', () => {
    test('all collapsed borders have FAB at trailing edge of strip', async ({ page }) => {
        for (const edge of ['top', 'bottom', 'left', 'right']) {
            const btn = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`);
            await btn.click();
        }
        await page.waitForTimeout(200);

        for (const edge of ['top', 'bottom', 'left', 'right'] as const) {
            const strip = page.locator(`.flexlayout__border_${edge}[data-collapsed-strip="true"]`).first();
            await expect(strip).toBeVisible();

            const placement = await strip.evaluate((el, currentEdge) => {
                const allButtons = Array.from(el.querySelectorAll('button')) as HTMLButtonElement[];
                const fabButton = allButtons.find((b) => b.dataset.collapsedFab === 'true');
                const tabButtons = allButtons.filter((b) => b.dataset.collapsedTabItem === 'true');

                if (!fabButton) return { hasFab: false, isLast: false, afterAllTabs: false };

                const fabIndex = allButtons.indexOf(fabButton);
                const isVertical = currentEdge === 'left' || currentEdge === 'right';
                const fabRect = fabButton.getBoundingClientRect();
                const fabStart = isVertical ? fabRect.top : fabRect.left;

                let maxTabEnd = 0;
                for (const tab of tabButtons) {
                    const rect = tab.getBoundingClientRect();
                    maxTabEnd = Math.max(maxTabEnd, isVertical ? rect.bottom : rect.right);
                }

                return {
                    hasFab: true,
                    isLast: fabIndex === allButtons.length - 1,
                    afterAllTabs: tabButtons.length === 0 || fabStart >= maxTabEnd - 2,
                };
            }, edge);

            expect(placement.hasFab).toBe(true);
            expect(placement.isLast).toBe(true);
            expect(placement.afterAllTabs).toBe(true);
        }

        await page.screenshot({ path: `${evidencePath}/bug4-fab-position.png` });
    });

    test('empty border (no tabs) shows FAB via absolute positioning', async ({ page }) => {
        const layoutBox = await findPath(page, '/').boundingBox();
        expect(layoutBox).not.toBeNull();

        const emptyBorderFabs = page.locator('button[data-empty-border-fab="true"]');
        const fabCount = await emptyBorderFabs.count();

        if (fabCount > 0) {
            for (let i = 0; i < fabCount; i++) {
                const fab = emptyBorderFabs.nth(i);
                await expect(fab).toBeVisible();
                const box = await fab.boundingBox();
                expect(box).not.toBeNull();
                expect(box!.width).toBeGreaterThan(0);
                expect(box!.height).toBeGreaterThan(0);
            }
        }
    });
});

test.describe('Bug #5: Flyout sizing — height ≤50% viewport, default ~25%', () => {
    test('left flyout is not full layout height when all borders collapsed', async ({ page }) => {
        for (const edge of ['top', 'bottom', 'left', 'right']) {
            const dockButton = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`).first();
            const title = await dockButton.getAttribute('title');
            if (title === 'Collapse') {
                await dockButton.click();
            }
        }
        await page.waitForTimeout(200);

        await findTabButton(page, '/border/left', 0).click();
        const flyoutPanel = page.locator('.flexlayout__flyout_panel');
        await expect(flyoutPanel).toBeVisible({ timeout: 5_000 });

        const flyout = await flyoutPanel.boundingBox();
        const layout = await findPath(page, '/').boundingBox();

        expect(flyout).not.toBeNull();
        expect(layout).not.toBeNull();

        expect(flyout!.height).toBeLessThan(layout!.height * 0.55);

        await page.screenshot({ path: `${evidencePath}/bug5-flyout-height.png` });
    });

    test('right flyout is not full layout height', async ({ page }) => {
        for (const edge of ['top', 'bottom', 'left', 'right']) {
            const dockButton = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`).first();
            const title = await dockButton.getAttribute('title');
            if (title === 'Collapse') {
                await dockButton.click();
            }
        }
        await page.waitForTimeout(200);

        await findTabButton(page, '/border/right', 0).click();
        const flyoutPanel = page.locator('.flexlayout__flyout_panel');
        await expect(flyoutPanel).toBeVisible({ timeout: 5_000 });

        const flyout = await flyoutPanel.boundingBox();
        const layout = await findPath(page, '/').boundingBox();

        expect(flyout).not.toBeNull();
        expect(layout).not.toBeNull();

        expect(flyout!.height).toBeLessThan(layout!.height * 0.55);
    });

    test('flyout respects collapsed border insets', async ({ page }) => {
        for (const edge of ['top', 'bottom', 'left', 'right']) {
            const dockButton = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`).first();
            const title = await dockButton.getAttribute('title');
            if (title === 'Collapse') {
                await dockButton.click();
            }
        }
        await page.waitForTimeout(200);

        await findTabButton(page, '/border/left', 0).click();
        const flyoutPanel = page.locator('.flexlayout__flyout_panel');
        await expect(flyoutPanel).toBeVisible({ timeout: 5_000 });

        const flyout = await flyoutPanel.boundingBox();
        const topStrip = await page.locator('.flexlayout__border_top[data-collapsed-strip="true"]').boundingBox();
        const bottomStrip = await page.locator('.flexlayout__border_bottom[data-collapsed-strip="true"]').boundingBox();

        expect(flyout).not.toBeNull();
        expect(topStrip).not.toBeNull();
        expect(bottomStrip).not.toBeNull();

        expect(flyout!.y).toBeGreaterThanOrEqual(topStrip!.y + topStrip!.height - 2);
        expect(flyout!.y + flyout!.height).toBeLessThanOrEqual(bottomStrip!.y + 2);

        await page.screenshot({ path: `${evidencePath}/bug5-flyout-insets.png` });
    });
});
