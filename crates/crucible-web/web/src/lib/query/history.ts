import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { getSessionHistory, sendChatMessage, type SendOutcome } from '@/lib/api';
import type { SessionHistoryResponse } from '@/lib/types';
import type { CommentRef } from '@/lib/diffset';
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
 * The document carries `transcript`, the daemon's fold of the stored log.
 * `contexts/transcriptStore.ts` puts it on screen, and applies the ops of the
 * `transcript` frames of the stream after it. When an op does not fit, or the
 * stream lost frames, the store reads the document again with
 * `refetchSessionHistory`.
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
        queryFn: ({ signal }: { signal: AbortSignal }) => fetchHistory(sessionId as string, signal),
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
 * A new read of the transcript, for the store that found its copy behind.
 *
 * It ignores the cached answer, because the cached answer is what fell
 * behind. A read already in flight for the key answers both callers.
 */
export function refetchSessionHistory(sessionId: string): Promise<SessionHistoryResponse> {
  return getQueryClient().fetchQuery({
    queryKey: keys.sessionHistory(sessionId),
    queryFn: ({ signal }: { signal: AbortSignal }) => fetchHistory(sessionId, signal),
    staleTime: 0,
  });
}

/**
 * Sends one turn, and answers the id the daemon minted for it.
 *
 * It patches no cache. The daemon echoes the turn as a transcript op under
 * that same id. The sending pane shows its own message from its optimistic
 * entry, and the daemon's item with that id then replaces the entry.
 */
export function useSendChatMessage(): UseMutationResult<
  SendOutcome,
  Error,
  { id: string; message: string; comments?: CommentRef[] }
> {
  return useMutation(
    () => ({
      mutationFn: ({
        id,
        message,
        comments,
      }: {
        id: string;
        message: string;
        comments?: CommentRef[];
      }) => sendChatMessage(id, message, comments),
    }),
    () => getQueryClient(),
  );
}
