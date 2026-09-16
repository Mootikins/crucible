import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  listPendingInteractions,
  respondToInteraction,
  type PendingInteractionEntry,
} from '@/lib/api';
import { getBus } from '@/lib/bus';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * Every request the daemon is holding for a human, across sessions, and the
 * answer that clears one.
 *
 * Three readers asked for this aggregate separately: `attentionStore` polled
 * it every ten seconds for the header badge, the inbox drew the same entries,
 * and every chat pane read it once on bind to recover a request raised before
 * the page loaded. One key answers all three.
 *
 * The interval stays, at the same ten seconds, and it is a FALLBACK rather
 * than the mechanism. `lib/query/routes/session.ts` invalidates this key on
 * `interaction_requested`, so a session with an open pane raises its card at
 * once — but a session with NO open pane has no stream anyone is subscribed
 * to, and its request would otherwise reach the badge only when something
 * else happened to refetch.
 */

/** The poll the store used to run by hand, kept at its own interval. */
const POLL_INTERVAL_MS = 10_000;

/**
 * The aggregate as a query.
 *
 * `refetchOnWindowFocus` is on for this one key, against the app default: the
 * store also re-read the aggregate on `visibilitychange`, because a tab that
 * was hidden for an hour must not show an hour-old badge on the first glance.
 * The interval alone does not cover that — it does not run while the document
 * is hidden.
 */
export function usePendingInteractions(): UseQueryResult<PendingInteractionEntry[], Error> {
  return useQuery(
    () => ({
      queryKey: keys.pendingInteractions(),
      queryFn: () => listPendingInteractions(),
      refetchInterval: POLL_INTERVAL_MS,
      refetchOnWindowFocus: true,
    }),
    () => getQueryClient(),
  );
}

/**
 * The aggregate as a promise, for a caller that cannot render a pending state.
 *
 * A chat pane binding to a session reads it once, to recover a request the
 * daemon raised before this page existed: the stream carries only NEW
 * requests, so without this the composer showed no card while the daemon
 * waited and every send was refused. It goes through the key the badge polls,
 * so a bind costs nothing while that answer is fresh.
 */
export function fetchPendingInteractionsOnce(): Promise<PendingInteractionEntry[]> {
  return getQueryClient().ensureQueryData({
    queryKey: keys.pendingInteractions(),
    queryFn: () => listPendingInteractions(),
  });
}

/**
 * Reads the aggregate from the daemon now, whatever the cache holds.
 *
 * For the caller that means "re-sync", not "read": the badge after a window
 * the tab spent hidden, or a test driving the store. Every other reader of
 * the key sees the answer, because it lands in the shared cache.
 */
export function refetchPendingInteractions(): Promise<PendingInteractionEntry[]> {
  return getQueryClient().fetchQuery({
    queryKey: keys.pendingInteractions(),
    queryFn: () => listPendingInteractions(),
    staleTime: 0,
  });
}

/** The variables of one answer. */
interface Answer {
  sessionId: string;
  requestId: string;
  response: unknown;
}

/** Takes one request out of the held list, and answers the list it replaced. */
function dropPending(requestId: string): PendingInteractionEntry[] | undefined {
  let replaced: PendingInteractionEntry[] | undefined;
  getQueryClient().setQueryData<PendingInteractionEntry[]>(
    keys.pendingInteractions(),
    (held) => {
      if (!held) return held;
      replaced = held;
      return held.filter((entry) => entry.request_id !== requestId);
    },
  );
  return replaced;
}

/**
 * Answers one request.
 *
 * The entry leaves the cached list at once and the list is NOT re-read
 * afterwards. That is the whole point of the patch: the daemon's aggregate
 * lags an answer by up to one poll, so a refetch here would put the answered
 * request back — and a pane binding in that window would raise a card for a
 * request that is already answered. That lag is what `ChatContext` kept a set
 * of answered request ids for; one shared list replaces it.
 *
 * A refusal puts the list back, because a request the daemon did not accept
 * an answer for is still waiting on the user.
 *
 * The bus carries the result to the panes: the inbox answers on another
 * pane's behalf, and that pane's own card has to go with it.
 */
export function useRespondToInteraction(): UseMutationResult<
  void,
  Error,
  Answer,
  { replaced: PendingInteractionEntry[] | undefined }
> {
  return useMutation(
    () => ({
      mutationFn: ({ sessionId, requestId, response }: Answer) =>
        respondToInteraction(sessionId, requestId, response),
      onMutate: ({ requestId }: Answer) => ({ replaced: dropPending(requestId) }),
      onError: (
        _error: Error,
        _answer: Answer,
        context: { replaced: PendingInteractionEntry[] | undefined } | undefined,
      ) => {
        if (context?.replaced) {
          getQueryClient().setQueryData(keys.pendingInteractions(), context.replaced);
        }
      },
      onSuccess: (_result: void, { sessionId, requestId }: Answer) => {
        getBus().emit('interactionResolved', { sessionId, requestId });
      },
    }),
    () => getQueryClient(),
  );
}
