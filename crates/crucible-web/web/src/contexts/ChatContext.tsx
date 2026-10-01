import {
  createContext,
  useContext,
  ParentComponent,
  createEffect,
  createMemo,
  on,
  onCleanup,
  type JSX,
} from 'solid-js';
import type { InteractionResponse, ChatMode, ModeDescriptor, SubagentEvent } from '@/lib/types';
import type { ChatContextValue } from '@/lib/types/context';
import { fetchPendingInteractionsOnce, useRespondToInteraction } from '@/lib/query/interactions';
import { useCancelSession } from '@/lib/query/sessions';
import { useExecuteCommand } from '@/lib/query/commands';
import { commandResultText } from '@/lib/slash-commands';
import {
  fetchSessionHistoryOnce,
  useSendChatMessage,
  useSessionHistory,
} from '@/lib/query/history';
import { useSessionModes, useSetSessionMode } from '@/lib/query/modes';
import { consumePendingFirstMessage, peekPendingFirstMessage } from '@/lib/draft-session';
import { getBus } from '@/lib/bus';
import type { CommentRef } from '@/lib/diffset';
import { statusBarStore } from '@/stores/statusBarStore';
import { notificationActions } from '@/stores/notificationStore';
import { attentionActions } from '@/stores/attentionStore';
import {
  addLocalRow,
  addOptimisticTurn,
  confirmOptimisticTurn,
  daemonTranscriptOf,
  dropOptimisticTurn,
  failOptimisticTurn,
  patchTranscript,
  queueTurn,
  removeQueuedTurn,
  prioritizeQueuedTurn,
  releaseTranscript,
  renderTranscript,
  retainTranscript,
  retryTranscriptStream,
  seedTranscript,
  setOptimisticQueued,
  setTranscriptPendingInteraction,
  setTranscriptStreaming,
  shiftQueuedTurn,
  transcriptOf,
  transcriptOpened,
  type QueuedTurn,
} from './transcriptStore';
import { bootstrapSessionWithFallback } from './sessionBootstrap';
import { FALLBACK_MODES } from '@/components/ChatModeControl';

interface ChatProviderProps {
  sessionId: string;
  children: JSX.Element;
}

const ChatContext = createContext<ChatContextValue>();

