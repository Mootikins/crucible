/**
 * Playwright run-code script: reproduce center splitter resize.
 * Run with: playwright-cli run-code "$(cat e2e/scripts/center-resize-repro.js)"
 * Prerequisite: playwright-cli open "http://localhost:5173/" first.
 */
(async () => {
  const splitter = page.locator('[data-split-id="split-root"]');
  await splitter.waitFor({ state: 'visible' });
  const container = splitter.locator('..');
  const firstPane = container.locator('> div').first();
  const widthBefore = await firstPane.evaluate((el) => el.getBoundingClientRect().width);
  const box = await splitter.boundingBox();
  if (!box) throw new Error('splitter not visible');
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 80, cy, { steps: 5 });
  await page.mouse.up();
  const widthAfter = await firstPane.evaluate((el) => el.getBoundingClientRect().width);
  console.log(JSON.stringify({ widthBefore, widthAfter }));
})();
