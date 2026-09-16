import { useQuery, type UseQueryResult } from '@tanstack/solid-query';
import { getSurfaces, type Surface } from '@/lib/api';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * Every surface a plugin declared, held once.
 *
 * The panel used to own this list in a `createResource` and keep it current
 * itself: it read the stream, patched its own resource on a withdrawal and
 * refetched on anything else. That worked for one panel. A second panel — a
 * split, or the same surface opened in another tab of the shell — got its own
 * resource, its own fetch, and its own idea of what the roster held.
 *
 * The list is a cache entry now, and the stream's route writes to it
 * (`lib/query/routes/surfaces.ts`). One event reaches every mounted panel, and
 * a panel that mounts after the event reads the corrected list rather than the
 * one it would have fetched a moment earlier.
 */
export function useSurfaces(): UseQueryResult<Surface[], Error> {
  return useQuery(
    () => ({ queryKey: keys.surfaces(), queryFn: getSurfaces }),
    getQueryClient,
  );
}
