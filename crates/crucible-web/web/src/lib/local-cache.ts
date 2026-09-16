/**
 * Last-known-value cache for panel data (stale-while-revalidate, browser
 * side). A hard reload was showing "Loading…" text all over the shell while
 * six fast-but-async fetches raced the first paint — the previous answer is
 * almost always right for catalog-ish data, so paint it immediately and let
 * the fetch correct it.
 */

const PREFIX = 'crucible:cache:';

/**
 * The last value stored under one key, or `null` — nothing stored it, or the
 * entry is corrupt, or the browser refuses storage (private mode).
 *
 * Exported because the query layer keeps the same behaviour under the same
 * keys: a hook seeds its cache from here and writes back on a successful
 * fetch, so storage has one owner rather than one per entity module.
 */
export function readLocalCache<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    return raw === null ? null : (JSON.parse(raw) as T);
  } catch {
    return null;
  }
}

/** Stores one value, and does nothing when the browser refuses storage. */
export function writeLocalCache<T>(key: string, value: T): void {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    /* private mode / quota: the live value still reached the caller */
  }
}

/** Apply the cached value (if any) synchronously, then fetch, re-apply, and
 * persist. Fetch failures keep the cached value on screen. */
export function swrLocal<T>(
  key: string,
  fetcher: () => Promise<T>,
  apply: (value: T) => void,
): void {
  const cached = readLocalCache<T>(key);
  if (cached !== null) apply(cached);
  void fetcher()
    .then((value) => {
      apply(value);
      writeLocalCache(key, value);
    })
    .catch(() => {
      /* offline / server gone: last-known value stands */
    });
}
