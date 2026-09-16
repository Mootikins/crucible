/**
 * The kiln registry, for the callers that hold a name and need a directory.
 *
 * The store this file used to be is gone: the roster is one query now, and
 * `lib/query/kilns.ts` owns the fetch, the cache and the last-known list. The
 * two lookups keep their names here because a per-message component, a memo
 * and a plain helper all import them, and none of them is a query caller.
 */
export { kilnPathOf, mostRecentKilnPath } from '@/lib/query/kilns';
