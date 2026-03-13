import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION } from './helpers/fixtures';

test.describe('Placeholder panels for missing tab types', () => {
  test.beforeEach(async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION] });
    // Mock layout endpoint
    await page.route('**/api/layout', async (route) => {
      if (route.request().method() === 'GET') {
        await route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
        return;
      }
      await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
    });
    await page.goto('/');
    await page.waitForTimeout(500);
    // Expand left panel (collapsed by default) so tabs are visible
    await page.evaluate(() => {
      (window as any).__windowActions?.setEdgePanelCollapsed('left', false);
    });
    await page.waitForTimeout(200);
    // Wait for the left edge panel tabbar to be visible
    await expect(page.locator('[data-testid="edge-tabbar-left"]')).toBeVisible({ timeout: 5000 });
  });


  test('explorer tab shows placeholder content', async ({ page }) => {


    const explorerTab = page.locator('[data-tab-id="explorer-tab"]');
    await expect(explorerTab).toBeVisible({ timeout: 5000 });
    await explorerTab.click();

    const panelContent = page.locator('[data-testid="panel-content-explorer"]');
    await expect(panelContent).toBeVisible({ timeout: 5000 });
    await expect(panelContent).toContainText('Coming soon');
    await expect(panelContent).toContainText('Explorer');
  });

  test('search tab shows placeholder content', async ({ page }) => {


    const searchTab = page.locator('[data-tab-id="search-tab"]');
    await expect(searchTab).toBeVisible({ timeout: 5000 });
    await searchTab.click();

    const panelContent = page.locator('[data-testid="panel-content-search"]');
    await expect(panelContent).toBeVisible({ timeout: 5000 });
    await expect(panelContent).toContainText('Coming soon');
    await expect(panelContent).toContainText('Search');
  });

  test('source-control tab shows placeholder content', async ({ page }) => {


    const sourceControlTab = page.locator('[data-tab-id="source-control-tab"]');
    await expect(sourceControlTab).toBeVisible({ timeout: 5000 });
    await sourceControlTab.click();

    const panelContent = page.locator('[data-testid="panel-content-source-control"]');
    await expect(panelContent).toBeVisible({ timeout: 5000 });
    await expect(panelContent).toContainText('Coming soon');
    await expect(panelContent).toContainText('Source Control');
  });

  test('outline tab shows placeholder content', async ({ page }) => {
    // Expand right panel first
    await page.evaluate(() => {
      (window as any).__windowActions?.setEdgePanelCollapsed('right', false);
    });
    await page.waitForTimeout(200);

    const outlineTab = page.locator('[data-tab-id="outline-tab"]');
    await expect(outlineTab).toBeVisible({ timeout: 5000 });
    await outlineTab.click();

    const panelContent = page.locator('[data-testid="panel-content-outline"]');
    await expect(panelContent).toBeVisible({ timeout: 5000 });
    await expect(panelContent).toContainText('Coming soon');
    await expect(panelContent).toContainText('Outline');
  });

  test('problems tab shows placeholder content', async ({ page }) => {
    // Expand bottom panel first
    await page.evaluate(() => {
      (window as any).__windowActions?.setEdgePanelCollapsed('bottom', false);
    });
    await page.waitForTimeout(200);

    const problemsTab = page.locator('[data-tab-id="problems-tab"]');
    await expect(problemsTab).toBeVisible({ timeout: 5000 });
    await problemsTab.click();

    const panelContent = page.locator('[data-testid="panel-content-problems"]');
    await expect(panelContent).toBeVisible({ timeout: 5000 });
    await expect(panelContent).toContainText('Coming soon');
    await expect(panelContent).toContainText('Problems');
  });

  test('output tab shows placeholder content', async ({ page }) => {
    // Expand bottom panel first
    await page.evaluate(() => {
      (window as any).__windowActions?.setEdgePanelCollapsed('bottom', false);
    });
    await page.waitForTimeout(200);

    const outputTab = page.locator('[data-tab-id="output-tab"]');
    await expect(outputTab).toBeVisible({ timeout: 5000 });
    await outputTab.click();

    const panelContent = page.locator('[data-testid="panel-content-output"]');
    await expect(panelContent).toBeVisible({ timeout: 5000 });
    await expect(panelContent).toContainText('Coming soon');
    await expect(panelContent).toContainText('Output');
  });
});
