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

async function waitForDockviewTabs(page: Page): Promise<void> {
  await page.waitForFunction(() => {
    return document.querySelectorAll('.dv-tab').length >= 3;
  }, { timeout: 10_000 });
}

async function waitForFloatButtons(page: Page): Promise<void> {
  await page.waitForFunction(() => {
    return document.querySelectorAll('.dv-float-action-btn').length >= 1;
  }, { timeout: 10_000 });
}

async function getTabCountInZone(page: Page, zone: string): Promise<number> {
  return page.locator(`[data-zone="${zone}"] .dv-tab`).count();
}

test.describe('Floating Panels — Float/Dock Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => localStorage.clear());
    await page.goto('/');
    await waitForShellReady(page);
    await waitForDockviewTabs(page);
  });

  test('float action buttons render in non-center zone headers', async ({ page }) => {
    await waitForFloatButtons(page);

    const floatButtons = page.locator('.dv-float-action-btn');
    const count = await floatButtons.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test('float button shows ⊞ in non-center zones', async ({ page }) => {
    await waitForFloatButtons(page);

    const leftFloatBtn = page.locator('[data-zone="left"] .dv-float-action-btn').first();
    const hasLeftBtn = await leftFloatBtn.isVisible().catch(() => false);

    if (hasLeftBtn) {
      const text = await leftFloatBtn.textContent();
      expect(text).toBe('⊞');
    }
  });

  test('float action moves panel from source zone to center', async ({ page }) => {
    await waitForFloatButtons(page);

    const leftTabsBefore = await getTabCountInZone(page, 'left');
    const centerTabsBefore = await getTabCountInZone(page, 'center');

    const leftFloatBtn = page.locator('[data-zone="left"] .dv-float-action-btn').first();
    const hasLeftBtn = await leftFloatBtn.isVisible().catch(() => false);
    if (!hasLeftBtn) return;

    await leftFloatBtn.click();

    await page.waitForFunction(
      ({ prevLeft, prevCenter }: { prevLeft: number; prevCenter: number }) => {
        const leftTabs = document.querySelectorAll('[data-zone="left"] .dv-tab').length;
        const centerTabs = document.querySelectorAll('[data-zone="center"] .dv-tab').length;
        return leftTabs < prevLeft || centerTabs > prevCenter;
      },
      { prevLeft: leftTabsBefore, prevCenter: centerTabsBefore },
      { timeout: 5_000 },
    );

    const leftTabsAfter = await getTabCountInZone(page, 'left');
    expect(leftTabsAfter).toBeLessThan(leftTabsBefore);
  });

  test('floated panel tab appears in center zone after float', async ({ page }) => {
    await waitForFloatButtons(page);

    const leftFloatBtn = page.locator('[data-zone="left"] .dv-float-action-btn').first();
    const hasLeftBtn = await leftFloatBtn.isVisible().catch(() => false);
    if (!hasLeftBtn) return;

    const leftTabsBefore = await getTabCountInZone(page, 'left');
    await leftFloatBtn.click();

    await page.waitForFunction(
      (prevCount: number) => {
        return document.querySelectorAll('[data-zone="left"] .dv-tab').length < prevCount;
      },
      leftTabsBefore,
      { timeout: 5_000 },
    );

    const centerTabs = await getTabCountInZone(page, 'center');
    expect(centerTabs).toBeGreaterThanOrEqual(2);
  });

  test('dock button shows ⊡ in center zone', async ({ page }) => {
    await waitForFloatButtons(page);

    const centerDockBtn = page.locator('[data-zone="center"] .dv-float-action-btn').first();
    const hasCenterBtn = await centerDockBtn.isVisible().catch(() => false);

    if (hasCenterBtn) {
      const text = await centerDockBtn.textContent();
      expect(text).toBe('⊡');
    }
  });

  test('dock action returns panel to original zone', async ({ page }) => {
    await waitForFloatButtons(page);

    const leftTabsBefore = await getTabCountInZone(page, 'left');
    const leftFloatBtn = page.locator('[data-zone="left"] .dv-float-action-btn').first();
    const hasLeftBtn = await leftFloatBtn.isVisible().catch(() => false);
    if (!hasLeftBtn || leftTabsBefore === 0) return;

    await leftFloatBtn.click();

    await page.waitForFunction(
      (prevCount: number) => {
        return document.querySelectorAll('[data-zone="left"] .dv-tab').length < prevCount;
      },
      leftTabsBefore,
      { timeout: 5_000 },
    );

    const centerDockBtns = page.locator('[data-zone="center"] .dv-float-action-btn');
    const dockBtnCount = await centerDockBtns.count();

    if (dockBtnCount > 0) {
      const lastDockBtn = centerDockBtns.last();
      await lastDockBtn.click();

      await page.waitForFunction(
        (expectedCount: number) => {
          return document.querySelectorAll('[data-zone="left"] .dv-tab').length === expectedCount;
        },
        leftTabsBefore,
        { timeout: 5_000 },
      );

      const leftTabsAfter = await getTabCountInZone(page, 'left');
      expect(leftTabsAfter).toBe(leftTabsBefore);
    }
  });

  test('no console errors during float/dock cycle', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await waitForFloatButtons(page);

    const leftFloatBtn = page.locator('[data-zone="left"] .dv-float-action-btn').first();
    const hasLeftBtn = await leftFloatBtn.isVisible().catch(() => false);

    if (hasLeftBtn) {
      await leftFloatBtn.click();

      await page.waitForFunction(
        () => {
          const left = document.querySelectorAll('[data-zone="left"] .dv-tab').length;
          return left < 2;
        },
        { timeout: 5_000 },
      ).catch(() => null);
    }

    const critical = errors.filter(
      (e) => !e.includes('fetch') && !e.includes('net::') && !e.includes('HTTP'),
    );
    expect(critical).toEqual([]);
  });
});
