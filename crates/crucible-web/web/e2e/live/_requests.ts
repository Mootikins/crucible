import { expect, type Page } from '@playwright/test';

/**
 * What a live spec counts, and why it counts it here.
 *
 * The plan's Part E asks every entity spec to prove "one fetch per entity per
 * page load". The daemon's own log cannot answer that. `fakeLogPath` records
 * the calls the DAEMON makes to the fake model server, so it never sees a
 * browser call to `/api/*`; it stays the proof that a turn reached the model
 * and nothing else. The browser's outbound requests are visible only to
 * Playwright, through `page.on('request')` — the pattern
 * `session-management.live.spec.ts` already uses for a single route.
 *
 * A request is recorded when the browser ISSUES it, not when it answers, so a
 * count taken after the UI settles includes a call that is still in flight.
 * That is the count a cache claim needs: a second in-flight fetch for one
 * entity is the defect, whether or not it has answered yet.
 */

/** One outbound browser request to the daemon's HTTP API. */
export interface ApiRequest {
  method: string;
  path: string;
  /** The query string without its `?`, so a spec can tell two keys apart. */
  query: string;
  /** Playwright's resource type. An `EventSource` reports `eventsource`. */
  resourceType: string;
}

/** The reader a spec holds while it drives the page. */
export interface ApiRequestLog {
  /** How many times the page asked for `path` with `method`. */
  count(method: string, path: string | RegExp): number;
  /** Every recorded request, oldest first. */
  all(): ApiRequest[];
  /** The requests matching `path`, for a failure message that names them. */
  matching(path: string | RegExp): ApiRequest[];
  /** Forgets everything so far, so a later phase counts from zero. */
  reset(): void;
}

function pathMatches(candidate: string, wanted: string | RegExp): boolean {
  return typeof wanted === 'string' ? candidate === wanted : wanted.test(candidate);
}

/**
 * Records every browser request to `/api/*` so a spec can count fetches by
 * method and route.
 *
 * Attach it BEFORE `page.goto`, or the first load's fetches are already gone.
 */
export function captureApiRequests(page: Page): ApiRequestLog {
  const seen: ApiRequest[] = [];
  page.on('request', (req) => {
    let url: URL;
    try {
      url = new URL(req.url());
    } catch {
      return; // a data: or blob: request; never the daemon's API
    }
    if (!url.pathname.startsWith('/api/')) return;
    seen.push({
      method: req.method(),
      path: url.pathname,
      query: url.search.replace(/^\?/, ''),
      resourceType: req.resourceType(),
    });
  });
  return {
    count: (method, path) =>
      seen.filter((r) => r.method === method && pathMatches(r.path, path)).length,
    all: () => seen.slice(),
    matching: (path) => seen.filter((r) => pathMatches(r.path, path)),
    reset: () => {
      seen.length = 0;
    },
  };
}

/**
 * Waits until the page has issued no `/api/*` request for `quietMs`.
 *
 * A count taken at a fixed delay is a race: too early and a fetch the claim is
 * about has not been made, too late and the interactions poll has added one.
 * This waits for the page to STOP asking instead, which is the moment the
 * claim is about.
 *
 * The window stays well under the ten-second interactions poll
 * (`lib/query/interactions.ts`), so that poll ends a wait rather than
 * extending it forever.
 *
 * `expect.poll`, never an arbitrary sleep. The live tier is sleep-free and
 * `src/__tests__/architecture/e2e-discipline.test.ts` enforces it by scanning
 * this source, so the banned name does not appear here even in prose. The
 * rule is what this helper exists for: a fixed wait either ends before the
 * fetch a claim is about, or lasts long enough to collect one the claim is
 * not about. The condition here is "no new request for `quietMs`", which is
 * the state every count in this tier is taken in.
 */
export async function apiQuiet(
  log: ApiRequestLog,
  quietMs = 1200,
  capMs = 25_000,
): Promise<void> {
  let last = log.all().length;
  let since = Date.now();
  await expect
    .poll(
      () => {
        const now = log.all().length;
        if (now !== last) {
          last = now;
          since = Date.now();
        }
        return Date.now() - since >= quietMs;
      },
      {
        timeout: capMs,
        intervals: [100, 200, 200, 200, 400],
        message: `the page never stopped calling /api/* for ${quietMs}ms`,
      },
    )
    .toBe(true);
}

/** A failure message that names the requests behind a wrong count. */
export function describeRequests(log: ApiRequestLog, path: string | RegExp): string {
  const hits = log.matching(path);
  if (hits.length === 0) return 'no request matched';
  return hits.map((r) => `${r.method} ${r.path}${r.query ? `?${r.query}` : ''}`).join('\n');
}

// =============================================================================
// EventSource
// =============================================================================

/**
 * Why the SSE specs patch `EventSource` instead of counting requests.
 *
 * A count of `GET /api/chat/events/{id}` answers "how many streams were
 * OPENED". The refcount claim also needs "how many are open NOW": the second
 * pane must not open a stream, and the LAST pane to close must close the one
 * that is there. A closed source leaves no request behind, so the open count
 * alone cannot fail when a stream is leaked.
 *
 * The patch runs as an init script, before any application code, and keeps the
 * real constructor — it only records the url and marks the entry closed when
 * `close()` runs or the source errors into `CLOSED`.
 */
export interface SseRecord {
  url: string;
  open: boolean;
}

/** Installs the spy. Call it before `page.goto`, once per page. */
export async function installEventSourceSpy(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const records: SseRecord[] = [];
    (globalThis as Record<string, unknown>).__eventSources = records;
    const Real = globalThis.EventSource;
    class Spy extends Real {
      constructor(url: string | URL, init?: EventSourceInit) {
        super(url, init);
        const record = { url: String(url), open: true };
        records.push(record);
        this.addEventListener('error', () => {
          // A dropped stream retries by itself, so only a source the browser
          // gave up on is closed. A retrying one is still this pane's stream.
          if (this.readyState === Real.CLOSED) record.open = false;
        });
        (this as unknown as { __record: { open: boolean } }).__record = record;
      }
      close(): void {
        super.close();
        (this as unknown as { __record?: { open: boolean } }).__record!.open = false;
      }
    }
    globalThis.EventSource = Spy as unknown as typeof EventSource;
  });
}

/** Every stream the page opened, in order, with whether it is open now. */
export async function eventSources(page: Page): Promise<SseRecord[]> {
  return page.evaluate(
    () => ((globalThis as Record<string, unknown>).__eventSources as SseRecord[]) ?? [],
  );
}

/** The streams whose url contains `needle`. */
export async function sourcesFor(page: Page, needle: string): Promise<SseRecord[]> {
  return (await eventSources(page)).filter((s) => s.url.includes(needle));
}
