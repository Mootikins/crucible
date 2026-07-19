import type { Page } from '@playwright/test';

/**
 * Force every declared web font (IBM Plex Sans/Mono, all weights) to finish
 * downloading before a visual baseline is captured.
 *
 * `document.fonts.ready` alone is insufficient: webfonts load lazily (a weight
 * is only fetched once an element using it renders), so awaiting `ready` at
 * navigation time can resolve *before* the bold heading/body weights are even
 * requested. On the dev-server that leaves text rendered in the fallback font
 * (FOUT) at screenshot time — a ~2% text-antialiasing diff that fails
 * `toHaveScreenshot`. Iterating `document.fonts` and calling `.load()` on each
 * face forces the download eagerly, so all weights are applied before capture.
 * Call after navigation/mount.
 */
export async function waitForFonts(page: Page): Promise<void> {
  await page.evaluate(async () => {
    // Force-load every declared @font-face (all families/weights), not just the
    // ones an element has already triggered.
    await Promise.all(Array.from(document.fonts, (f) => f.load().catch(() => undefined)));
    await document.fonts.ready;
  });
}
