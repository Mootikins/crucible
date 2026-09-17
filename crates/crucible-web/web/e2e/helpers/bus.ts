import type { Page } from '@playwright/test';

/**
 * Emits one typed bus event in the page, the way a product gesture would.
 *
 * `page.evaluate` cannot reach a module singleton, so `src/lib/bus.ts` puts
 * its one bus on `window.__bus` — the same seam `__windowStore` gives these
 * specs. The event names and payloads are the ones `BusEvents` declares.
 */
export async function busEmit(page: Page, event: string, payload?: unknown): Promise<void> {
  await page.evaluate(
    ([name, data]) => {
      (window as unknown as { __bus: { emit: (event: string, payload?: unknown) => void } })
        .__bus.emit(name, data);
    },
    [event, payload] as [string, unknown?],
  );
}
