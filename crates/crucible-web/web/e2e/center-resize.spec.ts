import { test, expect } from '@playwright/test';

/**
 * E2E: center splitter resize. Verifies that dragging the root splitter
 * changes the first pane width (layout updates and re-renders).
 */
test('center splitter resize updates pane width', async ({ page }) => {
  await page.goto('/');

  const splitter = page.locator('[data-split-id="split-root"]');
  await splitter.waitFor({ state: 'visible' });

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
