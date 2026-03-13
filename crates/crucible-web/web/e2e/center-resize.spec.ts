import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';


/**
 * E2E: center splitter resize. Verifies that dragging the root splitter
 * changes the first pane width (layout updates and re-renders).
 */
test('center splitter resize updates pane width', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');

  const sessionItem = page.getByTestId('session-item-test-session-001');
  await expect(sessionItem).toBeVisible({ timeout: 5000 });
  await sessionItem.click();
  await expect(page.locator('[data-tab-id^="tab-chat-"]')).toBeVisible({ timeout: 5000 });

  await page.evaluate(async () => {
    // @ts-expect-error - Vite dev server runtime path import for browser context
    const { windowStore, windowActions } = await import('/src/stores/windowStore.ts');
    const layout = windowStore.layout;
    if (layout.type === 'pane') {
      windowActions.splitPane(layout.id, 'horizontal');
    }
  });
  await page.waitForTimeout(200);

  const splitter = page.locator('[data-split-id]').first();
  await splitter.waitFor({ state: 'visible', timeout: 3000 });

  const container = splitter.locator('..');
  const firstPane = container.locator('> div').first();

  const widthBefore = await firstPane.evaluate((el) => el.getBoundingClientRect().width);

  const box = await splitter.boundingBox();
  expect(box).toBeTruthy();
  const cx = box!.x + box!.width / 2;
  const cy = box!.y + box!.height / 2;

  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 80, cy, { steps: 5 });
  await page.mouse.up();

  const widthAfter = await firstPane.evaluate((el) => el.getBoundingClientRect().width);

  expect(widthAfter).toBeGreaterThan(widthBefore);
});
