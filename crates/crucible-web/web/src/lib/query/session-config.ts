import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  getKnob,
  setKnob,
  type KnobValue,
  type PluginApproval,
  type StatusDisplayItem,
} from '@/lib/api';
import { rpc } from '@/lib/api-client';
import type { AgentConfigOptions, SessionKnobSupport } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

export type { PluginApproval } from '@/lib/api';
export { PLUGIN_APPROVAL_ACTION } from '@/lib/api';

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
  return sessionQuery(id, keys.sessionKnobs, (sessionId) =>
    rpc('session.list_knobs', { session_id: sessionId }),
  );
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
    rpc('session.list_agent_options', { session_id: sessionId }).catch(() => ({
      session_id: sessionId,
      options: [],
    })),
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
      mutationFn: async ({
        id,
        optionId,
        value,
      }: {
        id: string;
        optionId: string;
        value: string;
      }) => {
        await rpc('session.set_agent_option', { session_id: id, option_id: optionId, value });
      },
      onSuccess: (_result: void, { id }: { id: string; optionId: string; value: string }) =>
        getQueryClient().invalidateQueries({ queryKey: keys.sessionAgentOptions(id) }),
    }),
    () => getQueryClient(),
  );
}

/**
 * One knob's value, read from `session.knob.get`.
 *
 * The one generic pair every knob shares: a new knob needs no sibling read
 * hook here, only a call site that names it.
 */
/** The value type of one [`KnobValue`] variant, by its `knob` tag. */
type KnobValueOf<K extends KnobValue['knob']> = Extract<KnobValue, { knob: K }>['value'];

export function useKnob<K extends KnobValue['knob']>(
  id: Accessor<string | null>,
  knob: K,
): UseQueryResult<KnobValueOf<K>, Error> {
  // `KnobValueOf<K>` does not simplify while `K` is a type parameter, so
  // `sessionQuery`'s own generic cannot unify with it at this call site. The
  // cast at the boundary is the one place that limitation is paid; the field
  // the daemon and this file agree on, `value.value`, is still read without
  // a cast.
  type Result = UseQueryResult<KnobValueOf<K>, Error>;
  return sessionQuery(
    id,
    (sessionId) => keys.sessionKnob(sessionId, knob),
    async (sessionId) => (await getKnob(sessionId, knob)).value,
  ) as unknown as Result;
}

/**
 * Writes one knob's value, through `session.knob.set`.
 *
 * The held value moves before the daemon answers, because a control must
 * respond to the click, and moves back when the daemon refuses: a setting
 * nothing applies must not look applied. This is the one generic pair every
 * knob shares; a new knob needs no sibling write hook here.
 */
export function useSetKnob(): UseMutationResult<
  void,
  Error,
  { id: string; value: KnobValue },
  { replaced: unknown }
> {
  const hold = (id: string, value: KnobValue): unknown => {
    let replaced: unknown;
    getQueryClient().setQueryData(keys.sessionKnob(id, value.knob), (held: unknown) => {
      replaced = held;
      return value.value;
    });
    return replaced;
  };

  return useMutation(
    () => ({
      mutationFn: ({ id, value }: { id: string; value: KnobValue }) => setKnob(id, value),
      onMutate: ({ id, value }: { id: string; value: KnobValue }) => ({
        replaced: hold(id, value),
      }),
      onError: (
        _error: Error,
        { id, value }: { id: string; value: KnobValue },
        context: { replaced: unknown } | undefined,
      ) => {
        if (context?.replaced !== undefined) {
          getQueryClient().setQueryData(keys.sessionKnob(id, value.knob), context.replaced);
        }
      },
      onSettled: (
        _result: void | undefined,
        _error: Error | null,
        { id, value }: { id: string; value: KnobValue },
      ) => getQueryClient().invalidateQueries({ queryKey: keys.sessionKnob(id, value.knob) }),
    }),
    () => getQueryClient(),
  );
}

export function usePluginApprovals(
  id: Accessor<string | null>,
): UseQueryResult<Record<string, PluginApproval>, Error> {
  return sessionQuery(
    id,
    keys.sessionPluginApprovals,
    async (sessionId) =>
      (await rpc('session.list_plugin_approvals', { session_id: sessionId })).approvals,
  );
}

export function useSetPluginApproval(): UseMutationResult<
  void,
  Error,
  { id: string; plugin: string; approval: PluginApproval }
> {
  return useMutation(
    () => ({
      mutationFn: async ({
        id,
        plugin,
        approval,
      }: {
        id: string;
        plugin: string;
        approval: PluginApproval;
      }) => {
        await rpc('session.set_plugin_approval', { session_id: id, plugin, approval });
      },
      onSuccess: (
        _result: void,
        { id }: { id: string; plugin: string; approval: PluginApproval },
      ) => getQueryClient().invalidateQueries({ queryKey: keys.sessionPluginApprovals(id) }),
    }),
    () => getQueryClient(),
  );
}

/**
 * The status list of one session: what plugins published and the engine's
 * plugin-turn items.
 *
 * A refused read is no chips and never a notification: it fails on every
 * daemon reconnect, and a session with nothing to say is the normal case. The
 * key carries the session id, so the chips of the session the user left cannot
 * linger over the one they opened.
 */
export function useSessionStatus(
  id: Accessor<string | null>,
): UseQueryResult<StatusDisplayItem[], Error> {
  return sessionQuery(id, keys.sessionStatus, (sessionId) =>
    rpc('session.status', { session_id: sessionId })
      .then((r) => r.status)
      .catch(() => []),
  );
}
