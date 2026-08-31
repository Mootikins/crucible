import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady } from './helpers/nav';

/**
 * E2E: which theme the shell boots in, and who decides.
 *
 * Two things had never been tested in a real browser.
 *
 * The app now reads `prefers-color-scheme`, and nothing covered the light theme
 * — light was unreachable by default before, so there was nothing to cover.
 * jsdom cannot stand in for this: it does not resolve `var()`, does not
 * implement `color-scheme`, and the unit tests stub `matchMedia` outright. The
 * media query, the cascade and the `color-scheme` declaration are only real
 * here.
 *
 * The precedence rule is the other half. A stored choice must beat the OS, and
 * `src/lib/__tests__/theme-preference.test.ts` proves that against a stub. This
 * proves it against Chromium's own media query.
 *
 * NO screenshot baselines here on purpose: a palette is a computed colour, and
 * a committed PNG would only re-state it in a form nothing can read.
 */

const KEY = 'crucible:theme';

/** Seed (or clear) the stored preference before the app's first paint. */
function storedTheme(value: 'dark' | 'light' | null) {
  return {
    cookies: [],
    origins:
      value === null
        ? []
        : [
            {
              origin: new URL(
                process.env.CRUCIBLE_WEB_PORT
                  ? `http://localhost:${process.env.CRUCIBLE_WEB_PORT}`
                  : 'http://localhost:5273',
              ).origin,
              localStorage: [{ name: KEY, value }],
            },
          ],
  };
}

/**
 * What the shell actually painted.
 *
 * `data-theme` is the app's own state; `color-scheme` is what native
 * scrollbars and form widgets follow; the body luminance is the palette that
 * a person sees. Reading all three keeps a passing test from meaning only
 * that an attribute was written.
 */
async function paintedTheme(page: Page) {
  return page.evaluate(() => {
    const root = document.documentElement;
    const rgb = getComputedStyle(document.body).backgroundColor;
    const [r, g, b] = (/rgba?\(([^)]+)\)/.exec(rgb)?.[1] ?? '0,0,0')
      .split(',')
      .map((n) => Number(n.trim()) / 255);
    // Rough relative luminance — enough to tell a near-black canvas from a
    // near-white one without restating either token's hex.
    return {
      attribute: root.getAttribute('data-theme'),
      colorScheme: getComputedStyle(root).colorScheme,
      prefersDark: window.matchMedia('(prefers-color-scheme: dark)').matches,
      stored: localStorage.getItem('crucible:theme'),
      luminance: 0.2126 * r + 0.7152 * g + 0.0722 * b,
    };
  });
}

test.describe('theme', () => {
  test.beforeEach(async ({ page }) => {
    await setupBasicMocks(page);
  });

  test('the suite boots DARK, which is what every visual baseline holds', async ({ page }) => {
    // The gate for the config pin. Both inputs are asserted, because removing
    // EITHER one silently changes what the story screenshots capture: without
    // `storageState` the app falls through to the media query, and without
    // `colorScheme` that query is Playwright's default of light.
    await page.goto('/');
    await appReady(page);

    const painted = await paintedTheme(page);
    expect(painted.stored, 'playwright.config.ts must seed the stored preference').toBe('dark');
    expect(painted.prefersDark, 'playwright.config.ts must pin colorScheme').toBe(true);
    expect(painted.attribute).toBeNull();
    expect(painted.colorScheme).toBe('dark');
    expect(painted.luminance).toBeLessThan(0.1);
  });

  test.describe('light', () => {
    test.use({ colorScheme: 'light', storageState: storedTheme('light') });

    test('paints the light palette, and native controls follow it', async ({ page }) => {
      await page.goto('/');
      await appReady(page);

      const painted = await paintedTheme(page);
      expect(painted.attribute).toBe('light');
      // Only a real browser proves this: the declaration is what scrollbars,
      // form widgets and the caret read, and jsdom implements none of it.
      expect(painted.colorScheme).toBe('light');
      expect(painted.luminance).toBeGreaterThan(0.7);
    });

    test('reads the light palette through the tokens, not a hardcoded rule', async ({ page }) => {
      await page.goto('/');
      await appReady(page);
      const shellBg = await page.evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--color-shell-bg').trim(),
      );
      const bodyBg = await page.evaluate(
        () => getComputedStyle(document.body).backgroundColor,
      );
      // `body` says `background-color: var(--color-shell-bg)`, so the two must
      // agree once the browser resolves the variable. A light theme that
      // painted the right colour from the wrong place would pass the luminance
      // check above and fail here.
      const asRgb = await page.evaluate((hex: string) => {
        const probe = document.createElement('span');
        probe.style.color = hex;
        document.body.appendChild(probe);
        const out = getComputedStyle(probe).color;
        probe.remove();
        return out;
      }, shellBg);
      expect(bodyBg.replace(/\s/g, '')).toBe(asRgb.replace(/\s/g, ''));
    });
  });

  test.describe('an OS preference the user has not overruled', () => {
    test.use({ colorScheme: 'light', storageState: storedTheme(null) });

    test('is honoured — a light-set machine no longer opens on dark', async ({ page }) => {
      await page.goto('/');
      await appReady(page);

      const painted = await paintedTheme(page);
      expect(painted.stored, 'boot must NOT write a choice the user never made').toBeNull();
      expect(painted.attribute).toBe('light');
      expect(painted.luminance).toBeGreaterThan(0.7);
    });
  });

  test.describe('a stored choice against a contrary OS', () => {
    // The regression that actually bites: pick dark on a light-set machine.
    test.use({ colorScheme: 'light', storageState: storedTheme('dark') });

    test('the stored choice WINS', async ({ page }) => {
      await page.goto('/');
      await appReady(page);

      const painted = await paintedTheme(page);
      expect(painted.prefersDark, 'the OS is asking for light').toBe(false);
      expect(painted.attribute).toBeNull();
      expect(painted.colorScheme).toBe('dark');
      expect(painted.luminance).toBeLessThan(0.1);
    });
  });
});
