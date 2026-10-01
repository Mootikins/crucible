import { expect, test } from '@playwright/test';

test('mock note hover shows content and cleans up its transient tab', async ({ page }) => {
  await page.clock.install();
  await page.goto('/shell-mockup.html');
  await page.locator('[data-note="The Knowledge Graph"]').first().hover();
  const popup = page.locator('[data-window-id]');
  await expect(popup).toBeVisible();
  await expect(popup.getByText(/mockup carries the text/)).toHaveCount(0);
  await popup.hover();
  await page.clock.fastForward(800);
  await expect(popup).toBeVisible();
  await page.mouse.move(400, 20);
  await expect(popup).toHaveCount(0);
  const orphaned = await page.evaluate(async () => {
    // @ts-expect-error Vite serves the core module for the mockup test.
    const { windowStore } = await import('/src/windowing/store/index.ts');
    return Object.values(windowStore.tabGroups as Record<string, { tabs: { id: string }[] }>).some((group) =>
      group.tabs.some((tab) => tab.id.startsWith('hover:')),
    );
  });
  expect(orphaned).toBe(false);
});

test('a pinned mock hover stays open after leaving its link', async ({ page }) => {
  await page.clock.install();
  await page.goto('/shell-mockup.html');
  await page.locator('[data-note="The Knowledge Graph"]').first().hover();
  const popup = page.locator('[data-window-id]');
  await expect(popup).toBeVisible();
  await popup.getByRole('button', { name: 'Pin: keep it open, with all its tools' }).click();
  await page.mouse.move(400, 20);
  await page.clock.fastForward(850);
  await expect(popup).toBeVisible();
});
