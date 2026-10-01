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

test('permission previews use the same diff rows and word emphasis as file buffers', async ({ page }) => {
  await page.goto('/shell-mockup.html?review');
  const permission = page.getByRole('alertdialog', { name: 'Permission request' });
  await expect(permission.locator('.add .mk-diff-word').first()).toBeVisible();
  const styles = (el: Element) => {
    const s = getComputedStyle(el);
    return { font: s.fontSize, color: s.color, background: s.backgroundColor, gutter: s.borderLeftColor, columns: s.gridTemplateColumns.split(' ').slice(0, 3) };
  };
  const promptRow = await permission.locator('.mk-diff-row.add').first().evaluate(styles);
  const bufferRow = await page.getByTestId('mock-review').locator('.mk-diff-row.add').first().evaluate(styles);
  expect(promptRow).toEqual(bufferRow);
  const background = await permission.locator('.mk-pdiff').evaluate(el => getComputedStyle(el).backgroundColor);
  expect(background).not.toBe(await permission.evaluate(el => getComputedStyle(el).backgroundColor));
});

test('permission diffs show more lines before scrolling and fit shorter viewports', async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 960 });
  await page.goto('/shell-mockup.html?review');
  const prompt = page.getByRole('alertdialog', { name: 'Permission request' });
  const rows = prompt.locator('.mk-diff-rows');
  expect((await rows.boundingBox())!.height).toBeGreaterThan(192);
  await page.setViewportSize({ width: 1600, height: 500 });
  const sizing = await rows.evaluate(el => {
    const s = getComputedStyle(el);
    return { min: parseFloat(s.minHeight), max: parseFloat(s.maxHeight), height: el.getBoundingClientRect().height, overflow: s.overflowY };
  });
  expect(sizing.min).toBeGreaterThan(0);
  expect(sizing.min).toBeLessThanOrEqual(sizing.max);
  expect(sizing.height).toBeLessThanOrEqual(200);
  expect(sizing.overflow).toBe('auto');
  const allow = await prompt.getByRole('button', { name: 'Allow', exact: true }).boundingBox();
  expect(allow!.y).toBeGreaterThanOrEqual(0);
  expect(allow!.y + allow!.height).toBeLessThanOrEqual(500);
});
