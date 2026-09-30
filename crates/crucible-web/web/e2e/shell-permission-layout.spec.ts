import { expect, test } from '@playwright/test';

test('permission acceptance keeps copy controls at the end of the assistant turn', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: async (text: string) => { Object.assign(window, { copiedResponse: text }); } } });
  });
  await page.goto('/shell-mockup.html?review');
  await page.getByRole('button', { name: 'Allow', exact: true }).click();
  const interim = page.locator('.mk-aturn').filter({ hasText: 'I will add it after Checking Current Settings.' });
  await expect(interim).toBeVisible();
  await expect(interim.locator('.mk-turnacts')).toHaveCount(0);
  const final = page.locator('.mk-aturn').filter({ hasText: 'Added When to turn it off under Configuration.' });
  await expect(final.getByRole('button', { name: 'Copy response', exact: true })).toBeVisible();
  await expect(final.getByRole('button', { name: 'Regenerate response', exact: true })).toBeVisible();
  await final.getByRole('button', { name: 'Copy response', exact: true }).click();
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'copiedResponse'))).toBe('I will add it after **Checking Current Settings**.\n\nAdded **When to turn it off** under Configuration.');
});
