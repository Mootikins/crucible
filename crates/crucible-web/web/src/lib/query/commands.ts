import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { executeCommand, listSlashCommands, type CommandResult, type SlashCommand } from '@/lib/api';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The slash commands the composer completes, and the call that runs one.
 *
 * The daemon serves the list from the same constant `execute_command`
 * dispatches on, so it cannot change while the daemon runs. That is why
 * `staleTime` is `Infinity` here and nowhere else: a refetch could only return
 * what the cache already holds.
 *
 * The autocomplete used to keep this in a module-level promise, which gave the
 * same answer to every composer and never went stale — the right behaviour,
 * built a second time beside a cache that already does it. The cache also does
 * the part the memo had to do by hand: a refused fetch leaves no data, so the
 * next keystroke asks again rather than reading a poisoned promise.
 */

/** The options the hook and the promise read both use. */
function commandsQueryOptions() {
  return {
    queryKey: keys.slashCommands(),
    queryFn: listSlashCommands,
    staleTime: Number.POSITIVE_INFINITY,
  };
}

/** The command list as a query, for a caller that can render a pending state. */
export function useSlashCommands(): UseQueryResult<SlashCommand[], Error> {
  return useQuery(() => commandsQueryOptions(), getQueryClient);
}

/**
 * The list as a promise, for the composer's autocomplete.
 *
 * `useAutocomplete` reaches this from an async keystroke handler with no owner
 * to mount an observer on, so it cannot hold a query. It joins the same entry
 * as `useSlashCommands()` rather than starting a request of its own.
 */
export function fetchSlashCommandsOnce(): Promise<SlashCommand[]> {
  // `revalidateIfStale` is what makes `resetCommandCache()` reach this caller.
  // Without it a read served the held list whatever its state, and the seam
  // below would mark an entry stale that nothing ever asked about again.
  return getQueryClient().ensureQueryData({
    ...commandsQueryOptions(),
    revalidateIfStale: true,
  });
}

/**
 * Drops the held list, so the next read asks the daemon again.
 *
 * It is the seam the autocomplete's tests drive, and it is the one thing that
 * can make a list with `staleTime: Infinity` move: a daemon restarted under a
 * browser that stayed open serves a different set.
 */
export function resetCommandCache(): Promise<void> {
  return getQueryClient().invalidateQueries({ queryKey: keys.slashCommands() });
}

/**
 * Runs one slash command in one session.
 *
 * It invalidates nothing of its own. What a command changes — the model, the
 * mode, the transcript — belongs to another entity, and that entity's own
 * mutation hook owns the keys the change makes wrong. Naming them here would
 * be a second, weaker copy of those lists.
 *
 * The session id arrives as an accessor: the composer outlives the session on
 * screen, and a mutation bound to the id at mount would send the next command
 * to the session the user just left.
 */
export function useExecuteCommand(
  sessionId: Accessor<string>,
): UseMutationResult<CommandResult, Error, string> {
  return useMutation(
    () => ({ mutationFn: (command: string) => executeCommand(sessionId(), command) }),
    getQueryClient,
  );
}
