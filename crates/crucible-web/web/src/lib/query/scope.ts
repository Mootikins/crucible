import {
  useMutation,
  type UseMutationResult,
} from '@tanstack/solid-query';
import { connectSessionKiln, disconnectSessionKiln, type SessionScope } from '@/lib/api';
import { getQueryClient } from './client';
import { keys } from './keys';
import { patchCachedSession } from './sessions';

/**
 * Attaching a kiln to a session, and detaching one.
 *
 * Both routes ECHO the whole scope the session ends up with — its kiln set and
 * its workspace — so neither write needs a read after it. The echo is folded
 * into the cached session row, which is where the scope lives: there is no
 * separate scope document to fetch, and `Session.kilns` is the same field the
 * rail, the chips and the files panel draw.
 *
 * The daemon is the authority on the result, not the caller: it re-checks kiln
 * trust on attach and refuses a mutation mid-turn, so the set it answers can
 * differ from the one the click asked for. That is why nothing is patched
 * optimistically here and the echo is taken whole.
 *
 * The session lists are invalidated too, under both archived variants: a row
 * in either list carries the kilns this write just changed.
 */

/** Folds one echoed scope into every cached copy of that session. */
function applyEcho(scope: SessionScope): Promise<void> {
  patchCachedSession(scope.session_id, {
    kilns: scope.kilns,
    workspace: scope.workspace,
  });
  const client = getQueryClient();
  return Promise.all([
    client.invalidateQueries({ queryKey: keys.sessions(false) }),
    client.invalidateQueries({ queryKey: keys.sessions(true) }),
  ]).then(() => undefined);
}

/** One kiln write, which answers the scope the session ends up with. */
function useScopeMutation(
  write: (sessionId: string, kiln: string) => Promise<SessionScope>,
): UseMutationResult<SessionScope, Error, { id: string; kiln: string }> {
  return useMutation(
    () => ({
      mutationFn: ({ id, kiln }: { id: string; kiln: string }) => write(id, kiln),
      onSuccess: (scope: SessionScope) => applyEcho(scope),
    }),
    () => getQueryClient(),
  );
}

/** Attaches one kiln to a session. Idempotent, as the route is. */
export function useConnectSessionKiln(): UseMutationResult<
  SessionScope,
  Error,
  { id: string; kiln: string }
> {
  return useScopeMutation(connectSessionKiln);
}

/** Detaches one kiln from a session. Any member may be detached. */
export function useDisconnectSessionKiln(): UseMutationResult<
  SessionScope,
  Error,
  { id: string; kiln: string }
> {
  return useScopeMutation(disconnectSessionKiln);
}
