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

test.describe('Shell Layout — Multi-Dockview Structure', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => localStorage.clear());
    await page.goto('/');
    await waitForShellReady(page);
  });

  test('renders all 4 zone wrappers with data-zone attributes', async ({ page }) => {
    for (const zone of ['left', 'center', 'right', 'bottom'] as const) {
      const wrapper = page.locator(`[data-zone="${zone}"]`);
      await expect(wrapper).toBeAttached();
    }
  });

  test('center zone has flex:1 and sidebars have flex-basis', async ({ page }) => {
    const centerFlex = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="center"]');
      if (!el) return null;
      return getComputedStyle(el).flex;
    });
    const centerFlexValue = await centerFlex.jsonValue();
    expect(centerFlexValue).toContain('1');

    const leftBasis = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="left"]');
      if (!el) return null;
      return getComputedStyle(el).flexBasis;
    });
    const leftBasisValue = await leftBasis.jsonValue();
    expect(leftBasisValue).not.toBe('0px');
    expect(leftBasisValue).toMatch(/\d+px/);

    const rightBasis = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="right"]');
      if (!el) return null;
      return getComputedStyle(el).flexBasis;
    });
    const rightBasisValue = await rightBasis.jsonValue();
    expect(rightBasisValue).not.toBe('0px');
    expect(rightBasisValue).toMatch(/\d+px/);
  });

  test('each zone contains a dockview container', async ({ page }) => {
    await page.waitForFunction(() => {
      const zones = ['left', 'center', 'right'];
      return zones.every((z) => {
        const zone = document.querySelector(`[data-zone="${z}"]`);
        return zone?.querySelector('.dockview-theme-abyss') != null;
      });
    }, { timeout: 10_000 });

    for (const zone of ['left', 'center', 'right'] as const) {
      const dv = page.locator(`[data-zone="${zone}"] .dockview-theme-abyss`);
      await expect(dv).toBeAttached();
    }
  });

  test('toggle buttons render for left, right, and bottom', async ({ page }) => {
    for (const testid of ['toggle-left', 'toggle-right', 'toggle-bottom'] as const) {
      const btn = page.locator(`[data-testid="${testid}"]`);
      await expect(btn).toBeVisible();
    }
  });

  test('icon rails have correct aria-labels', async ({ page }) => {
    const leftToggle = page.locator('[data-testid="toggle-left"]');
    await expect(leftToggle).toBeVisible();
    await expect(leftToggle).toHaveAttribute('aria-label', 'Toggle left sidebar');

    const rightToggle = page.locator('[data-testid="toggle-right"]');
    await expect(rightToggle).toBeVisible();
    await expect(rightToggle).toHaveAttribute('aria-label', 'Toggle right sidebar');
  });

  test('app loads without critical errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.goto('/');
    await waitForShellReady(page);

    const critical = errors.filter(
      (e) => !e.includes('fetch') && !e.includes('net::') && !e.includes('HTTP'),
    );
    expect(critical).toEqual([]);
  });

  test('default zone visibility: left and right expanded, bottom collapsed', async ({ page }) => {
    const leftExpanded = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="left"]');
      if (!el) return null;
      return getComputedStyle(el).flexBasis !== '0px';
    });
    expect(await leftExpanded.jsonValue()).toBe(true);

    const rightExpanded = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="right"]');
      if (!el) return null;
      return getComputedStyle(el).flexBasis !== '0px';
    });
    expect(await rightExpanded.jsonValue()).toBe(true);

    const bottomCollapsed = await page.waitForFunction(() => {
      const el = document.querySelector('[data-zone="bottom"]');
      if (!el) return null;
      return getComputedStyle(el).flexBasis === '0px';
    });
    expect(await bottomCollapsed.jsonValue()).toBe(true);
  });

  test('toggle button aria-expanded reflects zone state', async ({ page }) => {
    const leftToggle = page.locator('[data-testid="toggle-left"]');
    await expect(leftToggle).toHaveAttribute('aria-expanded', 'true');

    const bottomToggle = page.locator('[data-testid="toggle-bottom"]');
    await expect(bottomToggle).toHaveAttribute('aria-expanded', 'false');
  });
});
