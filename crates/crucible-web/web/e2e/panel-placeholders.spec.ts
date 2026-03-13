import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION } from './helpers/fixtures';

async function waitForApp(page: Page) {
  await setupBasicMocks(page, { sessions: [MOCK_SESSION] });
  await page.route('**/api/layout', async (route) => {
    const method = route.request().method();
    if (method === 'GET') {
      await route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
      return;
    }
    if (method === 'POST' || method === 'DELETE') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
      return;
    }
    await route.continue();
  });
  await page.goto('/');
  await page.waitForTimeout(500);
}

test.describe('Placeholder panels for missing tab types', () => {
  test.beforeEach(async ({ page }) => {
    await waitForApp(page);
    const sessionItem = page.getByTestId('session-item-test-session-001');
    await expect(sessionItem).toBeVisible({ timeout: 5000 });
    await sessionItem.click();
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toBeVisible({ timeout: 5000 });
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
    const outlineTab = page.locator('[data-tab-id="outline-tab"]');
    await expect(outlineTab).toBeVisible({ timeout: 5000 });
    await outlineTab.click();

    const panelContent = page.locator('[data-testid="panel-content-outline"]');
    await expect(panelContent).toBeVisible({ timeout: 5000 });
    await expect(panelContent).toContainText('Coming soon');
    await expect(panelContent).toContainText('Outline');
  });

  test('problems tab shows placeholder content', async ({ page }) => {
    const problemsTab = page.locator('[data-tab-id="problems-tab"]');
    await expect(problemsTab).toBeVisible({ timeout: 5000 });
    await problemsTab.click();

    const panelContent = page.locator('[data-testid="panel-content-problems"]');
    await expect(panelContent).toBeVisible({ timeout: 5000 });
    await expect(panelContent).toContainText('Coming soon');
    await expect(panelContent).toContainText('Problems');
  });

  test('output tab shows placeholder content', async ({ page }) => {
    const outputTab = page.locator('[data-tab-id="output-tab"]');
    await expect(outputTab).toBeVisible({ timeout: 5000 });
    await outputTab.click();

    const panelContent = page.locator('[data-testid="panel-content-output"]');
    await expect(panelContent).toBeVisible({ timeout: 5000 });
    await expect(panelContent).toContainText('Coming soon');
    await expect(panelContent).toContainText('Output');
  });
});
