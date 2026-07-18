import type { Page } from '@playwright/test';

/**
 * Block until all requested web fonts (IBM Plex Sans/Mono) have finished
 * loading. Without this, the Vite dev-server test environment can capture a
 * visual baseline while text is still rendered in the fallback font (FOUT),
 * producing a ~2% text-antialiasing diff that fails `toHaveScreenshot`
 * nondeterministically (which font tips over the threshold varies run to run).
 * Call after navigation/mount, before any screenshot assertion.
 */
export async function waitForFonts(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
}
