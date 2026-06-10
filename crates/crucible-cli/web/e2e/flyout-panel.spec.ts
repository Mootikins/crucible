import { test, expect, type Page } from '@playwright/test';

type PanelPosition = 'left' | 'right' | 'bottom';

const HIDE_TITLES: Record<PanelPosition, string> = {
  left: 'Hide Left Panel',
  right: 'Hide Right Panel',
  bottom: 'Hide Bottom Panel',
};

async function collapsePanel(page: Page, position: PanelPosition) {
  const toggle = page.locator(`button[title="${HIDE_TITLES[position]}"]`);
  if (await toggle.isVisible()) await toggle.click();
}

async function openFlyoutFromCollapsed(page: Page, position: PanelPosition) {
  await collapsePanel(page, position);
  const btn = page.locator(`[data-testid="collapsed-tab-button-${position}"]`).first();
  await btn.waitFor({ state: 'visible', timeout: 3000 });
  const btnBox = await btn.boundingBox();
  expect(btnBox).toBeTruthy();
  await btn.click();
  const flyout = page.locator('[data-testid="flyout-panel"]');
  await flyout.waitFor({ state: 'visible', timeout: 2000 });
  const flyoutBox = await flyout.boundingBox();
  expect(flyoutBox).toBeTruthy();
  return { btnBox: btnBox!, flyoutBox: flyoutBox!, flyout };
}

test.describe('FlyoutPanel anchored popover', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(500);
  });

  test('left panel flyout appears to the right of collapsed strip', async ({ page }) => {
    const { btnBox, flyoutBox } = await openFlyoutFromCollapsed(page, 'left');

    expect(flyoutBox.x).toBeGreaterThanOrEqual(btnBox.x);
    const flyoutCenterX = flyoutBox.x + flyoutBox.width / 2;
    expect(flyoutCenterX).toBeLessThan(page.viewportSize()!.width / 2);
  });

  test('right panel flyout appears to the left of collapsed strip', async ({ page }) => {
    const { btnBox, flyoutBox } = await openFlyoutFromCollapsed(page, 'right');

    const flyoutRight = flyoutBox.x + flyoutBox.width;
    expect(flyoutRight).toBeLessThanOrEqual(btnBox.x + btnBox.width + 2);
    const flyoutCenterX = flyoutBox.x + flyoutBox.width / 2;
    expect(flyoutCenterX).toBeGreaterThan(page.viewportSize()!.width / 2);
  });

  test('bottom panel flyout appears above collapsed strip', async ({ page }) => {
    const { btnBox, flyoutBox } = await openFlyoutFromCollapsed(page, 'bottom');

    const flyoutBottom = flyoutBox.y + flyoutBox.height;
    expect(flyoutBottom).toBeLessThanOrEqual(btnBox.y + 2);
  });

  test('Escape key closes flyout', async ({ page }) => {
    const { flyout } = await openFlyoutFromCollapsed(page, 'left');
    await page.keyboard.press('Escape');
    await expect(flyout).not.toBeVisible({ timeout: 2000 });
  });

  test('clicking outside flyout closes it', async ({ page }) => {
    const { flyout } = await openFlyoutFromCollapsed(page, 'left');
    const { width, height } = page.viewportSize()!;
    await page.mouse.click(width / 2, height / 2);
    await expect(flyout).not.toBeVisible({ timeout: 2000 });
  });
});
