import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { listPendingInteractions, respondToInteraction } from '@/lib/api';
import type { PendingInteractionEntry } from '@/lib/types';
import { getBus } from '@/lib/bus';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * Every request the daemon is holding for a human, across sessions, and the
 * answer that clears one.
 *
 * The header badge, Inbox, and chat panes share one aggregate. A pane reads
 * it from the daemon on attach; the session event stream invalidates the key
 * when a new request arrives. The query also refreshes on window focus.
 */

/** How long one answer is remembered when the daemon keeps listing it. */
const ANSWERED_TTL_MS = 30_000;

/**
 * The requests this client answered, by id, with the moment it answered them.
 *
 * The daemon purges an answered request from its aggregate when it gets round
 * to it, and until then the aggregate still lists it. That lag is harmless
 * while nothing re-reads the list — but `routes/session.ts` invalidates this
 * key whenever ANY session raises a request, so a refetch lands in the middle
 * of the lag and writes the answered entry back. The badge lights again and
 * the pane that answered raises the card a second time.
 *
 * So an answered id is held here and filtered out of what every reader sees.
 * The record is short-lived in both directions: the first answer from the
 * daemon that no longer lists the id drops it, and the timeout drops it
 * anyway, so a daemon that never purges cannot silence that session for the
 * life of the tab. This is what `ChatContext`'s per-provider set of answered
 * ids used to do, for one pane only.
 */
const answered = new Map<string, number>();

/** Drops the ids whose answer is older than the timeout. */
function expireAnswered(now: number): void {
  for (const [id, at] of answered) {
    if (now - at > ANSWERED_TTL_MS) answered.delete(id);
  }
}

/**
 * The list without the requests this client answered.
 *
 * `select` applies it to every reactive reader. The two promise readers below
 * apply it themselves, because `select` does not run for an imperative read —
 * and a chat pane binding mid-lag is exactly the reader that must not see one.
 */
function withoutAnswered(entries: PendingInteractionEntry[]): PendingInteractionEntry[] {
  expireAnswered(Date.now());
  if (answered.size === 0) return entries;
  return entries.filter((entry) => !answered.has(entry.request_id));
}

/**
 * Reads the aggregate, and forgets the answers the daemon has caught up with.
 *
 * The pruning belongs here and not in the filter: the filter also runs against
 * a list this client has already patched, where an answered id is missing
 * because the patch removed it, which says nothing about what the daemon
 * holds.
 */
async function fetchPending(): Promise<PendingInteractionEntry[]> {
  const entries = await listPendingInteractions();
  const listed = new Set(entries.map((entry) => entry.request_id));
  expireAnswered(Date.now());
  for (const id of [...answered.keys()]) {
    if (!listed.has(id)) answered.delete(id);
  }
  return entries;
}

/**
 * The aggregate as a query.
 *
 * `refetchOnWindowFocus` is on for this one key, against the app default: the
 * store also re-read the aggregate on `visibilitychange`, because a tab that
 * was hidden for an hour must not show an hour-old badge on the first glance.
 * The stream does not deliver events while the document is hidden.
 */
export function usePendingInteractions(): UseQueryResult<PendingInteractionEntry[], Error> {
  return useQuery(
    () => ({
      queryKey: keys.pendingInteractions(),
      queryFn: fetchPending,
      select: withoutAnswered,
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
 * waited and every send was refused. A bind reads the daemon even when the
 * aggregate has a cached answer, so an older cache cannot hide a prompt.
 */
export function fetchPendingInteractionsOnce(): Promise<PendingInteractionEntry[]> {
  return refetchPendingInteractions();
}

/**
 * Reads the aggregate from the daemon now, whatever the cache holds.
 *
 * For the caller that means "re-sync", not "read": the badge after a window
 * the tab spent hidden, or a test driving the store. Every other reader of
 * the key sees the answer, because it lands in the shared cache.
 */
export function refetchPendingInteractions(): Promise<PendingInteractionEntry[]> {
  return getQueryClient()
    .fetchQuery({
      queryKey: keys.pendingInteractions(),
      queryFn: fetchPending,
      staleTime: 0,
    })
    .then(withoutAnswered);
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
 * may briefly lag an answer, so a refetch here would put the answered
 * request back — and a pane binding in that window would raise a card for a
 * request that is already answered.
 *
 * The id is also recorded in `answered` above, because this client does not
 * own the next refetch: any session raising a request invalidates this key,
 * and that refetch lands in the same lag. The record is what keeps the stale
 * entry off every reader until the daemon agrees. It replaces the set of
 * answered ids `ChatContext` kept for one pane.
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
      onMutate: ({ requestId }: Answer) => {
        answered.set(requestId, Date.now());
        return { replaced: dropPending(requestId) };
      },
      onError: (
        _error: Error,
        { requestId }: Answer,
        context: { replaced: PendingInteractionEntry[] | undefined } | undefined,
      ) => {
        // The request was not answered, so it is still waiting on the user:
        // the list goes back and nothing about it is held.
        answered.delete(requestId);
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

/**
 * Forgets every answer this module is holding.
 *
 * The map is module state, so one test's answer would filter the next test's
 * list. Production code never calls it.
 */
export function resetInteractionsForTests(): void {
  answered.clear();
}
