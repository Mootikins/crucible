import { test, expect } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { appReady } from '../helpers/nav';

/**
 * The gate on the visual baselines' palette.
 *
 * Every committed screenshot under `e2e/__screenshots__/stories` is DARK. That
 * used to be guaranteed by the app itself, which booted dark no matter what;
 * once it started honouring `prefers-color-scheme` the palette became an input
 * to the suite, and an unpinned input decided four baselines. Playwright's
 * `colorScheme` "Defaults to 'light'" (playwright-core types.d.ts), so nothing
 * about that was obvious from reading the config.
 *
 * This lives in the `stories` project deliberately: it is the project whose
 * output the baselines describe, so it fails exactly where the damage lands.
 * It reads the RESOLVED project config and then the painted result — not the
 * config file's text, which is satisfied by a literal that no longer applies.
 */

const KEY = 'crucible:theme';
const PINNED = 'dark';

test.describe('the stories project pins its theme', () => {
  test('the resolved project config pins BOTH inputs', () => {
    // Two inputs, two ways to lose the pin. `storageState` is the app's own
    // preference, which wins by design; `colorScheme` is the media query it
    // falls back to when a spec clears storage. Assert each, or removing one
    // passes.
    const { use } = test.info().project;

    expect(use.colorScheme, 'playwright.config.ts must pin colorScheme').toBe(PINNED);

    const state = use.storageState;
    expect(typeof state, 'storageState must be an inline object, not a file path').toBe('object');
    const origins = (state as { origins?: { localStorage?: { name: string; value: string }[] }[] })
      .origins;
    const seeded = origins?.flatMap((o) => o.localStorage ?? []).find((e) => e.name === KEY);
    expect(seeded?.value, `playwright.config.ts must seed ${KEY}`).toBe(PINNED);
  });

  test('and the app actually boots in it', async ({ page }) => {
    // The config half above can be right while the app ignores it — a renamed
    // storage key, or a boot path that overwrites the stored value. Measure
    // the paint, not the declaration.
    await setupBasicMocks(page);
    await page.goto('/');
    await appReady(page);

    const painted = await page.evaluate(() => {
      const rgb = getComputedStyle(document.body).backgroundColor;
      const [r, g, b] = (/rgba?\(([^)]+)\)/.exec(rgb)?.[1] ?? '255,255,255')
        .split(',')
        .map((n) => Number(n.trim()) / 255);
      return {
        attribute: document.documentElement.getAttribute('data-theme'),
        prefersDark: window.matchMedia('(prefers-color-scheme: dark)').matches,
        stored: localStorage.getItem('crucible:theme'),
        luminance: 0.2126 * r + 0.7152 * g + 0.0722 * b,
      };
    });

    expect(painted.stored).toBe(PINNED);
    expect(painted.prefersDark).toBe(true);
    // Dark writes NO attribute — the bare `:root` declaration IS the dark
    // palette, so an absent attribute and a dark one mean the same thing.
    expect(painted.attribute).toBeNull();
    expect(painted.luminance, 'the baselines are dark').toBeLessThan(0.1);
  });
});
