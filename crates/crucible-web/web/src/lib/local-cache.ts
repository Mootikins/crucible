/**
 * Last-known-value cache for panel data (stale-while-revalidate, browser
 * side). A hard reload was showing "Loading…" text all over the shell while
 * six fast-but-async fetches raced the first paint — the previous answer is
 * almost always right for catalog-ish data, so paint it immediately and let
 * the fetch correct it.
 */

const PREFIX = 'crucible:cache:';

/** Apply the cached value (if any) synchronously, then fetch, re-apply, and
 * persist. Fetch failures keep the cached value on screen. */
export function swrLocal<T>(
  key: string,
  fetcher: () => Promise<T>,
  apply: (value: T) => void,
): void {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    if (raw !== null) apply(JSON.parse(raw) as T);
  } catch {
    /* corrupt entry or private mode: fall through to the fetch */
  }
  void fetcher()
    .then((value) => {
      apply(value);
      try {
        localStorage.setItem(PREFIX + key, JSON.stringify(value));
      } catch {
        /* private mode / quota: live value still applied */
      }
    })
    .catch(() => {
      /* offline / server gone: last-known value stands */
    });
}
