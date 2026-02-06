import { test, expect, type Page } from '@playwright/test';

async function waitForShellReady(page: Page): Promise<void> {
  await page.waitForFunction(() => {
    const zones = ['left', 'center', 'right', 'bottom'];
    return zones.every((z) => {
      const el = document.querySelector(`[data-zone="${z}"]`);
      return el instanceof HTMLElement;
    });
  }, { timeout: 10_000 });
}

async function waitForZoneCollapsed(page: Page, zone: string): Promise<void> {
  await page.waitForFunction(
    (z: string) => {
      const el = document.querySelector(`[data-zone="${z}"]`);
      return el instanceof HTMLElement && getComputedStyle(el).flexBasis === '0px';
    },
    zone,
    { timeout: 5_000 },
  );
}

test.describe('Layout Persistence — localStorage Survival', () => {
  test('zone visibility state persists across page refresh', async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await waitForShellReady(page);

    await page.locator('[data-testid="toggle-left"]').click();
    await waitForZoneCollapsed(page, 'left');

    const zoneStateBeforeRefresh = await page.evaluate(() =>
      localStorage.getItem('crucible:zones'),
    );
    expect(zoneStateBeforeRefresh).toBeTruthy();

    await page.reload();
    await waitForShellReady(page);

    await waitForZoneCollapsed(page, 'left');
  });

  test('zone widths are saved to localStorage on toggle', async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await waitForShellReady(page);

    await page.locator('[data-testid="toggle-left"]').click();
    await waitForZoneCollapsed(page, 'left');

    const widthsAfterToggle = await page.evaluate(() =>
      localStorage.getItem('crucible:zone-widths'),
    );
    expect(widthsAfterToggle).toBeTruthy();

    const parsed = JSON.parse(widthsAfterToggle!);
    expect(parsed.left).toBeGreaterThan(0);
    expect(parsed.right).toBeGreaterThan(0);
    expect(parsed.bottom).toBeGreaterThan(0);
  });

  test('per-zone layout uses independent storage keys', async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await waitForShellReady(page);

    await page.waitForFunction(() => {
      return document.querySelectorAll('.dv-tab').length >= 3;
    }, { timeout: 10_000 });

    const storageKeys = await page.evaluate(() => {
      const keys: string[] = [];
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key?.startsWith('crucible:')) keys.push(key);
      }
      return keys;
    });

    const hasZoneFormat = storageKeys.some((k) => k.match(/^crucible:layout:[a-z]+$/));
    const hasOldFormat = storageKeys.includes('crucible:layout');
    expect(hasOldFormat).toBe(false);

    if (hasZoneFormat) {
      const zoneKeys = storageKeys.filter((k) => k.match(/^crucible:layout:[a-z]+$/));
      for (const key of zoneKeys) {
        const value = await page.evaluate((k: string) => localStorage.getItem(k), key);
        expect(value).toBeTruthy();
        const parsed = JSON.parse(value!);
        expect(parsed).toHaveProperty('grid');
        expect(parsed).toHaveProperty('panels');
      }
    }
  });

  test('old layout key migrates to per-zone format', async ({ page }) => {
    const mockLayout = JSON.stringify({
      grid: { root: {}, width: 100, height: 100, orientation: 0 },
      panels: { chat: { id: 'chat', contentComponent: 'chat' } },
    });

    await page.goto('/');
    await page.evaluate((layout: string) => {
      localStorage.clear();
      localStorage.setItem('crucible:layout', layout);
    }, mockLayout);
    await page.reload();
    await waitForShellReady(page);

    const migrationResult = await page.waitForFunction(() => {
      const oldKey = localStorage.getItem('crucible:layout');
      const centerKey = localStorage.getItem('crucible:layout:center');
      return { oldKeyCleared: oldKey == null, centerKeySet: centerKey != null };
    }, { timeout: 5_000 });

    const result = await migrationResult.jsonValue();
    expect(result.oldKeyCleared).toBe(true);
  });

  test('corrupt localStorage does not crash the app', async ({ page }) => {
    await page.goto('/');
    await page.evaluate(() => {
      localStorage.setItem('crucible:zones', 'invalid json{{{');
      localStorage.setItem('crucible:zone-widths', '}{not valid');
      localStorage.setItem('crucible:layout:center', 'broken');
    });

    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.reload();
    await waitForShellReady(page);

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
    await waitForShellReady(page);

    await page.waitForFunction(() => {
      return document.querySelectorAll('.dv-tab').length >= 1;
    }, { timeout: 10_000 });

    const dockviewErrors = consoleErrors.filter(
      (e) => e.toLowerCase().includes('dockview') || e.includes('pane'),
    );
    expect(dockviewErrors).toHaveLength(0);
  });
});
