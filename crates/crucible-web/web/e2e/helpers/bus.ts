import type { Page } from '@playwright/test';
import type { BusEvents } from '@/lib/bus';

/**
 * Emits one typed bus event in the page, the way a product gesture would.
 *
 * `page.evaluate` cannot reach a module singleton, so `src/lib/bus.ts` puts
 * its one bus on `window.__bus` — the same seam `__windowStore` gives these
 * specs. The event names and payloads are the ones `BusEvents` declares.
 *
 * An omitted payload emits `{}`, which is what the product's own call sites
 * pass for an all-optional payload. Handlers destructure the payload, so
 * emitting `undefined` would throw inside the handler instead of failing here.
 */
export async function busEmit<K extends keyof BusEvents>(
  page: Page,
  event: K,
  payload: BusEvents[K] = {} as BusEvents[K],
): Promise<void> {
  await page.evaluate(
    ([name, data]) => {
      (window as unknown as { __bus: { emit: (event: string, payload?: unknown) => void } })
        .__bus.emit(name, data);
    },
    [event, payload] as [string, unknown?],
  );
}
