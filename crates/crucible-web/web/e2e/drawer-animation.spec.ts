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

async function waitForZoneExpanded(page: Page, zone: string): Promise<void> {
  await page.waitForFunction(
    (z: string) => {
      const el = document.querySelector(`[data-zone="${z}"]`);
      if (!(el instanceof HTMLElement)) return false;
      const basis = getComputedStyle(el).flexBasis;
      return basis !== '0px' && basis !== '0%';
    },
    zone,
    { timeout: 5_000 },
  );
}

test.describe('Drawer Animation — Zone Collapse/Expand', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => localStorage.clear());
    await page.goto('/');
    await waitForShellReady(page);
  });

  test('toggle left zone collapses with flex-basis animation', async ({ page }) => {
    await waitForZoneExpanded(page, 'left');

    await page.locator('[data-testid="toggle-left"]').click();
    await waitForZoneCollapsed(page, 'left');

    const basis = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="left"]');
      return el ? getComputedStyle(el).flexBasis : null;
    });
    expect(await basis.jsonValue()).toBe('0px');
  });

  test('toggle right zone collapses with flex-basis animation', async ({ page }) => {
    await waitForZoneExpanded(page, 'right');

    await page.locator('[data-testid="toggle-right"]').click();
    await waitForZoneCollapsed(page, 'right');

    const basis = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="right"]');
      return el ? getComputedStyle(el).flexBasis : null;
    });
    expect(await basis.jsonValue()).toBe('0px');
  });

  test('toggle bottom zone expands then collapses', async ({ page }) => {
    await waitForZoneCollapsed(page, 'bottom');

    await page.locator('[data-testid="toggle-bottom"]').click();
    await waitForZoneExpanded(page, 'bottom');

    await page.locator('[data-testid="toggle-bottom"]').click();
    await waitForZoneCollapsed(page, 'bottom');
  });

  test('rapid toggle 5x does not leave zone in stuck state', async ({ page }) => {
    const toggle = page.locator('[data-testid="toggle-left"]');

    for (let i = 0; i < 5; i++) {
      await toggle.click();
    }

    // 5 toggles from expanded = collapsed (odd number of toggles)
    await waitForZoneCollapsed(page, 'left');
    const basis = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="left"]');
      return el ? getComputedStyle(el).flexBasis : null;
    });
    expect(await basis.jsonValue()).toBe('0px');
  });

  test('prefers-reduced-motion disables animation duration', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/');
    await waitForShellReady(page);

    const transitionDuration = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="left"]');
      if (!el) return null;
      return getComputedStyle(el).transitionDuration;
    });
    const value = await transitionDuration.jsonValue();
    if (value && value !== '0s') {
      const ms = parseFloat(value) * (value.includes('ms') ? 1 : 1000);
      expect(ms).toBeLessThanOrEqual(10);
    }
  });

  test('collapse and expand round-trip restores zone width', async ({ page }) => {
    const initialBasis = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="left"]');
      return el ? getComputedStyle(el).flexBasis : null;
    }).then((h) => h.jsonValue());
    expect(initialBasis).not.toBe('0px');

    await page.locator('[data-testid="toggle-left"]').click();
    await waitForZoneCollapsed(page, 'left');

    await page.locator('[data-testid="toggle-left"]').click();

    await page.waitForFunction(
      (target: string) => {
        const el = document.querySelector('[data-zone="left"]');
        if (!el) return false;
        return getComputedStyle(el).flexBasis === target;
      },
      initialBasis as string,
      { timeout: 5_000 },
    );

    const restoredBasis = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="left"]');
      return el ? getComputedStyle(el).flexBasis : null;
    }).then((h) => h.jsonValue());
    expect(restoredBasis).toBe(initialBasis);
  });

  test('all three zones can be collapsed independently', async ({ page }) => {
    await page.locator('[data-testid="toggle-left"]').click();
    await waitForZoneCollapsed(page, 'left');

    const rightStillExpanded = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="right"]');
      return el ? getComputedStyle(el).flexBasis !== '0px' : false;
    });
    expect(await rightStillExpanded.jsonValue()).toBe(true);

    await page.locator('[data-testid="toggle-right"]').click();
    await waitForZoneCollapsed(page, 'right');

    await page.locator('[data-testid="toggle-bottom"]').click();
    await waitForZoneExpanded(page, 'bottom');

    await page.locator('[data-testid="toggle-bottom"]').click();
    await waitForZoneCollapsed(page, 'bottom');
  });
});
