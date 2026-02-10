import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  drag,
  Location,
} from './helpers';

const baseURL = '/flexlayout-test.html';

test.describe('Drop Validation', () => {
  test('drop validation callback blocks invalid drops', async ({ page }) => {
    await page.goto(baseURL + '?layout=drop_validation');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const initialTabCount = await findAllTabSets(page).count();
    const sourceTab = findTabButton(page, '/ts0', 0);
    const targetTabSet = findPath(page, '/ts1');

    await drag(page, sourceTab, targetTabSet, Location.CENTER);
    await page.waitForTimeout(300);

    const afterTabCount = await findAllTabSets(page).count();
    expect(afterTabCount).toBe(initialTabCount);

    await expect(sourceTab).toBeVisible();
  });

  test('drop validation allows valid drops', async ({ page }) => {
    await page.goto(baseURL + '?layout=drop_validation_allow');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const sourceTab = findTabButton(page, '/ts0', 0);
    const targetTabSet = findPath(page, '/ts1');

    const initialTs0Count = await findPath(page, '/ts0').locator('.flexlayout__tab_button').count();
    const initialTs1Count = await findPath(page, '/ts1').locator('.flexlayout__tab_button').count();

    await drag(page, sourceTab, targetTabSet, Location.CENTER);
    await page.waitForTimeout(300);

    const afterTs0Count = await findPath(page, '/ts0').locator('.flexlayout__tab_button').count();
    const afterTs1Count = await findPath(page, '/ts1').locator('.flexlayout__tab_button').count();

    expect(afterTs0Count).toBe(initialTs0Count - 1);
    expect(afterTs1Count).toBe(initialTs1Count + 1);
  });

  test('drop validation prevents border drops when blocked', async ({ page }) => {
    await page.goto(baseURL + '?layout=drop_validation_border');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const sourceTab = findTabButton(page, '/ts0', 0);
    const borderStrip = findPath(page, '/border_left');

    await drag(page, sourceTab, borderStrip, Location.CENTER);
    await page.waitForTimeout(300);

    await expect(sourceTab).toBeVisible();
    const borderTabs = borderStrip.locator('.flexlayout__border_button');
    expect(await borderTabs.count()).toBe(0);
  });

  test('drop validation allows border drops when permitted', async ({ page }) => {
    await page.goto(baseURL + '?layout=drop_validation_border_allow');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const sourceTab = findTabButton(page, '/ts0', 0);
    const borderStrip = findPath(page, '/border_left');

    const initialBorderCount = await borderStrip.locator('.flexlayout__border_button').count();

    await drag(page, sourceTab, borderStrip, Location.CENTER);
    await page.waitForTimeout(300);

    const afterBorderCount = await borderStrip.locator('.flexlayout__border_button').count();
    expect(afterBorderCount).toBe(initialBorderCount + 1);
  });

  test('drop validation with conditional logic based on tab properties', async ({ page }) => {
    await page.goto(baseURL + '?layout=drop_validation_conditional');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const pinnedTab = findTabButton(page, '/ts0', 0);
    const unpinnedTab = findTabButton(page, '/ts0', 1);
    const targetTabSet = findPath(page, '/ts1');

    await drag(page, pinnedTab, targetTabSet, Location.CENTER);
    await page.waitForTimeout(300);
    await expect(pinnedTab).toBeVisible();

    await drag(page, unpinnedTab, targetTabSet, Location.CENTER);
    await page.waitForTimeout(300);

    const ts1Tabs = targetTabSet.locator('.flexlayout__tab_button');
    expect(await ts1Tabs.count()).toBeGreaterThan(0);
  });

  test('drop validation shows no drop indicator when blocked', async ({ page }) => {
    await page.goto(baseURL + '?layout=drop_validation');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const sourceTab = findTabButton(page, '/ts0', 0);
    const targetTabSet = findPath(page, '/ts1');

    const sourceBox = await sourceTab.boundingBox();
    const targetBox = await targetTabSet.boundingBox();

    if (!sourceBox || !targetBox) throw new Error('Could not get bounding boxes');

    await page.mouse.move(sourceBox.x + sourceBox.width / 2, sourceBox.y + sourceBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(targetBox.x + targetBox.width / 2, targetBox.y + targetBox.height / 2, { steps: 10 });

    const outlineRect = page.locator('.flexlayout__outline_rect');
    const isVisible = await outlineRect.isVisible().catch(() => false);
    expect(isVisible).toBe(false);

    await page.mouse.up();
  });

  test('drop validation allows drop indicator when permitted', async ({ page }) => {
    await page.goto(baseURL + '?layout=drop_validation_allow');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const sourceTab = findTabButton(page, '/ts0', 0);
    const targetTabSet = findPath(page, '/ts1');

    const sourceBox = await sourceTab.boundingBox();
    const targetBox = await targetTabSet.boundingBox();

    if (!sourceBox || !targetBox) throw new Error('Could not get bounding boxes');

    await page.mouse.move(sourceBox.x + sourceBox.width / 2, sourceBox.y + sourceBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(targetBox.x + targetBox.width / 2, targetBox.y + targetBox.height / 2, { steps: 10 });

    const outlineRect = page.locator('.flexlayout__outline_rect');
    await expect(outlineRect).toBeVisible();

    await page.mouse.up();
  });
});
