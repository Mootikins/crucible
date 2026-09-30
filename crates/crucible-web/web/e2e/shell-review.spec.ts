import { expect, test } from '@playwright/test';

test('the mockup separates recorded changes from proposal decisions', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/shell-mockup.html?review');
  const review = page.getByTestId('mock-review');
  await expect(review.getByRole('heading', { name: 'Tighten the knowledge notes' })).toBeVisible();
  await expect(review.getByRole('button', { name: 'Accept all', exact: true })).toBeVisible();
  await review.getByRole('button', { name: 'Comment' }).first().click();
  await review
    .getByRole('textbox', { name: 'Review comment' })
    .fill('Keep the kiln terminology consistent.');
  await review.locator('form').getByRole('button', { name: 'Comment', exact: true }).click();
  await expect(review.getByText('Attached to chat', { exact: true })).toBeVisible();
  await expect(page.getByText('Kilns · comment')).toBeVisible();
  await review.getByRole('button', { name: 'Reject file', exact: true }).first().click();
  await expect(review.getByText('rejected', { exact: true })).toBeVisible();
  await review.getByRole('button', { name: 'Accept all', exact: true }).click();
  await expect(review.getByRole('button', { name: 'Accept all', exact: true })).toBeDisabled();
  await page.locator('[data-tab-id="changes:s1"]').click();
  const changes = page.getByTestId('mock-changes');
  await expect(changes).toBeVisible();
  await expect(changes.getByRole('button', { name: /Accept|Reject|Undo/ })).toHaveCount(0);
  await expect(changes.getByRole('heading', { name: 'Changes', exact: true })).toBeVisible();
  await expect(changes.locator('.mk-review-diff')).toHaveCount(2);
  await expect(page.locator('[data-tab-id="review:record:s1"]')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('file actions stay together when the review pane narrows', async ({ page }) => {
  await page.goto('/shell-mockup.html?review');
  const review = page.getByTestId('mock-review');
  await expect(review).toBeVisible();
  for (const width of [340, 420, 800]) {
    await review.evaluate((el, value) => {
      el.style.width = `${value}px`;
      el.style.maxWidth = '100%';
    }, width);
    for (const head of await review.locator('.mk-review-file-head').all()) {
      const comment = head.getByRole('button', { name: 'Comment', exact: true });
      await expect(comment).toBeVisible();
      const bounds = await Promise.all([
        comment.boundingBox(),
        head.getByRole('button', { name: 'Reject file', exact: true }).boundingBox(),
        head.getByRole('button', { name: 'Accept file', exact: true }).boundingBox(),
      ]);
      const centers = bounds.map((box) => box!.y + box!.height / 2);
      expect(Math.max(...centers) - Math.min(...centers)).toBeLessThan(2);
    }
  }
});

test('review diffs have numbered rows and fold with a caret', async ({ page }) => {
  await page.goto('/shell-mockup.html?review');
  const file = page
    .getByTestId('mock-review')
    .locator('[data-review-file="Help/Concepts/Session Compaction"]');
  await expect(file.locator('.mk-diff-row.remove .n').first()).toHaveText('1');
  await expect(file.locator('.mk-diff-row.remove .m')).toHaveText('−');
  await expect(file.locator('.mk-diff-row.add .m')).toHaveText('+');
  await file
    .getByRole('button', {
      name: 'Collapse changes for Help/Concepts/Session Compaction',
      exact: true,
    })
    .click();
  await expect(file.locator('.mk-review-diff')).toHaveCount(0);
  await expect(file.getByRole('button', { name: 'Accept file', exact: true })).toBeVisible();
  await file
    .getByRole('button', {
      name: 'Expand changes for Help/Concepts/Session Compaction',
      exact: true,
    })
    .click();
  await expect(file.locator('.mk-review-diff')).toBeVisible();
});

test('the proposal separates its title from global controls and highlights changed words', async ({
  page,
}) => {
  await page.goto('/shell-mockup.html?review');
  const review = page.getByTestId('mock-review');
  const title = await review
    .getByRole('heading', { name: 'Tighten the knowledge notes' })
    .boundingBox();
  const toolbar = review.locator('.mk-review-toolbar');
  await expect(toolbar.getByRole('button', { name: 'Accept all', exact: true })).toBeVisible();
  const bar = await toolbar.boundingBox();
  expect(bar!.y).toBeGreaterThanOrEqual(title!.y + title!.height);
  const file = review.locator('[data-review-file="Help/Concepts/Session Compaction"]');
  await expect(file.locator('.mk-diff-row.add .mk-diff-word')).toContainText(
    'A session that reaches its budget keeps every turn.',
  );
  await expect(file.locator('.mk-diff-row.remove .mk-diff-word')).toHaveCount(0);
});

test('the mockup code example highlights syntax and folds its two hunks independently', async ({
  page,
}) => {
  await page.goto('/shell-mockup.html?review&code');
  const file = page.getByTestId('mock-changes').locator('[data-review-file="src/server.rs"]');
  await expect(file).toBeVisible();
  const hunks = file.locator('.mk-review-hunk-toggle');
  await expect(hunks).toHaveCount(2);
  await expect(file.locator('.mk-diff-row .c [style]').first()).toBeVisible();
  await expect(file.locator('.add .mk-diff-word').first()).toBeVisible();
  const colors = await file
    .locator('.mk-diff-row .c [style]')
    .evaluateAll((spans) => [...new Set(spans.map((s) => (s as HTMLElement).style.color))]);
  expect(colors.length).toBeGreaterThan(3);
  await hunks.first().click();
  await expect(hunks.first()).toHaveAttribute('aria-expanded', 'false');
  await expect(file.locator('.mk-diff-rows')).toHaveCount(1);
  await expect(file.getByText('tracing', { exact: true })).toBeVisible();
});

test('review carets share one axis', async ({ page }) => {
  await page.goto('/shell-mockup.html?review&code');
  const changes = page.getByTestId('mock-changes');
  const file = changes.locator('[data-review-file="src/server.rs"]');
  const fileCaret = await file.locator('.mk-diff-toggle svg').boundingBox();
  const hunkCaret = await file.locator('.mk-review-hunk-toggle svg').first().boundingBox();
  expect(Math.abs(fileCaret!.x + fileCaret!.width / 2 - hunkCaret!.x - hunkCaret!.width / 2)).toBeLessThan(0.5);
});

test('the active review tab stays visible after narrowing', async ({ page }) => {
  await page.goto('/shell-mockup.html?review');
  const pane = page.locator('.wm-pane').filter({ has: page.getByTestId('mock-review') });
  await pane.evaluate(el => { el.style.width = '450px'; el.style.maxWidth = '450px'; });
  const tab = pane.locator('.wm-tab[data-active]');
  await expect.poll(async () => {
    const active = await tab.boundingBox();
    const strip = await pane.locator('.wm-tabstrip').boundingBox();
    return active!.x + active!.width - strip!.x - strip!.width;
  }).toBeLessThanOrEqual(1);
});
