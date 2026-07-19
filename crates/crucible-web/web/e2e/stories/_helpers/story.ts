import type { Page, TestInfo } from '@playwright/test';
import path from 'node:path';

/**
 * Story helper: records a named image sequence for a user-story spec.
 *
 * Each `step()` call takes a full-page screenshot and writes it to the test's
 * artifact dir as `steps/NN-<slug>.png`, and attaches it to the HTML report so
 * the story reads as an ordered image sequence. The `stories` Playwright
 * project also records video + trace for the whole run (see playwright.config).
 *
 * Usage:
 *   const story = createStory(testInfo);
 *   await story.step(page, 'session selected');
 */
export interface Story {
  step(page: Page, name: string): Promise<void>;
  /** Current step count (useful for assertions/logging). */
  count(): number;
}

function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

export function createStory(testInfo: TestInfo): Story {
  let n = 0;
  return {
    async step(page: Page, name: string): Promise<void> {
      n += 1;
      const idx = String(n).padStart(2, '0');
      const file = path.join(testInfo.outputDir, 'steps', `${idx}-${slugify(name)}.png`);
      const buffer = await page.screenshot({ path: file, fullPage: false });
      await testInfo.attach(`step ${idx}: ${name}`, { body: buffer, contentType: 'image/png' });
    },
    count() {
      return n;
    },
  };
}
