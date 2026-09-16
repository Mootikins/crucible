import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { listModes, setSessionMode } from '@/lib/api';
import type { SessionModes } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The modes one session declares, and the write that changes the current one.
 *
 * Modes are declared in Lua, so the list belongs to the session and comes
 * from the daemon. Two owners read it: `contexts/ChatContext.tsx`, for the
 * cycle on Shift+Tab, and `components/SessionStatusChips.tsx`, for the review
 * policy the daemon says is in force. The chips fetched the list again on
 * every mount, and the two copies disagreed the moment one of them failed.
 *
 * The context also read it once, at the moment it was built, from the session
 * id it held then — so a pane that rebound to another session kept the first
 * session's modes. The id is an accessor here, so the list follows the bind.
 *
 * `lib/query/routes/session.ts` invalidates this key on `mode_changed`,
 * whoever caused the change: another pane, another client, or the agent.
 */

/**
 * The declared modes of one session, as a query, or none while the id is null.
 */
export function useSessionModes(
  id: Accessor<string | null>,
): UseQueryResult<SessionModes, Error> {
  return useQuery(
    () => {
      const sessionId = id();
      return {
        queryKey: keys.sessionModes(sessionId ?? ''),
        queryFn: () => listModes(sessionId as string),
        enabled: sessionId !== null && sessionId !== '',
      };
    },
    () => getQueryClient(),
  );
}

/**
 * Names the current mode of a cached list, and answers the one it replaced.
 *
 * A list nothing has read is left alone: minting a list from one mode id
 * would answer the next reader a session with exactly one mode.
 */
function nameCurrentMode(sessionId: string, mode: string): string | undefined {
  let replaced: string | undefined;
  getQueryClient().setQueryData<SessionModes>(keys.sessionModes(sessionId), (held) => {
    if (!held) return held;
    replaced = held.current_mode_id;
    return { ...held, current_mode_id: mode };
  });
  return replaced;
}

/**
 * Switches the mode of one session.
 *
 * The chip moves before the daemon answers, because the control must respond
 * to the click, and moves back when the daemon refuses: a plan mode that
 * nothing enforces must not look enabled. The daemon echoes the change as a
 * `mode_changed` event, and the settled write asks for the list again anyway,
 * so a daemon that accepted the mode under another name still has the last
 * word.
 */
export function useSetSessionMode(): UseMutationResult<
  void,
  Error,
  { id: string; mode: string },
  { replaced: string | undefined }
> {
  return useMutation(
    () => ({
      mutationFn: ({ id, mode }: { id: string; mode: string }) => setSessionMode(id, mode),
      onMutate: ({ id, mode }: { id: string; mode: string }) => ({
        replaced: nameCurrentMode(id, mode),
      }),
      onError: (
        _error: Error,
        { id }: { id: string; mode: string },
        context: { replaced: string | undefined } | undefined,
      ) => {
        if (context?.replaced !== undefined) nameCurrentMode(id, context.replaced);
      },
      onSettled: (
        _result: void | undefined,
        _error: Error | null,
        { id }: { id: string; mode: string },
      ) => getQueryClient().invalidateQueries({ queryKey: keys.sessionModes(id) }),
    }),
    () => getQueryClient(),
  );
}
