import {
  createContext,
  useContext,
  ParentComponent,
  createEffect,
  on,
  onCleanup,
  type JSX,
} from 'solid-js';
import type {
  Message,
  InteractionResponse,
  ChatMode,
  ModeDescriptor,
  ToolCallDisplay,
} from '@/lib/types';
import type { ChatContextValue } from '@/lib/types/context';
import type { DaemonHistoryEvent, SessionHistoryResponse } from '@/lib/types';
import {
  generateMessageId,
  turnResponseId,
  turnSegmentId,
  stripFrozenPrefix,
  estimateThinkingTokens,
} from '@/lib/turn';
import {
  fetchPendingInteractionsOnce,
  useRespondToInteraction,
} from '@/lib/query/interactions';
import { useCancelSession } from '@/lib/query/sessions';
import {
  fetchSessionHistoryOnce,
  useSendChatMessage,
  useSessionHistory,
} from '@/lib/query/history';
import { useSessionModes, useSetSessionMode } from '@/lib/query/modes';
import { consumePendingFirstMessage, peekPendingFirstMessage } from '@/lib/draft-session';
import { getBus } from '@/lib/bus';
import { statusBarStore } from '@/stores/statusBarStore';
import { notificationActions } from '@/stores/notificationStore';
import { attentionActions } from '@/stores/attentionStore';
import {
  clearTranscript,
  patchTranscript,
  queueTurn,
  recordTranscriptHydration,
  releaseTranscript,
  retainTranscript,
  retryTranscriptStream,
  setTranscriptPendingInteraction,
  setTranscriptStreaming,
  shiftQueuedTurn,
  transcriptMessages,
  transcriptOf,
  transcriptOpened,
  updateTranscriptMessages,
  type QueuedTurn,
} from './transcriptStore';
import { bootstrapSessionWithFallback } from './sessionBootstrap';
import { finalizeDanglingTool } from './chatEventReducer';
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
  // The session's LIVE transcript — messages, subagent events, streaming
  // state — keyed by session id in `transcriptStore`, shared by every pane
  // that shows this session and fed by that store's one stream subscription.
  // This pane holds it open and draws it; it does not own it.
  const transcript = () => transcriptOf(props.sessionId);
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
   * session's key and never onto the transcript now on screen — which is what
   * the abort of the in-flight history load used to prevent.
   */
  let bindAbortController: AbortController | null = null;
  /** The history document this bind last folded, so a refetch with more events can fold again. */
  let foldedHistoryFingerprint: string | null = null;
  let previousSessionId: string | null = null;

  const addMessage = (message: Message) => {
    updateTranscriptMessages(props.sessionId, (prev) => [...prev, message]);
  };

  const updateMessage = (id: string, updates: Partial<Message>) => {
    updateTranscriptMessages(props.sessionId, (prev) => {
      const index = prev.findIndex((m) => m.id === id);
      if (index === -1) return prev;
      const updated = [...prev];
      updated[index] = { ...updated[index], ...updates };
      return updated;
    });
  };

  const removeMessage = (id: string) => {
    updateTranscriptMessages(props.sessionId, (prev) => prev.filter((m) => m.id !== id));
  };

   const clearMessages = () => {
     clearTranscript(props.sessionId);
   };

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
         err instanceof Error ? err.message : 'Failed to set session mode'
       );
     });
   };

  const addSystemMessage = (content: string) => {
    addMessage({
      id: generateMessageId(),
      role: 'system',
      content,
      timestamp: Date.now(),
    });
  };

  /**
   * Folds one persisted transcript into this pane's messages.
   *
   * The document is the query's, not this pane's: `useSessionHistory` fetched
   * it, and every pane on this session reads the same one. The fold is per
   * pane, because the transcript a pane DRAWS is more than the daemon
   * persisted — the thinking block of a turn, a system notice a failed send
   * left, the optimistic entries of a turn in flight. The bind folds the
   * document once for that reason; the stream carries what follows to every
   * pane on its own.
   */
  const foldHistory = (response: SessionHistoryResponse) => {
    const loadedMessages: Message[] = [];

    // Pre-tool narration segments of the CURRENT turn, in order. A segmented
    // turn (text → tool → text) persists a `segment_complete` per boundary;
    // each becomes its own assistant bubble here, and the turn's final
    // message_complete bubble drops their concatenated prefix — exactly the
    // shape the live reducer produces, so a reload converges on it. Reset at
    // each new turn (user_message) so segments never leak across turns.
    let pendingSegments: string[] = [];

    // The turn's reasoning, accumulated the same way: `thinking` events carry
    // deltas, and the final assistant bubble carries the whole block — the
    // same place AssistantTurn renders it live. Reset at each new turn.
    let pendingThinking = '';

    // Attach a result to the newest matching tool entry.
    const findToolMessage = (callId: string): Message | undefined =>
      [...loadedMessages].reverse().find((m) => {
        const tool = m.toolCall;
        return m.role === 'tool' && tool && tool.callId === callId;
      });

    // Real event times, so a reloaded turn shows the same duration the
    // live one did. A missing stamp falls back to the old synthetic spacing.
    /**
     * One recorded event's payload.
     *
     * `data` is `unknown` on the wire: its shape differs per `event`, the
     * daemon owns that vocabulary, and the route forwards the object whole.
     * Every read below narrows through here rather than trusting a field.
     */
    const payload = (evt: DaemonHistoryEvent): Record<string, unknown> =>
      (evt.data ?? {}) as Record<string, unknown>;

    const eventTime = (evt: { timestamp?: string | null }): number | undefined => {
      const n = evt.timestamp ? Date.parse(evt.timestamp) : NaN;
      return Number.isNaN(n) ? undefined : n;
    };
    const synthetic = () => Date.now() - (response.history.length - loadedMessages.length) * 1000;
    // A turn's assistant bubbles carry the turn's START (the user message
    // time), the same stamp a live placeholder gets when the turn is sent.
    let turnStart: number | undefined;
    for (const evt of response.history) {
      const data = payload(evt);
      if (evt.event === 'user_message' && typeof data.content === 'string') {
        turnStart = eventTime(evt);
        // New turn: drop any segments a prior turn left uncollected.
        pendingSegments = [];
        pendingThinking = '';
        loadedMessages.push({
          id: (data.message_id as string) || `user-${loadedMessages.length}`,
          role: 'user',
          content: data.content,
          timestamp: turnStart ?? synthetic(),
        });
      } else if (evt.event === 'segment_complete') {
        // Canonical id derivation identical to the live reducer's, so a
        // reloaded segment bubble carries the same id it streamed under.
        const content = typeof data.content === 'string' ? data.content : '';
        const index = typeof data.index === 'number' ? data.index : Number(data.index ?? 0);
        const messageId = typeof data.message_id === 'string' ? data.message_id : undefined;
        pendingSegments.push(content);
        loadedMessages.push({
          id: messageId ? turnSegmentId(messageId, index) : `assistant-seg-${loadedMessages.length}`,
          role: 'assistant',
          content,
          timestamp: turnStart ?? synthetic(),
        });
      } else if (evt.event === 'tool_call') {
        // Reconstruct tool entries so past tool activity stays visible in
        // the transcript after a reload (they used to vanish at turn end).
        // Canonical daemon payload: {call_id, tool, args}. The entry starts
        // RUNNING and the sweep below finalizes it from its own events, the
        // way the live reducer does — a blanket 'complete' here would show a
        // cancelled tool as finished after the reload that live never showed.
        const callId = String(data.call_id ?? `hist-${loadedMessages.length}`);
        const name = String(data.tool ?? 'tool');
        const args = data.args;
        loadedMessages.push({
          id: `tool-${callId}`,
          role: 'tool',
          content: '',
          timestamp: eventTime(evt) ?? synthetic(),
          toolCall: {
            id: callId,
            callId,
            name,
            args: args === undefined ? '' : JSON.stringify(args),
            status: 'running',
            // The same daemon-computed fields the live reducer forwards, as
            // recorded on the event — the reloaded card renders identically
            // to the one that streamed.
            ...(data.display !== undefined
              ? { display: data.display as ToolCallDisplay['display'] }
              : {}),
            ...(typeof data.auto_approved === 'string' ? { autoApproved: data.auto_approved } : {}),
            ...(Array.isArray(data.diffs) ? { diffs: data.diffs as ToolCallDisplay['diffs'] } : {}),
          },
        });
      } else if (evt.event === 'tool_result' || evt.event === 'tool_result_error') {
        const callId = String(data.call_id ?? '');
        const target = findToolMessage(callId);
        if (target?.toolCall) {
          const raw = evt.event === 'tool_result_error' ? data.error : data.result;
          target.toolCall = {
            ...target.toolCall,
            status: evt.event === 'tool_result_error' ? 'error' : 'complete',
            result: raw === undefined ? target.toolCall.result
              : typeof raw === 'string' ? raw : JSON.stringify(raw),
          };
        }
      } else if (evt.event === 'precognition_complete') {
        // Metadata, not a bubble: reattach it to the user message that
        // triggered the retrieval — the same target the live reducer picks.
        // Field mapping mirrors the SSE path (which normalises in Rust):
        // the persisted payload is a PrecognitionNoteInfo, so `title`/`score`
        // become `name`/`relevance` here.
        const lastUser = [...loadedMessages].reverse().find((m) => m.role === 'user');
        if (lastUser) {
          const notes = (Array.isArray(data.notes) ? data.notes : [])
            .map((raw) => {
              const note = (raw ?? {}) as Record<string, unknown>;
              const name = note.title ?? note.name;
              return typeof name === 'string'
                ? { name, relevance: typeof note.score === 'number' ? note.score : 0 }
                : null;
            })
            .filter((n): n is { name: string; relevance: number } => n !== null);
          lastUser.precognition = {
            notesCount:
              typeof data.notes_count === 'number' ? data.notes_count : notes.length,
            notes,
          };
        }
      } else if (evt.event === 'thinking' && typeof data.content === 'string') {
        // Deltas, exactly as the live reducer accumulates them; attached to
        // the turn's final bubble at message_complete below.
        pendingThinking += data.content;
      } else if (evt.event === 'message_complete' && typeof data.full_response === 'string') {
        // The persisted full_response is the WHOLE turn; strip the prefix
        // already rendered as segment bubbles (same helper the live reducer
        // uses). Skip an empty trailing bubble when segments covered the
        // whole turn — the live reducer adds none in that case either.
        const hadSegments = pendingSegments.length > 0;
        const finalContent = stripFrozenPrefix(
          data.full_response,
          pendingSegments,
        );
        pendingSegments = [];
        // The turn's reasoning rides the answer bubble, the same place the
        // live reducer renders it; a turn whose segments covered everything
        // (no trailing bubble) pins it to the turn's last assistant bubble,
        // the same fallback the live reducer uses for token usage.
        const thinking = pendingThinking === '' ? undefined : {
          content: pendingThinking,
          isStreaming: false,
          tokenCount: estimateThinkingTokens(pendingThinking),
        };
        pendingThinking = '';
        if (finalContent !== '' || !hadSegments) {
          loadedMessages.push({
            // Same derivation the live reducer uses, so a reloaded transcript
            // carries identical ids to the one that streamed.
            id: data.message_id
              ? turnResponseId(data.message_id as string)
              : `assistant-${loadedMessages.length}`,
            role: 'assistant',
            content: finalContent,
            timestamp: turnStart ?? synthetic(),
            completedAt: eventTime(evt),
            ...(thinking ? { thinking } : {}),
          });
        } else if (thinking) {
          const lastAssistant = [...loadedMessages].reverse().find((m) => m.role === 'assistant');
          if (lastAssistant) lastAssistant.thinking = thinking;
        }
      }
    }

    // A turn can end (or the log can stop) with a tool still "running" — no
    // tool_result ever arrived. Finalize each with the same rule the live
    // reducer applies at turn end, so the reloaded transcript shows the state
    // the events describe rather than one the reload invented.
    for (const message of loadedMessages) {
      if (message.role === 'tool' && message.toolCall && message.toolCall.status === 'running') {
        message.toolCall = finalizeDanglingTool(message.toolCall);
      }
    }

    // MERGE, don't clobber: messages that arrived after the history
    // snapshot (optimistic sends, live SSE events during a slow load)
    // aren't in `loadedMessages`. Backend-canonical ids make the overlap
    // exact — anything already reconstructed is dropped from the live
    // set, everything newer is kept in order after it.
    updateTranscriptMessages(props.sessionId, (prev) => {
      const reconstructed = new Set(loadedMessages.map((m) => m.id));
      const newer = prev.filter((m) => !reconstructed.has(m.id));
      // Events stored before canonical message_ids existed reconstruct under
      // fallback ids (user-N / assistant-N), so a live-added canonical copy
      // of the same prompt escapes the exact-id overlap above and renders
      // twice. Drop a live message that an id-less reconstructed entry
      // already represents (same role + content). Only fires when a fallback
      // id is present, so current-daemon sessions (always canonical) are
      // untouched.
      const isFallbackId = (id: string) => /^(?:user|assistant)-\d+$/.test(id);
      const hasFallback = loadedMessages.some((m) => isFallbackId(m.id));
      const merged = hasFallback
        ? newer.filter((live) => !loadedMessages.some(
            (h) => isFallbackId(h.id) && h.role === live.role && h.content === live.content,
          ))
        : newer;
      return [...loadedMessages, ...merged];
    });

    // The cursor's other half: the hydration covered every event above, so
    // it records the max seq — AFTER the update finished, not before, or a
    // stream reopened mid-fold would replay events this fold was about to
    // draw anyway. Monotonic inside the seam; a fold that lands behind a
    // live-applied seq never drags the watermark back.
    const maxSeq = response.history.reduce(
      (max, evt) => (typeof evt.seq === 'number' && evt.seq > max ? evt.seq : max),
      0,
    );
    recordTranscriptHydration(props.sessionId, maxSeq > 0 ? maxSeq : undefined);
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
  createEffect(on(() => props.sessionId, (newSessionId) => {
    // Supersede the bind before it: its remaining reads must write nothing.
    if (bindAbortController) {
      bindAbortController.abort();
      bindAbortController = null;
    }
    // The transcript on screen belongs to the bind that is ending, so the new
    // one folds its own document even when it names the same session.
    foldedHistoryFingerprint = null;
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
      // the key it reads. The bind awaits the document, and the fold below
      // puts it on screen.
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
    // — loadHistory replaces the whole message list, so sending earlier
    // would let the (empty) history load wipe the optimistic message — and
    // (b) the SSE stream is open, so the response streams from token one.
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
      // POST waits for the gates below. The optimistic entries survive the
      // history load because loadHistory merges by id instead of clobbering.
      const temps = insertOptimisticTurn(pendingFirstMessage);
      const sseOpenOrTimeout = Promise.race([
        sseOpen,
        new Promise<void>((resolve) => setTimeout(resolve, 5000)),
      ]);
      void Promise.all([bootstrapPromise.catch(() => {}), sseOpenOrTimeout]).then(() => {
        const message = consumePendingFirstMessage(newSessionId);
        if (message) void dispatchTurn(message, temps);
      });
    }
  }));

  /**
   * Puts the persisted transcript on screen, once for each bind.
   *
   * Once, and not on every revision of the document, because the fold
   * REPLACES what this pane draws. The cached document is what the daemon
   * wrote down; the pane holds more than that — the thinking block of a turn,
   * the system notice a failed send left, a turn in flight. Folding a refetch
   * onto those would drop them, or move them to the end of the transcript,
   * for no gain: every pane of this session hears the same stream and folds
   * each event as it arrives.
   *
   * It runs after the bind effect above, which is what resets the flag, so a
   * rebind folds again and a document that arrives later still lands.
   */
  createEffect(() => {
    const document = history.data;
    if (!document) return;
    const fingerprint = `${document.session_id}:${document.total_events}:${document.history.length}`;
    if (foldedHistoryFingerprint === fingerprint) return;
    foldedHistoryFingerprint = fingerprint;
    foldHistory(document);
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

  // Palette "Clear Chat" / Ctrl+K. Multiple chat providers can be mounted
  // (split panes); only the one showing the active session clears — and the
  // transcript it clears is the session's, so every pane of it clears.
  getBus().on('clearChat', () => {
    if (props.sessionId && statusBarStore.activeSessionId() === props.sessionId) {
      clearMessages();
    }
  });

  // Optimistic entries go in BEFORE the POST so transcript order stays
  // user → answer even when SSE events beat the POST response, and so the
  // user sees their message + working indicator with zero delay. They carry
  // temp ids that the canonical ids replace in dispatchTurn — a temp id
  // never outlives the send, so convergence still rests on backend-canonical
  // ids only.
  const insertOptimisticTurn = (trimmed: string) => {
    patchTranscript(props.sessionId, { error: null, isLoading: true });
    setTranscriptStreaming(props.sessionId, true);
    const tempUserId = generateMessageId();
    addMessage({ id: tempUserId, role: 'user', content: trimmed, timestamp: Date.now() });
    const tempResponseId = generateMessageId();
    addMessage({ id: tempResponseId, role: 'assistant', content: '', timestamp: Date.now(), placeholder: true });
    patchTranscript(props.sessionId, { currentStreamingMessageId: tempResponseId });
    return { tempUserId, tempResponseId };
  };

  // The backend mints the canonical id: POST /api/chat/send returns the
  // turn's message_id, the SSE user_message echo carries the same id, and
  // message_complete.id is that turn id too. Keying the transcript on it
  // (user = id, assistant = `${id}-response` — see turnResponseId) means
  // every viewer converges on identical ids and dedup is exact, never
  // heuristic.
  const dispatchTurn = async (
    trimmed: string,
    { tempUserId, tempResponseId }: { tempUserId: string; tempResponseId: string },
  ) => {
    if (!props.sessionId) return;
    try {
      const messageId = await send.mutateAsync({ id: props.sessionId, message: trimmed });
      const messages = () => transcriptMessages(props.sessionId);

      // Canonicalize the user entry — unless the SSE echo already added it.
      if (messages().some((m) => m.id === messageId)) {
        removeMessage(tempUserId);
      } else {
        updateMessage(tempUserId, { id: messageId });
      }

      // Canonicalize the assistant entry. The reducer may have already
      // renamed it (segment_complete / message_complete rename the streaming
      // message to a canonical id) — then the temp id is gone and there is
      // nothing to do.
      const responseId = turnResponseId(messageId);
      if (messages().some((m) => m.id === tempResponseId)) {
        if (messages().some((m) => m.id === responseId)) {
          removeMessage(tempResponseId);
        } else {
          updateMessage(tempResponseId, { id: responseId });
        }
      }
      if (transcript().currentStreamingMessageId === tempResponseId) {
        patchTranscript(props.sessionId, { currentStreamingMessageId: responseId });
      }
    } catch (err) {
      console.error('Failed to send message:', err);
      // A refusal the daemon words "Concurrent request in progress" is a lost
      // admission race, not a lost message: a foreign client's turn (or a
      // cancel still winding down) held the one slot. Park the prompt back in
      // the queue — its entry is already on screen — and the flusher retries
      // when the stream goes idle. An error banner here would tell the user
      // their message failed while the transcript right below shows it
      // waiting its turn.
      if (err instanceof Error && /concurrent request/i.test(err.message)) {
        removeMessage(tempResponseId);
        if (transcript().currentStreamingMessageId === tempResponseId) {
          patchTranscript(props.sessionId, { currentStreamingMessageId: null });
          setTranscriptStreaming(props.sessionId, false);
        }
        queueTurn(props.sessionId, trimmed, tempUserId);
        return;
      }
      const errorMsg = err instanceof Error ? err.message : 'Failed to connect to server';
      patchTranscript(props.sessionId, { error: errorMsg });
      // Keep the user's text visible next to the failure notice, but drop the
      // empty assistant placeholder.
      removeMessage(tempResponseId);
      addMessage({
        id: generateMessageId(),
        role: 'system',
        content: `Failed to send: ${errorMsg}`,
        timestamp: Date.now(),
      });
      setTranscriptStreaming(props.sessionId, false);
      patchTranscript(props.sessionId, { isLoading: false, currentStreamingMessageId: null });
    }
  };

  const sendMessage = async (content: string) => {
    if (!content.trim() || !props.sessionId) return;
    const trimmed = content.trim();
    // A turn in flight holds the daemon's one request slot — posting now
    // would be refused as concurrent at best, and interleaved into the
    // running turn at worst. Queue instead: the optimistic entry renders at
    // the end of the streaming block immediately, and the flusher below
    // dispatches it as its own turn once the stream goes idle.
    if (transcript().isLoading || transcript().currentStreamingMessageId) {
      queueTurn(props.sessionId, trimmed);
      return;
    }
    await dispatchTurn(trimmed, insertOptimisticTurn(trimmed));
  };

  /**
   * Turns the oldest queued prompt into a running turn.
   *
   * The queue entry's optimistic message is already on screen — queuing put
   * it below the streaming block. This only opens the turn for it: the
   * assistant placeholder the daemon's answer will stream into, and the
   * dispatch that carries the POST.
   */
  const beginQueuedTurn = (entry: QueuedTurn) => {
    if (!props.sessionId) return;
    patchTranscript(props.sessionId, { error: null, isLoading: true });
    setTranscriptStreaming(props.sessionId, true);
    const tempResponseId = generateMessageId();
    addMessage({
      id: tempResponseId,
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      placeholder: true,
    });
    patchTranscript(props.sessionId, { currentStreamingMessageId: tempResponseId });
    updateMessage(entry.tempId, { queued: false });
    void dispatchTurn(entry.content, { tempUserId: entry.tempId, tempResponseId });
  };

  // The queue's drain pump. Every idle moment with a non-empty queue starts
  // exactly one queued turn: `shiftQueuedTurn` is the claim, so two panes of
  // one session — which both run this effect over the shared transcript —
  // cannot dispatch the same prompt twice, and the daemon's one-slot
  // admission never sees two concurrent POSTs from us.
  createEffect(() => {
    if (!props.sessionId) return;
    const t = transcript();
    if (t.isLoading || t.currentStreamingMessageId || t.queuedTurns.length === 0) return;
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
        // The daemon is unreachable, so no `ended` event will arrive to
        // close the turn — drop the streaming flags here or the spinner
        // spins forever. Any other path leaves the closing to the reducer's
        // `ended` case, which every subscribed pane receives, including
        // foreign cancellations this client never issued.
        setTranscriptStreaming(props.sessionId, false);
        patchTranscript(props.sessionId, { isLoading: false, currentStreamingMessageId: null });
      }
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
    messages: () => transcript().messages,
    isLoading: () => transcript().isLoading,
    isStreaming: () => transcript().isStreaming,
    pendingInteraction: () => transcript().pendingInteraction,
    error: () => transcript().error,
    connectionStatus: () => transcript().connectionStatus,
    retryConnection,
    subagentEvents: () => transcript().subagentEvents,
    chatMode: () => transcript().chatMode,
    availableModes,
    isLoadingHistory,
    setChatMode: (mode: ChatMode) => patchTranscript(props.sessionId, { chatMode: mode }),
    switchMode,
    sendMessage,
    respondToInteraction,
    clearMessages,
    cancelStream,
    addSystemMessage,
  };

  return (
    <ChatContext.Provider value={value}>
      {props.children}
    </ChatContext.Provider>
  );
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
  respondToInteraction: noopAsync,
  clearMessages: () => {},
  cancelStream: noopAsync,
  addSystemMessage: () => {},
};

export function useChatSafe(): ChatContextValue {
  const context = useContext(ChatContext);
  return context ?? fallbackChatContext;
}
