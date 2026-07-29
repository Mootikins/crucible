import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { openSessionsList } from './helpers/nav';
import { stableCenter } from './helpers/geometry';


/**
 * E2E: center splitter resize. Verifies that dragging the root splitter
 * changes the first pane width (layout updates and re-renders).
 */
test('center splitter resize updates pane width', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await openSessionsList(page);

  const sessionItem = page.getByTestId('session-item-test-session-001');
  await expect(sessionItem).toBeVisible({ timeout: 5000 });
  await sessionItem.click();
  await expect(page.locator('[data-tab-id^="tab-chat-"]')).toBeVisible({ timeout: 5000 });

  await page.evaluate(() => {
    const windowStore = (window as unknown as Record<string, unknown>).__windowStore as { layout: { type: string; id: string } };
    const windowActions = (window as unknown as Record<string, unknown>).__windowActions as { splitPane: (id: string, dir: string) => void };
    const layout = windowStore.layout;
    if (layout.type === 'pane') {
      windowActions.splitPane(layout.id, 'horizontal');
    }
  });

  const splitter = page.locator('[data-split-id]').first();
  await splitter.waitFor({ state: 'visible', timeout: 3000 });

  const container = splitter.locator('..');
  const firstPane = container.locator('> div').first();

  // Settle BEFORE measuring. Opening the session docks a chat in the right
  // edge panel, which expands and squeezes the center container (687px→397px
  // here). A `widthBefore` captured mid-expansion is compared against a
  // post-expansion `widthAfter`, so both panes have shrunk and the assertion
  // fails however the drag went.
  const { x: cx, y: cy } = await stableCenter(splitter);

  const widthBefore = await firstPane.evaluate((el) => el.getBoundingClientRect().width);

  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 80, cy, { steps: 5 });
  await page.mouse.up();

  const widthAfter = await firstPane.evaluate((el) => el.getBoundingClientRect().width);

  expect(widthAfter).toBeGreaterThan(widthBefore);
});
