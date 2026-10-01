import { expect, test } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openSession } from './helpers/nav';
import { MOCK_SESSION } from './helpers/fixtures';

test('production permission previews scroll without hiding decisions in short viewports', async ({ page }) => {
  const oldContent = Array.from({ length: 45 }, (_, i) => `const value${i} = "old";`).join('\n');
  const newContent = oldContent.replaceAll('"old"', '"new"');
  await page.setViewportSize({ width: 1600, height: 960 });
  await setupBasicMocks(page, { sseEvents: [{
    type: 'interaction_requested',
    data: { event: 'interaction_requested', data: {
      request_id: 'permission-layout',
      request: { kind: 'permission', action: { type: 'write', segments: ['greeting.ts'] }, pattern: 'greeting.ts', diffs: [{ path: 'greeting.ts', old_content: oldContent, new_content: newContent }] },
    } },
  }] });
  await page.goto('/');
  await appReady(page);
  await openSession(page, MOCK_SESSION.session_id);
  const preview = page.locator('.permission-preview');
  await expect(preview).toBeVisible();
  await expect(preview.locator('.diff-row.add .diff-word').first()).toBeVisible();
  expect((await preview.boundingBox())!.height).toBeGreaterThan(192);
  await page.setViewportSize({ width: 1600, height: 500 });
  await expect.poll(async () => (await preview.boundingBox())!.height).toBeLessThanOrEqual(200);
  expect(await preview.evaluate(el => getComputedStyle(el).overflowY)).toBe('auto');
  await expect(page.getByRole('button', { name: 'Allow', exact: true })).toBeInViewport();
  await expect(page.getByRole('button', { name: 'Deny', exact: true })).toBeInViewport();
});