export const ChatProvider: ParentComponent<ChatProviderProps> = (props) => {
  // The one cancel, shared with `SessionContext`'s stop control: two panes
  // stopping one turn send one shape of request.
  const cancel = useCancelSession();
  // The persisted transcript, from the one key every pane reads. Two panes on
  // this session share the request, and a rebind is a change of key rather
  // than an abort and a second read of the same transcript.
  const history = useSessionHistory(() => props.sessionId || null);
  const send = useSendChatMessage();
  // The session's state — the daemon's transcript, the optimistic entries,
  // the streaming state — keyed by session id in `transcriptStore`, shared by
  // every pane that shows this session and fed by that store's one stream
  // subscription. This pane holds it open and draws it; it does not own it.
  const transcript = () => transcriptOf(props.sessionId);
  /** The rows this pane draws, mapped from the daemon's transcript. */
  const messages = createMemo(() =>
    renderTranscript(daemonTranscriptOf(props.sessionId), transcript()),
  );
  const subagentEvents = createMemo(() =>
    messages().flatMap((m): SubagentEvent[] => (m.delegation ? [m.delegation] : [])),
  );
  // Answering a request takes it out of the shared pending list, so the read
  // below cannot hand back one this client already answered. That is what
  // retires the set of answered request ids this provider used to keep.
  const respond = useRespondToInteraction();
  // Modes are declared in Lua, so the list is per session and comes from the
  // daemon. It is read through the one key `SessionStatusChips` reads, which
  // fetched a second copy of its own on every mount, and the id is an
  // accessor: the read this replaced ran once, when the pane was built, for
  // whichever session it held then, so a rebound pane kept the first
  // session's modes. Held here rather than in ChatModeControl because
  // Shift+Tab cycles from ChatInput.
  const modes = useSessionModes(() => props.sessionId || null);
  const setMode = useSetSessionMode();
  /**
   * The modes this pane offers.
   *
   * The built-ins stand in until the daemon answers, and stay if it answers an
   * empty list: a chip with no options offers no way to change mode at all.
   */
  const availableModes = (): ModeDescriptor[] => {
    const listed = modes.data;
    return listed && listed.modes.length > 0 ? listed.modes : FALLBACK_MODES;
  };
  /**
   * Follows the mode the daemon says is current.
   *
   * `session.get` answers whatever was last written, and the daemon clamps to
   * a mode that still exists — so the persisted string can name a mode nobody
   * would run, and the chip showed its own placeholder for it. The list is
   * the authority, and the stream's route asks for it again whenever the mode
   * moves.
   */
  createEffect(() => {
    const listed = modes.data;
    if (listed && listed.modes.length > 0) {
      patchTranscript(props.sessionId, { chatMode: listed.current_mode_id });
    }
  });
  /**
   * This pane has nothing to draw and is waiting for the transcript.
   *
   * The query's own state. A bind onto a session this browser read already
   * paints from the cache instead of showing the skeleton a second time, and a
   * refetch of a document this pane has folded is not a load.
   */
  const isLoadingHistory = () => history.isLoading;

  /**
   * The mark that this bind is superseded, which a late answer checks.
   *
   * It aborts no request any more. The transcript is a query keyed by session
   * id, so an answer for the session this pane has left writes to that
   * session's key and never onto the transcript now on screen.
   */
  let bindAbortController: AbortController | null = null;
  let previousSessionId: string | null = null;

  /** UI-optimistic mode switch that also persists daemon-side. The daemon
   * echoes a mode_changed SSE event; on failure the UI reverts and surfaces
   * the error (plan mode that isn't enforced server-side must not look on). */
  const switchMode = (mode: ChatMode) => {
    const previous = transcript().chatMode;
    patchTranscript(props.sessionId, { chatMode: mode });
    // The mutation moves the cached `current_mode_id` and puts it back on a
    // refusal, which is what the other readers of the list see. This pane
    // holds its own chip, because the chip must answer the click whether or
    // not the daemon has answered the list at all. The mutation also asks
    // for the list again once it settles, so a mode the daemon rejects stops
    // being offered.
    void setMode.mutateAsync({ id: props.sessionId, mode }).catch((err) => {
      patchTranscript(props.sessionId, { chatMode: previous });
      notificationActions.addNotification(
        'error',
        err instanceof Error ? err.message : 'Failed to set session mode',
      );
    });
  };

  const addSystemMessage = (content: string) => {
    addLocalRow(props.sessionId, { role: 'system', content });
  };

  /**
   * Binds this pane to one session: the transcript it holds open, the
   * bootstrap, the history and the staged first message.
   *
   * `on` names the ONE dependency, and it is not a tidiness: the body reports
   * to the attention store, which reads the title, and the bootstrap writes
   * that title. A plain effect therefore tracked a signal its own bootstrap
   * changed, re-ran for the same session, and staged the draft's first message
   * a second time — the user saw their own turn twice. The reads inside are
   * deliberately untracked; only a new session id may bind again.
   */
  createEffect(
    on(
      () => props.sessionId,
      (newSessionId) => {
        // Supersede the bind before it: its remaining reads must write nothing.
        if (bindAbortController) {
          bindAbortController.abort();
          bindAbortController = null;
        }
        // The transcript this pane was drawing belongs to the session it leaves;
        // the keyed store hands this bind the transcript of ITS session, so there
        // is nothing to clear — only the hold to give back. The last pane out
        // frees the state, the same refcount the SSE root keeps on the source.
        if (newSessionId !== previousSessionId && previousSessionId !== null) {
          releaseTranscript(previousSessionId);
          attentionActions.clear(previousSessionId);
        }
        previousSessionId = newSessionId;

        if (!newSessionId) {
          return;
        }
        retainTranscript(newSessionId);

        const abortController = new AbortController();
        bindAbortController = abortController;

        const bootstrapPromise = bootstrapSessionWithFallback({
          sessionId: newSessionId,
          signal: abortController.signal,
          setSessionTitle: (title) => patchTranscript(newSessionId, { sessionTitle: title }),
          setChatMode: (mode) => patchTranscript(newSessionId, { chatMode: mode }),
          // The one request `useSessionHistory` is making for this session, under
          // the key it reads. The bind awaits the document, and the effect below
          // puts its transcript on screen.
          loadHistory: (id) => fetchSessionHistoryOnce(id).then(() => undefined),
        });

        // The stream carries only NEW interaction requests. A request the daemon
        // still holds from before a reload never arrives on it, so the composer
        // showed no card while the daemon waited and every send answered 422. Ask
        // the pending aggregate once on bind. A request that arrived on the stream
        // in the meantime wins, so this never overwrites a live one.
        // `Promise.resolve().then` keeps a synchronous throw (a test double with
        // no such function) on the rejection path instead of inside the effect.
        void Promise.resolve()
          .then(() => fetchPendingInteractionsOnce())
          .then((entries) => {
            if (abortController.signal.aborted || props.sessionId !== newSessionId) return;
            const held = entries.find((e) => e.session_id === newSessionId);
            if (held && !transcript().pendingInteraction) {
              setTranscriptPendingInteraction(newSessionId, held.request);
            }
          })
          .catch(() => {
            /* The aggregate is a courtesy; the stream still delivers new requests. */
          });

        // Resolves when the SSE stream is open (daemon subscribed). Sending
        // before that drops the response's first tokens — the turn then looks
        // frozen until message_complete backfills the full text. The stream
        // belongs to the session's transcript, so the gate is the session's,
        // shared with every pane that holds it.
        const sseOpen = transcriptOpened(newSessionId);

        // Lazy creation handoff: the draft surface staged the user's first
        // message before opening this session. Send it only after (a) bootstrap
        // and (b) the SSE stream is open, so the response streams from token one.
        // The timeout keeps the message from being stuck if SSE can't connect.
        // PEEK (non-destructive) so the optimistic turn renders on EVERY mount —
        // the handoff can race a panel remount, and a destructive read here let a
        // short-lived first mount swallow the message while the surviving mount
        // showed an empty transcript for seconds. The destructive consume happens
        // at dispatch time below: first dispatcher wins, any zombie sibling gets
        // undefined and skips, so the message renders instantly everywhere and is
        // sent exactly once.
        const pendingFirstMessage = peekPendingFirstMessage(newSessionId);
        if (pendingFirstMessage) {
          // Show the user's message + working indicator IMMEDIATELY — only the
          // POST waits for the gates below.
          const tempId = insertOptimisticTurn(pendingFirstMessage);
          const sseOpenOrTimeout = Promise.race([
            sseOpen,
            new Promise<void>((resolve) => setTimeout(resolve, 5000)),
          ]);
          void Promise.all([bootstrapPromise.catch(() => {}), sseOpenOrTimeout]).then(() => {
            const message = consumePendingFirstMessage(newSessionId);
            if (message) void dispatchTurn(message, tempId);
          });
        }
      },
    ),
  );

  /**
   * Puts the daemon's transcript on screen.
   *
   * Each document the history query answers is a snapshot of the daemon's
   * fold. The store keeps its own copy when the copy is newer, so a slow read
   * cannot move the transcript back.
   */
  createEffect(() => {
    const document = history.data;
    // A rebind can show the document of the session before for a moment.
    if (!document || !props.sessionId || document.session_id !== props.sessionId) return;
    // A server that predates the field serves no transcript.
    seedTranscript(
      props.sessionId,
      document.transcript ?? { as_of_seq: 0, items: [] },
      history.dataUpdatedAt,
    );
  });

  onCleanup(() => {
    if (bindAbortController) {
      bindAbortController.abort();
      bindAbortController = null;
    }
    if (props.sessionId) {
      releaseTranscript(props.sessionId);
      attentionActions.clear(props.sessionId);
    }
  });

  // Whoever answered a request — this pane, another pane, or the inbox on
  // this session's behalf — the write announces it, and the pane holding the
  // card drops it. `on` removes the handler with this owner.
  getBus().on('interactionResolved', ({ sessionId, requestId }) => {
    if (sessionId === props.sessionId && transcript().pendingInteraction?.id === requestId) {
      setTranscriptPendingInteraction(props.sessionId, null);
    }
  });

  // Palette "Clear Chat" / Ctrl+K is `/clear`: the daemon clears the model
  // context, and its context_cleared draws the divider in each pane. Multiple
  // chat providers can be mounted (split panes); only the one showing the
  // active session sends it.
  const runCommand = useExecuteCommand(() => props.sessionId);
  getBus().on('clearChat', () => {
    if (props.sessionId && statusBarStore.activeSessionId() === props.sessionId) {
      runCommand.mutate('/clear', {
        onError: (err) => patchTranscript(props.sessionId, { error: err.message }),
      });
    }
  });

  // The optimistic entry goes in BEFORE the POST, so the user sees the
  // message and the working indicator at once. The daemon's user turn with
  // the id that the send answers then replaces it.
  const insertOptimisticTurn = (trimmed: string): string => {
    patchTranscript(props.sessionId, { error: null, isLoading: true });
    setTranscriptStreaming(props.sessionId, true);
    return addOptimisticTurn(props.sessionId, trimmed);
  };

  /** Ends the busy state of a send that opened no turn. */
  const settleIdle = () => {
    setTranscriptStreaming(props.sessionId, false);
    patchTranscript(props.sessionId, { isLoading: false });
  };

  const dispatchTurn = async (trimmed: string, tempId: string, comments?: CommentRef[]) => {
    if (!props.sessionId) return;
    try {
      const outcome = await send.mutateAsync({ id: props.sessionId, message: trimmed, comments });
      // A command that the daemon ran without a turn: no turn opened, so the
      // optimistic entry goes, and the result shows as a system line.
      if (outcome.outcome === 'command') {
        dropOptimisticTurn(props.sessionId, tempId);
        settleIdle();
        addSystemMessage(`/${outcome.command}: ${commandResultText(outcome.result)}`);
        return;
      }
      confirmOptimisticTurn(props.sessionId, tempId, outcome.message_id);
    } catch (err) {
      console.error('Failed to send message:', err);
      // A refusal the daemon words "Concurrent request in progress" is a lost
      // admission race, not a lost message: a foreign client's turn (or a
      // cancel still winding down) held the one slot. Park the prompt back in
      // the queue — its entry is already on screen — and the flusher retries
      // when the stream goes idle. The turn that holds the slot keeps the
      // session streaming, and its `turn_finished` ends that state.
      if (err instanceof Error && /concurrent request/i.test(err.message)) {
        patchTranscript(props.sessionId, { isLoading: false });
        queueTurn(props.sessionId, trimmed, tempId, comments);
        return;
      }
      const errorMsg = err instanceof Error ? err.message : 'Failed to connect to server';
      patchTranscript(props.sessionId, { error: errorMsg });
      // Keep the user's text visible next to the failure notice.
      failOptimisticTurn(props.sessionId, tempId, `Failed to send: ${errorMsg}`);
      settleIdle();
    }
  };

  /** A turn runs, or a send waits for its answer. */
  const busy = () => transcript().isLoading || transcript().isStreaming;

  const sendMessage = async (content: string, comments?: CommentRef[]) => {
    const attached = comments?.length ? comments : undefined;
    if ((!content.trim() && !attached) || !props.sessionId) return;
    const trimmed = content.trim();
    // A turn in flight holds the daemon's one request slot — posting now
    // would be refused as concurrent at best, and interleaved into the
    // running turn at worst. Queue instead: the optimistic entry renders at
    // the end of the transcript immediately, and the flusher below
    // dispatches it as its own turn once the stream goes idle.
    if (busy()) {
      queueTurn(props.sessionId, trimmed, undefined, attached);
      return;
    }
    await dispatchTurn(trimmed, insertOptimisticTurn(trimmed), attached);
  };

  /** Turns the oldest queued prompt into a running turn. */
  const beginQueuedTurn = (entry: QueuedTurn) => {
    if (!props.sessionId) return;
    patchTranscript(props.sessionId, { error: null, isLoading: true });
    setTranscriptStreaming(props.sessionId, true);
    setOptimisticQueued(props.sessionId, entry.tempId, false);
    void dispatchTurn(entry.content, entry.tempId, entry.comments);
  };

  // The queue's drain pump. Every idle moment with a non-empty queue starts
  // exactly one queued turn: `shiftQueuedTurn` is the claim, so two panes of
  // one session — which both run this effect over the shared state —
  // cannot dispatch the same prompt twice.
  createEffect(() => {
    if (!props.sessionId) return;
    const t = transcript();
    if (t.isLoading || t.isStreaming || t.queuedTurns.length === 0) return;
    const entry = shiftQueuedTurn(props.sessionId);
    if (entry) beginQueuedTurn(entry);
  });

  const respondToInteraction = async (response: InteractionResponse) => {
    const request = transcript().pendingInteraction;
    if (!request || !props.sessionId) return;

    setTranscriptPendingInteraction(props.sessionId, null);

    try {
      await respond.mutateAsync({
        sessionId: props.sessionId,
        requestId: request.id,
        response,
      });
    } catch (err) {
      console.error('Failed to send interaction response:', err);
      patchTranscript(props.sessionId, {
        error: err instanceof Error ? err.message : 'Failed to respond',
      });
    }
  };

  const cancelStream = async () => {
    if (props.sessionId) {
      try {
        await cancel.mutateAsync(props.sessionId);
      } catch (err) {
        console.error('Failed to cancel session:', err);
        // The daemon is unreachable, so no `turn_finished` event will arrive
        // to close the turn — drop the streaming flags here or the spinner
        // spins forever. Any other path leaves the closing to the reducer's
        // `turn_finished` case, which every subscribed pane receives,
        // including foreign cancellations this client never issued.
        settleIdle();
      }
    }
  };

  const removeQueuedMessage = (id: string) => removeQueuedTurn(props.sessionId, id);
  const sendQueuedMessageNow = async (id: string) => {
    const wasBusy = busy();
    if (!prioritizeQueuedTurn(props.sessionId, id) || !wasBusy) return;
    try {
      // Only the daemon's idle event releases the selected prompt. Cancelling
      // does not grant permission to interleave it into the current turn.
      await cancel.mutateAsync(props.sessionId);
    } catch (err) {
      patchTranscript(props.sessionId, {
        error: err instanceof Error ? err.message : 'Failed to interrupt the current turn',
      });
    }
  };

  /**
   * Skip the backoff wait.
   *
   * `reconnect()` clears the pending timer AND closes the dead EventSource, so
   * this is a real re-issue of the connect, not a cosmetic one. It keeps every
   * subscriber, so a pane does not drop its handler to get a fresh source, and
   * the other panes of this session come back with it. The status is NOT set
   * optimistically: the new stream emits `connection/connected` when it opens,
   * and `connection/reconnecting` when it fails again, so the banner reports
   * what happened rather than what we hoped.
   */
  const retryConnection = () => {
    if (props.sessionId) retryTranscriptStream(props.sessionId);
  };

  const value: ChatContextValue = {
    sessionId: () => props.sessionId,
    messages,
    isLoading: () => transcript().isLoading,
    isStreaming: () => transcript().isStreaming,
    pendingInteraction: () => transcript().pendingInteraction,
    error: () => transcript().error,
    connectionStatus: () => transcript().connectionStatus,
    retryConnection,
    subagentEvents,
    chatMode: () => transcript().chatMode,
    availableModes,
    isLoadingHistory,
    setChatMode: (mode: ChatMode) => patchTranscript(props.sessionId, { chatMode: mode }),
    switchMode,
    sendMessage,
    removeQueuedMessage,
    sendQueuedMessageNow,
    respondToInteraction,
    cancelStream,
    addSystemMessage,
  };

  return <ChatContext.Provider value={value}>{props.children}</ChatContext.Provider>;
};

export function useChat(): ChatContextValue {
  const context = useContext(ChatContext);
  if (!context) {
    throw new Error('useChat must be used within a ChatProvider');
  }
  return context;
}

const noopAsync = async () => {};

const fallbackChatContext: ChatContextValue = {
  sessionId: () => undefined,
  messages: () => [],
  isLoading: () => false,
  isStreaming: () => false,
  pendingInteraction: () => null,
  error: () => null,
  connectionStatus: () => 'connected',
  retryConnection: () => {},
  subagentEvents: () => [],
  chatMode: () => 'ask',
  availableModes: () => FALLBACK_MODES,
  isLoadingHistory: () => false,
  setChatMode: () => {},
  switchMode: () => {},
  sendMessage: noopAsync,
  removeQueuedMessage: () => {},
  sendQueuedMessageNow: noopAsync,
  respondToInteraction: noopAsync,
  cancelStream: noopAsync,
  addSystemMessage: () => {},
};

export function useChatSafe(): ChatContextValue {
  const context = useContext(ChatContext);
  return context ?? fallbackChatContext;
}
