import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openFilesPanel } from './helpers/nav';

test.use({ reducedMotion: 'reduce' });

for (const width of [390, 1440]) {
  test(`navigation is flat and rounded at ${width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 900 });
    await setupBasicMocks(page, { projects: [{ path: '/repo', name: 'Example project', kilns: [], last_accessed: '2026-01-01T00:00:00Z' }] });
    await page.route('**/api/rpc/fs.list_dir', route => route.fulfill({ json: { entries: [{ name: 'Notes.md', rel_path: 'Notes.md', is_dir: false, size: 10, modified: 0, status: null }], truncated: false } }));
    await page.goto('/');
    if (width < 768) {
      await page.getByRole('button', { name: 'Sessions and files', exact: true }).click();
      await page.getByRole('tab', { name: 'Files', exact: true }).click();
    } else {
      await appReady(page);
      await openFilesPanel(page);
    }
    await page.evaluate(() => document.documentElement.style.setProperty('--mk-radius', '18px'));
    const trigger = page.getByTestId('root-dropdown');
    await expect(trigger).toHaveCSS('border-top-width', '0px');
    await expect(trigger).toHaveCSS('border-radius', '12px');
    await trigger.click();
    const popup = page.getByTestId('root-dropdown-popout');
    await expect(popup).toHaveCSS('border-top-width', '0px');
    await expect(popup).toHaveCSS('border-radius', '18px');
    await expect(popup).toHaveCSS('box-shadow', 'none');
    const option = popup.getByRole('option').filter({ hasText: 'Example project' });
    await option.hover();
    await expect(option).toHaveCSS('border-radius', '12px');
    for (const theme of ['dark', 'light']) {
      await page.evaluate(t => { document.documentElement.dataset.theme = t; }, theme);
      await page.screenshot({ animations: 'disabled', path: testInfo.outputPath(`dropdown-${width}-${theme}.png`) });
    }
    await option.click();
    const row = page.locator('.tree-row').filter({ hasText: 'Notes' }).first();
    await expect(row).toBeVisible();
    await row.hover();
    await expect(row).toHaveCSS('border-radius', '12px');
    await expect(page.locator('.files-toolbar')).toHaveCSS('border-bottom-width', '0px');
    await row.click({ button: 'right' });
    const menu = page.getByRole('menu').last();
    await expect(menu).toBeVisible();
    await expect(menu).toBeInViewport();
    await expect(menu).toHaveCSS('opacity', '1');
    await menu.getByRole('menuitem').first().hover({ timeout: 3000 });
    await expect(menu).toHaveCSS('border-top-width', '0px');
    await expect(menu).toHaveCSS('border-radius', '18px');
    await expect(menu).toHaveCSS('box-shadow', 'none');
    await expect(menu.getByRole('separator').first()).toHaveCSS('border-top-width', '0px');
    await page.screenshot({ animations: 'disabled', path: testInfo.outputPath(`context-menu-${width}.png`) });
    await page.keyboard.press('Escape');
    if (width < 768) {
      if (await page.getByTestId('drawer-left').getAttribute('inert') !== null)
        await page.getByRole('button', { name: 'Sessions and files', exact: true }).click();
      await page.getByRole('tab', { name: 'Sessions', exact: true }).click();
    }
    const session = page.locator('[data-session-id]').first();
    await session.hover();
    await expect(session).toHaveCSS('border-radius', '12px');
    await page.screenshot({ animations: 'disabled', path: testInfo.outputPath(`sessions-${width}.png`) });
  });
}
