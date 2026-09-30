import { test, expect } from '@playwright/test';

test('the real review preview keeps two hunks and attaches a line comment', async ({ page }) => {
  await page.goto('/review-harness.html');
  const headers = page.getByTestId('diff-hunk-toggle');
  await expect(headers).toHaveCount(2);
  await expect(headers.first()).not.toContainText('@@');
  await headers.first().click();
  await expect(headers.first()).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByTestId('diff-line-31')).toBeVisible();
  await headers.first().click();
  await page.getByTestId('diff-line-17').click();
  await page.getByTestId('diff-comment-input').fill('Should this timeout be configurable?');
  await page.getByTestId('diff-comment-submit').click();
  await expect(page.getByTestId('diff-comment')).toContainText(
    'Should this timeout be configurable?',
  );
  await expect(page.getByTestId('diff-comment')).toContainText('In the composer');
  await page.getByTestId('diff-layout-split').click();
  await expect(headers).toHaveCount(4);
  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});
