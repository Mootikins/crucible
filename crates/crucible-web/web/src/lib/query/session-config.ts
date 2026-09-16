import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  getContextStrategy,
  getPrecognition,
  getSessionStatus,
  listAgentOptions,
  listKnobs,
  setAgentOption,
  setContextStrategy,
  setPrecognition,
  type SessionStatusSlot,
} from '@/lib/api';
import type { AgentConfigOptions, SessionKnobSupport } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * What one session may be configured to do, and the writes that configure it.
 *
 * Five questions, one per key, and they are separate because the daemon
 * answers them at five routes: which settings this session supports at all,
 * the settings its external agent declares for itself, whether precognition
 * runs, how the context is assembled, and the status slots plugins published.
 *
 * Every panel read them in its own `onMount`, from whichever session was
 * current at the moment it mounted. Two consequences: reopening a panel paid
 * for every round trip again, and a panel left open across a session switch
 * kept showing the settings of the session the user left. The id is an
 * accessor here, so each read follows the selection, and the panels share one
 * answer per session.
 *
 * This is session config, not the app config of `lib/query/config.ts`. Two
 * unrelated entities, which is why Part F gave this one its own file name.
 */

/** The shape of a read that takes one session id and nothing else. */
function sessionQuery<T>(
  id: Accessor<string | null>,
  key: (sessionId: string) => readonly unknown[],
  read: (sessionId: string) => Promise<T>,
): UseQueryResult<T, Error> {
  return useQuery(
    () => {
      const sessionId = id();
      return {
        queryKey: key(sessionId ?? ''),
        queryFn: () => read(sessionId as string),
        enabled: sessionId !== null && sessionId !== '',
      };
    },
    () => getQueryClient(),
  );
}

/** Which settings this session can change, as the daemon declares them. */
export function useSessionKnobs(
  id: Accessor<string | null>,
): UseQueryResult<SessionKnobSupport, Error> {
  return sessionQuery(id, keys.sessionKnobs, listKnobs);
}

/**
 * The settings this session's external agent advertised for itself.
 *
 * A daemon that predates the route answers 404, and an internal session has no
 * agent options at all. Neither is a failure the panel should report, so both
 * answer an empty list — which is what the panel's own `catch` did before the
 * read moved here.
 */
export function useAgentOptions(
  id: Accessor<string | null>,
): UseQueryResult<AgentConfigOptions, Error> {
  return sessionQuery(id, keys.sessionAgentOptions, (sessionId) =>
    listAgentOptions(sessionId).catch(() => ({ session_id: sessionId, options: [] })),
  );
}

/**
 * Sends one of the agent's own settings back to it.
 *
 * No optimistic patch, on purpose: the agent may clamp or normalise what it is
 * sent, and it is the only authority on what the value became. The list is
 * re-read instead.
 */
export function useSetAgentOption(): UseMutationResult<
  void,
  Error,
  { id: string; optionId: string; value: string }
> {
  return useMutation(
    () => ({
      mutationFn: ({ id, optionId, value }: { id: string; optionId: string; value: string }) =>
        setAgentOption(id, optionId, value),
      onSuccess: (_result: void, { id }: { id: string; optionId: string; value: string }) =>
        getQueryClient().invalidateQueries({ queryKey: keys.sessionAgentOptions(id) }),
    }),
    () => getQueryClient(),
  );
}

/** Whether this session injects context before a turn. */
export function useGetPrecognition(
  id: Accessor<string | null>,
): UseQueryResult<boolean, Error> {
  return sessionQuery(id, keys.sessionPrecognition, getPrecognition);
}

/**
 * Turns precognition on or off.
 *
 * The toggle moves before the daemon answers, because the control must respond
 * to the click, and moves back when the daemon refuses: a setting nothing
 * applies must not look applied.
 */
export function useSetPrecognition(): UseMutationResult<
  void,
  Error,
  { id: string; enabled: boolean },
  { replaced: boolean | undefined }
> {
  const hold = (id: string, enabled: boolean): boolean | undefined => {
    let replaced: boolean | undefined;
    getQueryClient().setQueryData<boolean>(keys.sessionPrecognition(id), (held) => {
      replaced = held;
      return enabled;
    });
    return replaced;
  };

  return useMutation(
    () => ({
      mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
        setPrecognition(id, enabled),
      onMutate: ({ id, enabled }: { id: string; enabled: boolean }) => ({
        replaced: hold(id, enabled),
      }),
      onError: (
        _error: Error,
        { id }: { id: string; enabled: boolean },
        context: { replaced: boolean | undefined } | undefined,
      ) => {
        if (context?.replaced !== undefined) hold(id, context.replaced);
      },
      onSettled: (
        _result: void | undefined,
        _error: Error | null,
        { id }: { id: string; enabled: boolean },
      ) => getQueryClient().invalidateQueries({ queryKey: keys.sessionPrecognition(id) }),
    }),
    () => getQueryClient(),
  );
}

/** How this session assembles its context, or null while it has no setting. */
export function useGetContextStrategy(
  id: Accessor<string | null>,
): UseQueryResult<string | null, Error> {
  return sessionQuery(id, keys.sessionContextStrategy, getContextStrategy);
}

/**
 * Sets the context-assembly strategy.
 *
 * The daemon parses the string and refuses one it does not know, so the held
 * value is replaced only once it accepts, and then re-read: the daemon may
 * store a name other than the one the dropdown sent.
 */
export function useSetContextStrategy(): UseMutationResult<
  void,
  Error,
  { id: string; strategy: string }
> {
  return useMutation(
    () => ({
      mutationFn: ({ id, strategy }: { id: string; strategy: string }) =>
        setContextStrategy(id, strategy),
      onSuccess: (_result: void, { id, strategy }: { id: string; strategy: string }) => {
        const client = getQueryClient();
        client.setQueryData<string | null>(keys.sessionContextStrategy(id), strategy);
        return client.invalidateQueries({ queryKey: keys.sessionContextStrategy(id) });
      },
    }),
    () => getQueryClient(),
  );
}

/**
 * The status slots plugins published for one session.
 *
 * A refused read is no chips and never a notification: it fails on every
 * daemon reconnect, and a session with nothing to say is the normal case. The
 * key carries the session id, so the chips of the session the user left cannot
 * linger over the one they opened.
 */
export function useSessionStatus(
  id: Accessor<string | null>,
): UseQueryResult<SessionStatusSlot[], Error> {
  return sessionQuery(id, keys.sessionStatus, (sessionId) =>
    getSessionStatus(sessionId).catch(() => []),
  );
}
