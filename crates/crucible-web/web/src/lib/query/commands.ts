import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { executeCommand, type CommandResult, type SessionCommand } from '@/lib/api';
import { rpc } from '@/lib/api-client';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The command catalog of a session, and the call that runs a built-in command.
 *
 * The catalog is per session: modes, plugins, skills and the agent's own
 * commands differ between sessions. It moves only when the daemon says so
 * (`commands_changed`) or when this client reloads a plugin, so `staleTime`
 * is `Infinity` and those two paths invalidate it.
 *
 * A refused fetch leaves no data, so the next keystroke asks again.
 */

/** The options the hook and the promise read both use. */
function commandsQueryOptions(sessionId: string) {
  return {
    queryKey: keys.slashCommands(sessionId),
    queryFn: async () => (await rpc('session.commands', { session_id: sessionId })).commands,
    staleTime: Number.POSITIVE_INFINITY,
  };
}

/** The catalog as a query, for a caller that can render a pending state. */
export function useSlashCommands(
  sessionId: Accessor<string>,
): UseQueryResult<SessionCommand[], Error> {
  return useQuery(() => commandsQueryOptions(sessionId()), getQueryClient);
}

/**
 * The catalog as a promise, for the composer's autocomplete.
 *
 * `useAutocomplete` reaches this from an async keystroke handler with no owner
 * to mount an observer on, so it cannot hold a query. It joins the same entry
 * as `useSlashCommands()` rather than starting a request of its own.
 */
export function fetchSlashCommandsOnce(sessionId: string): Promise<SessionCommand[]> {
  // `revalidateIfStale` is what makes an invalidation reach this caller.
  return getQueryClient().ensureQueryData({
    ...commandsQueryOptions(sessionId),
    revalidateIfStale: true,
  });
}

/**
 * Drops the held catalogs, so the next read asks the daemon again: every
 * session's, or one session's.
 */
export function resetCommandCache(sessionId?: string): Promise<void> {
  return getQueryClient().invalidateQueries({ queryKey: keys.slashCommands(sessionId) });
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
