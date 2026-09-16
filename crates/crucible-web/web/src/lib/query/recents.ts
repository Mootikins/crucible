import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { fetchRecents, recordRecent } from '@/lib/api';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The recently opened files, as one list.
 *
 * One entry under one key: the list is short, it is never indexed by path, and
 * every reader wants the same twenty names. The reason it needs a key at all
 * is the WRITE — opening a file records it, and the list on the empty centre
 * is wrong the moment that lands.
 */

/** One entry of the list, as the panel reads it. */
export interface RecentFile {
  absPath: string;
  name: string;
}

/** What one record names. */
export interface RecordRecentParams {
  readonly path: string;
  readonly name: string;
}

/** The options of the list. */
function recentsOptions() {
  return { queryKey: keys.recents(), queryFn: () => fetchRecents() };
}

/** The recently opened files, newest first. */
export function useFetchRecents(): UseQueryResult<RecentFile[], Error> {
  return useQuery(recentsOptions, getQueryClient);
}

/** The same list, as a promise, for a reader that acts rather than renders. */
export function fetchRecentsOnce(): Promise<RecentFile[]> {
  return getQueryClient().fetchQuery(recentsOptions());
}

/**
 * Records one open, then makes the held list wrong.
 *
 * A refusal is SWALLOWED. The daemon may be an older one with no `/api/recents`
 * at all, and the file is open on screen either way — a caller that opened a
 * file has no answer to give for a failure to remember it. The list is still
 * invalidated, because a daemon that refused this one may have taken the last.
 *
 * It is a plain function beside the hook because its caller is
 * `lib/file-actions.ts`, which opens a file from a menu, a drop or a link and
 * has no component of its own.
 */
export function recordRecentOnce(path: string, name: string): Promise<void> {
  return recordRecent(path, name)
    .catch(() => undefined)
    .then(() =>
      getQueryClient()
        .invalidateQueries({ queryKey: keys.recents() })
        .then(() => undefined),
    );
}

/** The same write, for a caller that wants its pending and error state. */
export function useRecordRecent(): UseMutationResult<void, Error, RecordRecentParams> {
  return useMutation(
    () => ({
      mutationFn: ({ path, name }: RecordRecentParams) => recordRecentOnce(path, name),
    }),
    getQueryClient,
  );
}
