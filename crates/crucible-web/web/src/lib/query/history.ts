import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { getSessionHistory, sendChatMessage } from '@/lib/api';
import type { SessionHistoryResponse } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The persisted transcript of one session, and the write that adds to it.
 *
 * The pane used to own this document. Every `ChatProvider` read the whole
 * history on every bind, through an `AbortController` it cancelled on the next
 * one — so two panes on one session asked twice, and a pane that went back to
 * a session it had already read asked for that transcript again. One key
 * answers every pane, and a bind is a key change rather than an abort and a
 * second request.
 *
 * The document here is NOT the transcript a pane draws. `contexts/
 * chatEventReducer.ts` folds the live stream into the messages of ONE pane:
 * the streaming bubble, the thinking block, the system notice a failed send
 * left. This key holds what the daemon persisted, which every pane and every
 * later bind reads. `contexts/ChatContext.tsx` folds it into its own store
 * once per bind, and the stream carries what follows to all panes at once.
 *
 * `lib/query/routes/session.ts` keeps this key current: it appends the user
 * turn the daemon echoes, and invalidates the document when a turn ends.
 */

/**
 * The whole transcript, always.
 *
 * The server pages from the FRONT, so the default page cuts off the tail of a
 * long agentic turn — the tool results and the assistant's own text. It is a
 * constant and not a parameter because the limit is not part of the key: two
 * callers asking one session for different lengths would overwrite each
 * other's answer under one key, and the shorter one would win at random.
 */
const HISTORY_LIMIT = 10000;

/** The one fetch every reader of one session's transcript shares. */
function fetchHistory(sessionId: string, signal?: AbortSignal): Promise<SessionHistoryResponse> {
  return getSessionHistory(sessionId, HISTORY_LIMIT, undefined, signal);
}

/**
 * The transcript of one session, as a query, or no query while the id is null.
 *
 * The id is an accessor because a pane rebinds while it is mounted: the chat
 * surface follows the session the tab shows, and a pane with no session asks
 * the daemon nothing.
 */
export function useSessionHistory(
  id: Accessor<string | null>,
): UseQueryResult<SessionHistoryResponse, Error> {
  return useQuery(
    () => {
      const sessionId = id();
      return {
        queryKey: keys.sessionHistory(sessionId ?? ''),
        queryFn: ({ signal }: { signal: AbortSignal }) =>
          fetchHistory(sessionId as string, signal),
        enabled: sessionId !== null && sessionId !== '',
      };
    },
    () => getQueryClient(),
  );
}

/**
 * The transcript as a promise, for a caller that cannot render a pending state.
 *
 * The bind awaits the document before it dispatches a message the draft
 * surface staged, so the fold of an empty history cannot land on top of the
 * optimistic turn. It is the same key the hook reads, so the await and the
 * pane's own read are one request.
 */
export function fetchSessionHistoryOnce(sessionId: string): Promise<SessionHistoryResponse> {
  return getQueryClient().ensureQueryData({
    queryKey: keys.sessionHistory(sessionId),
    queryFn: ({ signal }: { signal: AbortSignal }) => fetchHistory(sessionId, signal),
  });
}

/**
 * Sends one turn, and answers the id the daemon minted for it.
 *
 * It patches no cache. The daemon echoes the turn on the chat stream under
 * that same id, and `routes/session.ts` appends it to this session's history
 * under a message-id guard; a second append here would be the same write in
 * two modules, and it could not run any earlier, because the id it needs
 * arrives with the answer to this request. The sending pane shows its own
 * message from its optimistic entry, which the canonical id then replaces.
 */
export function useSendChatMessage(): UseMutationResult<
  string,
  Error,
  { id: string; message: string }
> {
  return useMutation(
    () => ({
      mutationFn: ({ id, message }: { id: string; message: string }) =>
        sendChatMessage(id, message),
    }),
    () => getQueryClient(),
  );
}
