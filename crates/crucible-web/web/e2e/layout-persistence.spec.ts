import { test, expect, type Page } from '@playwright/test';

async function waitForDockviewReady(page: Page): Promise<void> {
  await page.waitForSelector('.dockview-theme-abyss', { timeout: 10_000 });
  await page.waitForFunction(() => {
    const dockview = document.querySelector('.dockview-theme-abyss');
    return dockview instanceof HTMLElement && dockview.children.length > 0;
  }, { timeout: 10_000 });
}

test.describe('Layout Persistence — Docked Panel State', () => {
  test('docked panel visibility persists across page refresh', async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await waitForDockviewReady(page);

    const isMac = process.platform === 'darwin';
    const modifier = isMac ? 'Meta' : 'Control';

    await page.keyboard.press(`${modifier}+KeyB`);

    await page.waitForFunction(() => {
      const el = document.querySelector('.dv-docked-left-container');
      if (!el) return false;
      return getComputedStyle(el).flexBasis === '0px';
    }, { timeout: 5000 });

    const layoutBeforeRefresh = await page.evaluate(() =>
      localStorage.getItem('crucible:layout'),
    );
    expect(layoutBeforeRefresh).toBeTruthy();

    const parsed = JSON.parse(layoutBeforeRefresh!);
    expect(parsed).toHaveProperty('dockedGroups');

    await page.reload();
    await waitForDockviewReady(page);

    await page.waitForFunction(() => {
      const el = document.querySelector('.dv-docked-left-container');
      if (!el) return false;
      return getComputedStyle(el).flexBasis === '0px';
    }, { timeout: 5000 });

    const leftCollapsed = await page.evaluate(() => {
      const el = document.querySelector('.dv-docked-left-container');
      return el ? getComputedStyle(el).flexBasis === '0px' : false;
    });
    expect(leftCollapsed).toBe(true);
  });

  test('docked panel sizes persist across page refresh', async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await waitForDockviewReady(page);

    const initialWidth = await page.evaluate(() => {
      const el = document.querySelector('.dv-docked-right-container');
      return el ? getComputedStyle(el).flexBasis : null;
    });
    expect(initialWidth).toMatch(/\d+px/);

    await page.reload();
    await waitForDockviewReady(page);

    const restoredWidth = await page.evaluate(() => {
      const el = document.querySelector('.dv-docked-right-container');
      return el ? getComputedStyle(el).flexBasis : null;
    });
    expect(restoredWidth).toBe(initialWidth);
  });

  test('single layout key stores all docked panel state', async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await waitForDockviewReady(page);

    await page.waitForTimeout(1000);

    const storageKeys = await page.evaluate(() => {
      const keys: string[] = [];
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key?.startsWith('crucible:')) keys.push(key);
      }
      return keys;
    });

    const hasNewFormat = storageKeys.includes('crucible:layout');
    expect(hasNewFormat).toBe(true);

    const hasOldZoneKeys = storageKeys.some((k) => k.match(/^crucible:layout:(left|right|bottom|center)$/));
    expect(hasOldZoneKeys).toBe(false);

    const hasOldZoneState = storageKeys.includes('crucible:zones');
    expect(hasOldZoneState).toBe(false);

    const hasOldZoneWidths = storageKeys.includes('crucible:zone-widths');
    expect(hasOldZoneWidths).toBe(false);
  });

  test('old layout keys migrate to new format', async ({ page }) => {
    const mockCenterLayout = JSON.stringify({
      grid: { root: {}, width: 100, height: 100, orientation: 0 },
      panels: { chat: { id: 'chat', contentComponent: 'chat' } },
    });

    await page.goto('/');
    await page.evaluate((layout: string) => {
      localStorage.clear();
      localStorage.setItem('crucible:layout:center', layout);
      localStorage.setItem('crucible:zones', JSON.stringify({ left: 'visible', right: 'visible', bottom: 'hidden' }));
      localStorage.setItem('crucible:zone-widths', JSON.stringify({ left: 280, right: 350, bottom: 200 }));
    }, mockCenterLayout);
    await page.reload();
    await waitForDockviewReady(page);

    const migrationResult = await page.waitForFunction(() => {
      const newKey = localStorage.getItem('crucible:layout');
      const oldCenterKey = localStorage.getItem('crucible:layout:center');
      const oldZonesKey = localStorage.getItem('crucible:zones');
      return {
        newKeySet: newKey != null,
        oldCenterKeyCleared: oldCenterKey == null,
        oldZonesKeyCleared: oldZonesKey == null,
      };
    }, { timeout: 5_000 });

    const result = await migrationResult.jsonValue();
    expect(result.newKeySet).toBe(true);
    expect(result.oldCenterKeyCleared).toBe(true);
    expect(result.oldZonesKeyCleared).toBe(true);

    const newLayout = await page.evaluate(() => localStorage.getItem('crucible:layout'));
    const parsed = JSON.parse(newLayout!);
    expect(parsed).toHaveProperty('dockedGroups');
    expect(Array.isArray(parsed.dockedGroups)).toBe(true);
  });

  test('corrupt localStorage does not crash the app', async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => {
      localStorage.setItem('crucible:layout', 'invalid json{{{');
    });

    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.reload();
    await waitForDockviewReady(page);

    const critical = errors.filter(
      (e) => !e.includes('fetch') && !e.includes('net::') && !e.includes('HTTP'),
    );
    expect(critical).toEqual([]);
  });

  test('no dockview errors on initial load', async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        consoleErrors.push(msg.text());
      }
    });

    await page.addInitScript(() => localStorage.clear());
    await page.goto('/');
    await waitForDockviewReady(page);

    await page.waitForFunction(() => {
      return document.querySelectorAll('.dv-tab').length >= 1;
    }, { timeout: 10_000 });

    const dockviewErrors = consoleErrors.filter(
      (e) => e.toLowerCase().includes('dockview') || e.includes('docked'),
    );
    expect(dockviewErrors).toHaveLength(0);
  });
});
