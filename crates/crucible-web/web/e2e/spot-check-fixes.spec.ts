import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openFilesPanel } from './helpers/nav';
import { setupEditorHarness, HARNESS_KILN } from './stories/_helpers/editor-harness';

test('table widget adds no trailing rendered HTML blank line', async ({ page }) => {
  const note = { name: 'Table', path: `${HARNESS_KILN}/Table.md`, content: '# Table\n\n| Element | Status |\n| --- | --- |\n| Heading | Styled |\n| Code | Editable |\n\nAfter the table.\n' };
  const harness = await setupEditorHarness(page, [note]);
  await harness.open(note);
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  const widget = page.locator('.cm-lp-table');
  await expect(widget).toBeVisible();
  const extra = await widget.evaluate(el => el.getBoundingClientRect().height - el.querySelector('table')!.getBoundingClientRect().height);
  expect(extra).toBeLessThanOrEqual(8);
});

test('long root label stays clear of toolbar actions and only one swap is offered', async ({ page }) => {
  await setupBasicMocks(page, { projects: [{ path: '/repo', name: 'chat-2026-10-01T1522-long-workspace-name', kilns: [], last_accessed: '2026-01-01T00:00:00Z' }] });
  await page.goto('/');
  await appReady(page);
  await openFilesPanel(page);
  await page.getByTestId('root-dropdown').click();
  await page.getByRole('option').filter({ hasText: 'chat-2026' }).click();
  const trigger = page.getByTestId('root-dropdown');
  await trigger.hover();
  const picker = (await trigger.boundingBox())!;
  const action = (await page.getByRole('button', { name: 'New note', exact: true }).boundingBox())!;
  expect(picker.x + picker.width).toBeLessThanOrEqual(action.x);
  await expect(page.getByRole('button', { name: /^Swap/ })).toHaveCount(1);
  const swap = page.getByTestId('ribbon-cmd-swap-centre');
  await expect(swap).toBeVisible();
  const layouts = () => page.evaluate(() => {
    const s = (window as any).__windowStore;
    return [s.edgePanels.left.layout.id, s.layout.id, s.edgePanels.right.layout.id];
  });
  const before = await layouts();
  await swap.click();
  expect(await layouts()).toEqual([before[0], before[2], before[1]]);
  await swap.click();
  expect(await layouts()).toEqual(before);
});
