import type { InvalidateQueryFilters, QueryClient, QueryKey } from '@tanstack/solid-query';

/** One key, or a full filter when a key prefix alone reaches too much. */
export type RefreshTarget = QueryKey | InvalidateQueryFilters;

function filterOf(target: RefreshTarget): InvalidateQueryFilters {
  return Array.isArray(target) ? { queryKey: target } : (target as InvalidateQueryFilters);
}

/**
 * Makes each matched query stale and refetches the active ones, after each
 * fetch that is in flight now settles.
 *
 * A plain invalidation is not enough. A query with no data yet reuses the
 * fetch in flight (query-core `Query.fetch`), so a change that arrived after
 * that fetch began would settle as fresh. A cancel before the invalidation is
 * not correct either: a query with no data reverts to its first state, and
 * each `fetchQuery` caller that waits on that first fetch gets a
 * `CancelledError`. Thus this waits for the fetch, then invalidates. The
 * fetch that follows begins after the change, so it reads the change.
 */
export async function refreshQueries(client: QueryClient, targets: readonly RefreshTarget[]): Promise<void> {
  await Promise.all(targets.map(async target => {
    const filter = filterOf(target);
    const inFlight = client.getQueryCache().findAll({ ...filter, fetchStatus: 'fetching' });
    // A failed fetch is still a settled fetch; the refetch below reports it.
    await Promise.all(inFlight.map(query => query.promise?.catch(() => undefined)));
    await client.invalidateQueries(filter);
  }));
}
