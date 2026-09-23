import type { Page } from '@playwright/test';

/**
 * Pins editor settings before any script of the page runs.
 *
 * A spec that holds unsent edits pins `autosaveSeconds: 0`: a note saves two
 * idle seconds after a keystroke by default, and that save races whatever the
 * spec reads next. `version: 2` is load-bearing — `loadSettings` migrates a
 * stored 0 from version 1 back to the default.
 */
export async function pinEditorSettings(page: Page, editor: Record<string, unknown>): Promise<void> {
  await page.addInitScript((pinned) => {
    localStorage.setItem('crucible:settings', JSON.stringify({ version: 2, editor: pinned }));
  }, editor);
}
