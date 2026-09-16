import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  createEffect,
  on,
  onCleanup,
  untrack,
} from 'solid-js';
import { createStore } from 'solid-js/store';
import type {
  Message,
  InteractionRequest,
  InteractionResponse,
  ToolCallDisplay,
  SubagentEvent,
  ContextUsage,
  ChatMode,
  ModeDescriptor,
  ConnectionStatus,
} from '@/lib/types';
import type { ChatContextValue } from '@/lib/types/context';
import type { SessionHistoryResponse } from '@/lib/api';
import {
  generateMessageId,
  turnResponseId,
  turnSegmentId,
  stripFrozenPrefix,
} from '@/lib/api';
import {
  fetchPendingInteractionsOnce,
  useRespondToInteraction,
} from '@/lib/query/interactions';
import { sessionEvents } from '@/lib/query/sse';
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
import { tabHost } from '@/lib/tab-host';
import { createChatEventReducer } from './chatEventReducer';
import { bootstrapSessionWithFallback } from './sessionBootstrap';
import { FALLBACK_MODES } from '@/components/ChatModeControl';


interface ChatProviderProps {
  sessionId: string;
  children: any;
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
  const [messages, setMessages] = createStore<Message[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);
  const [isStreaming, setIsStreamingRaw] = createSignal(false);
  const [pendingInteraction, setPendingInteractionRaw] = createSignal<InteractionRequest | null>(null);
  // Answering a request takes it out of the shared pending list, so the read
  // below cannot hand back one this client already answered. That is what
  // retires the set of answered request ids this provider used to keep.
  const respond = useRespondToInteraction();
  const [error, setError] = createSignal<string | null>(null);
  // Transport health, held apart from `error`: the error line also carries
  // daemon failures ("Failed to send: …"), and a "retry the connection"
  // control under one of THOSE would offer a cure for the wrong illness.
  const [connectionStatus, setConnectionStatus] = createSignal<ConnectionStatus>('connected');
  const [subagentEvents, setSubagentEvents] = createStore<SubagentEvent[]>([]);
  const [contextUsage, setContextUsage] = createSignal<ContextUsage | null>(null);
  const [chatMode, setChatMode] = createSignal<ChatMode>('ask');
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
    if (listed && listed.modes.length > 0) setChatMode(listed.current_mode_id);
  });
  /**
   * This pane has nothing to draw and is waiting for the transcript.
   *
   * The query's own state. A bind onto a session this browser read already
   * paints from the cache instead of showing the skeleton a second time, and a
   * refetch of a document this pane has folded is not a load.
   */
  const isLoadingHistory = () => history.isLoading;
  
  // Mirror interaction/streaming state into the global attention store so
  // the Inbox and header badge see every session with an open tab, not just
  // the focused one. Entries are cleared when this provider unmounts.
  const setIsStreaming = (value: boolean) => {
    setIsStreamingRaw(value);
    if (props.sessionId) {
      attentionActions.report(props.sessionId, {
        isStreaming: value,
        title: untrack(sessionTitle),
      });
    }
  };
  const setPendingInteraction = (request: InteractionRequest | null) => {
    setPendingInteractionRaw(request);
    if (props.sessionId) {
      attentionActions.report(props.sessionId, {
        pendingInteraction: request,
        title: untrack(sessionTitle),
      });
    }
  };

  /** Removes this pane from the shared stream. The stream itself belongs to
   *  `sessionEvents(id)`, which closes the source when the last pane leaves. */
  let streamUnsubscribe: (() => void) | null = null;
  /** The session the live stream belongs to. `retryConnection` needs it, and
   *  `props.sessionId` is not it: the stream is opened by an effect that also
   *  handles the null case, so the two can disagree for a tick. */
  let streamSessionId: string | null = null;
  /**
   * The mark that this bind is superseded, which a late answer checks.
   *
   * It aborts no request any more. The transcript is a query keyed by session
   * id, so an answer for the session this pane has left writes to that
   * session's key and never onto the transcript now on screen — which is what
   * the abort of the in-flight history load used to prevent.
   */
  let bindAbortController: AbortController | null = null;
  /** True once this bind folded the persisted transcript it binds to. */
  let boundHistoryFolded = false;
  let currentStreamingMessageId: string | null = null;
  let previousSessionId: string | null = null;
  const [sessionTitle, setSessionTitle] = createSignal<string | null>(null);

  const addMessage = (message: Message) => {
    setMessages((prev) => [...prev, message]);
  };

  const updateMessage = (id: string, updates: Partial<Message>) => {
    setMessages((prev) => {
      const index = prev.findIndex((m) => m.id === id);
      if (index === -1) return prev;
      const updated = [...prev];
      updated[index] = { ...updated[index], ...updates };
      return updated;
    });
  };

  const removeMessage = (id: string) => {
    setMessages((prev) => prev.filter((m) => m.id !== id));
  };

  const appendToMessage = (id: string, content: string) => {
    setMessages((prev) => {
      const index = prev.findIndex((m) => m.id === id);
      if (index === -1) return prev;
      const updated = [...prev];
      updated[index] = { ...updated[index], content: updated[index].content + content };
      return updated;
    });
  };

  /**
   * Tool calls are transcript entries. A turn's tools run BEFORE its answer
   * text, but sendMessage pre-creates the (empty) assistant message — so a
   * new tool entry is inserted before that placeholder while it is still
   * empty, keeping transcript order chronological: user → tools → answer.
   */
  const addToolMessage = (tool: ToolCallDisplay) => {
    const toolMessage: Message = {
      id: `tool-${tool.callId ?? tool.id}`,
      role: 'tool',
      content: '',
      timestamp: Date.now(),
      toolCall: tool,
    };
    setMessages((prev) => {
      const streamingId = currentStreamingMessageId;
      const index = streamingId ? prev.findIndex((m) => m.id === streamingId) : -1;
      if (index !== -1 && prev[index].content === '') {
        const next = [...prev];
        next.splice(index, 0, toolMessage);
        return next;
      }
      return [...prev, toolMessage];
    });
  };

  // Fine-grained path update: mutating `toolCall` in place keeps the message
  // object's identity stable, so <For> doesn't recreate the row and the
  // ToolCard's expanded state survives streaming result deltas. Every
  // producer sets callId, so that's the only match key.
  const updateToolMessage = (
    callId: string,
    updater: (tool: ToolCallDisplay) => ToolCallDisplay,
  ) => {
    setMessages(
      (m) => m.role === 'tool' && m.toolCall?.callId === callId,
      'toolCall',
      (tool) => updater(tool as ToolCallDisplay),
    );
  };

   const clearMessages = () => {
     setMessages([]);
     setSubagentEvents([]);
     setContextUsage(null);
     setError(null);
     setPendingInteraction(null);
     currentStreamingMessageId = null;
   };

   /** UI-optimistic mode switch that also persists daemon-side. The daemon
    * echoes a mode_changed SSE event; on failure the UI reverts and surfaces
    * the error (plan mode that isn't enforced server-side must not look on). */
   const switchMode = (mode: ChatMode) => {
     const previous = chatMode();
     setChatMode(mode);
     // The mutation moves the cached `current_mode_id` and puts it back on a
     // refusal, which is what the other readers of the list see. This pane
     // holds its own chip, because the chip must answer the click whether or
     // not the daemon has answered the list at all. The mutation also asks
     // for the list again once it settles, so a mode the daemon rejects stops
     // being offered.
     void setMode.mutateAsync({ id: props.sessionId, mode }).catch((err) => {
       setChatMode(previous);
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

  const handleEvent = createChatEventReducer({
    // No `onUnknownMode`: the stream's own route invalidates this session's
    // mode list on EVERY `mode_changed`, so the list is read again whether or
    // not this pane knows the mode the event names.
    messages: () => messages,
    currentStreamingMessageId: () => currentStreamingMessageId,
    setCurrentStreamingMessageId: (id) => {
      currentStreamingMessageId = id;
    },
    onTitleChanged: (title: string) => {
      setSessionTitle(title);
      const host = tabHost();
      const tab = host.find((t) => t.metadata?.sessionId === props.sessionId);
      if (tab) host.update(tab.id, { title });
      attentionActions.report(props.sessionId, { title });
      // The session list (Home resume, Inbox) learns the new name from the
      // stream's own route, which emits `sessionTitleChanged` on the bus once
      // per event. Announcing it here as well would announce it once per open
      // pane, and the panes of one session all carry the same title.
    },
    addMessage,
    updateMessage,
    appendToMessage,
    addToolMessage,
    updateToolMessage,
    setSubagentEvents,
    setContextUsage,
    setChatMode,
    setPendingInteraction,
    setError,
    setConnectionStatus,
    setIsLoading,
    setIsStreaming,
  });

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

    // Attach a result to the newest matching tool entry.
    const findToolMessage = (callId: string): Message | undefined =>
      [...loadedMessages].reverse().find((m) => {
        const tool = m.toolCall;
        return m.role === 'tool' && tool && tool.callId === callId;
      });

    // Real event times, so a reloaded turn shows the same duration the
    // live one did. A missing stamp falls back to the old synthetic spacing.
    const eventTime = (evt: { timestamp?: string }): number | undefined => {
      const n = evt.timestamp ? Date.parse(evt.timestamp) : NaN;
      return Number.isNaN(n) ? undefined : n;
    };
    const synthetic = () => Date.now() - (response.history.length - loadedMessages.length) * 1000;
    // A turn's assistant bubbles carry the turn's START (the user message
    // time), the same stamp a live placeholder gets when the turn is sent.
    let turnStart: number | undefined;
    for (const evt of response.history) {
      if (evt.event === 'user_message' && evt.data?.content) {
        turnStart = eventTime(evt);
        // New turn: drop any segments a prior turn left uncollected.
        pendingSegments = [];
        loadedMessages.push({
          id: evt.data.message_id as string || `user-${loadedMessages.length}`,
          role: 'user',
          content: evt.data.content,
          timestamp: turnStart ?? synthetic(),
        });
      } else if (evt.event === 'segment_complete') {
        // Canonical id derivation identical to the live reducer's, so a
        // reloaded segment bubble carries the same id it streamed under.
        const data = (evt.data ?? {}) as Record<string, unknown>;
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
        // Canonical daemon payload: {call_id, tool, args}.
        const data = (evt.data ?? {}) as Record<string, unknown>;
        const callId = String(data.call_id ?? `hist-${loadedMessages.length}`);
        const name = String(data.tool ?? 'tool');
        const args = data.args;
        loadedMessages.push({
          id: `tool-${callId}`,
          role: 'tool',
          content: '',
          timestamp: Date.now() - (response.history.length - loadedMessages.length) * 1000,
          toolCall: {
            id: callId,
            callId,
            name,
            args: args === undefined ? '' : JSON.stringify(args),
            status: 'complete',
          },
        });
      } else if (evt.event === 'tool_result' || evt.event === 'tool_result_error') {
        const data = (evt.data ?? {}) as Record<string, unknown>;
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
        const data = (evt.data ?? {}) as Record<string, unknown>;
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
      } else if (evt.event === 'message_complete' && evt.data?.full_response) {
        // The persisted full_response is the WHOLE turn; strip the prefix
        // already rendered as segment bubbles (same helper the live reducer
        // uses). Skip an empty trailing bubble when segments covered the
        // whole turn — the live reducer adds none in that case either.
        const hadSegments = pendingSegments.length > 0;
        const finalContent = stripFrozenPrefix(
          evt.data.full_response as string,
          pendingSegments,
        );
        pendingSegments = [];
        if (finalContent !== '' || !hadSegments) {
          loadedMessages.push({
            // Same derivation the live reducer uses, so a reloaded transcript
            // carries identical ids to the one that streamed.
            id: evt.data.message_id
              ? turnResponseId(evt.data.message_id as string)
              : `assistant-${loadedMessages.length}`,
            role: 'assistant',
            content: finalContent,
            timestamp: turnStart ?? synthetic(),
            completedAt: eventTime(evt),
          });
        }
      }
    }
    
    // MERGE, don't clobber: messages that arrived after the history
    // snapshot (optimistic sends, live SSE events during a slow load)
    // aren't in `loadedMessages`. Backend-canonical ids make the overlap
    // exact — anything already reconstructed is dropped from the live
    // set, everything newer is kept in order after it.
    setMessages((prev) => {
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
  };

  /**
   * Binds this pane to one session: the stream, the bootstrap, the history and
   * the staged first message.
   *
   * `on` names the ONE dependency, and it is not a tidiness: the body reports
   * to the attention store, which reads the title, and the bootstrap writes
   * that title. A plain effect therefore tracked a signal its own bootstrap
   * changed, re-ran for the same session, and staged the draft's first message
   * a second time — the user saw their own turn twice. The reads inside are
   * deliberately untracked; only a new session id may bind again.
   */
  createEffect(on(() => props.sessionId, (newSessionId) => {
    if (streamUnsubscribe) {
      streamUnsubscribe();
      streamUnsubscribe = null;
      streamSessionId = null;
    }
    
    // Supersede the bind before it: its remaining reads must write nothing.
    if (bindAbortController) {
      bindAbortController.abort();
      bindAbortController = null;
    }
    // The transcript on screen belongs to the bind that is ending, so the new
    // one folds its own document even when it names the same session.
    boundHistoryFolded = false;
    
    if (newSessionId !== previousSessionId && previousSessionId !== null) {
      clearMessages();
      attentionActions.clear(previousSessionId);
    }
    previousSessionId = newSessionId;
    
    if (!newSessionId) {
      return;
    }

    const abortController = new AbortController();
    bindAbortController = abortController;

    const bootstrapPromise = bootstrapSessionWithFallback({
      sessionId: newSessionId,
      signal: abortController.signal,
      setSessionTitle,
      setChatMode,
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
        if (held && !pendingInteraction()) {
          setPendingInteraction(held.request);
        }
      })
      .catch(() => {
        /* The aggregate is a courtesy; the stream still delivers new requests. */
      });

    // Resolves when the SSE stream is open (daemon subscribed). Sending
    // before that drops the response's first tokens — the turn then looks
    // frozen until message_complete backfills the full text.
    let resolveSseOpen: () => void = () => {};
    const sseOpen = new Promise<void>((resolve) => {
      resolveSseOpen = resolve;
    });
    streamSessionId = newSessionId;
    // The shared root, not a source of our own: a second pane on this session
    // joins the stream this one opened, and a pane that joins an open stream
    // gets `resolveSseOpen` at once rather than waiting for an open it missed.
    streamUnsubscribe = sessionEvents(newSessionId).subscribe(handleEvent, resolveSseOpen);

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
    if (!document || boundHistoryFolded) return;
    boundHistoryFolded = true;
    foldHistory(document);
  });

  onCleanup(() => {
    if (streamUnsubscribe) {
      streamUnsubscribe();
      streamUnsubscribe = null;
      streamSessionId = null;
    }
    if (bindAbortController) {
      bindAbortController.abort();
      bindAbortController = null;
    }
    if (props.sessionId) {
      attentionActions.clear(props.sessionId);
    }
  });

  // Whoever answered a request — this pane, another pane, or the inbox on
  // this session's behalf — the write announces it, and the pane holding the
  // card drops it. `on` removes the handler with this owner.
  getBus().on('interactionResolved', ({ sessionId, requestId }) => {
    if (sessionId === props.sessionId && pendingInteraction()?.id === requestId) {
      setPendingInteraction(null);
    }
  });

  // Palette "Clear Chat" / Ctrl+K. Multiple chat providers can be mounted
  // (split panes); only the one showing the active session clears its view.
  const onClearChatEvent = () => {
    if (props.sessionId && statusBarStore.activeSessionId() === props.sessionId) {
      clearMessages();
    }
  };
  window.addEventListener('crucible:clear-chat', onClearChatEvent);
  onCleanup(() => window.removeEventListener('crucible:clear-chat', onClearChatEvent));

  // Optimistic entries go in BEFORE the POST so transcript order stays
  // user → answer even when SSE events beat the POST response, and so the
  // user sees their message + working indicator with zero delay. They carry
  // temp ids that the canonical ids replace in dispatchTurn — a temp id
  // never outlives the send, so convergence still rests on backend-canonical
  // ids only.
  const insertOptimisticTurn = (trimmed: string) => {
    setError(null);
    setIsLoading(true);
    setIsStreaming(true);
    const tempUserId = generateMessageId();
    addMessage({ id: tempUserId, role: 'user', content: trimmed, timestamp: Date.now() });
    const tempResponseId = generateMessageId();
    addMessage({ id: tempResponseId, role: 'assistant', content: '', timestamp: Date.now(), placeholder: true });
    currentStreamingMessageId = tempResponseId;
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

      // Canonicalize the user entry — unless the SSE echo already added it.
      if (messages.some((m) => m.id === messageId)) {
        removeMessage(tempUserId);
      } else {
        updateMessage(tempUserId, { id: messageId });
      }

      // Canonicalize the assistant entry. The reducer may have already
      // renamed it (segment_complete / message_complete rename the streaming
      // message to a canonical id) — then the temp id is gone and there is
      // nothing to do.
      const responseId = turnResponseId(messageId);
      if (messages.some((m) => m.id === tempResponseId)) {
        if (messages.some((m) => m.id === responseId)) {
          removeMessage(tempResponseId);
        } else {
          updateMessage(tempResponseId, { id: responseId });
        }
      }
      if (currentStreamingMessageId === tempResponseId) {
        currentStreamingMessageId = responseId;
      }
    } catch (err) {
      console.error('Failed to send message:', err);
      const errorMsg = err instanceof Error ? err.message : 'Failed to connect to server';
      setError(errorMsg);
      // Keep the user's text visible next to the failure notice, but drop the
      // empty assistant placeholder.
      removeMessage(tempResponseId);
      addMessage({
        id: generateMessageId(),
        role: 'system',
        content: `Failed to send: ${errorMsg}`,
        timestamp: Date.now(),
      });
      setIsStreaming(false);
      setIsLoading(false);
      currentStreamingMessageId = null;
    }
  };

  const sendMessage = async (content: string) => {
    if (!content.trim() || isLoading() || !props.sessionId) return;
    const trimmed = content.trim();
    await dispatchTurn(trimmed, insertOptimisticTurn(trimmed));
  };

   const respondToInteraction = async (response: InteractionResponse) => {
    const request = pendingInteraction();
    if (!request || !props.sessionId) return;

    setPendingInteraction(null);

    try {
      await respond.mutateAsync({
        sessionId: props.sessionId,
        requestId: request.id,
        response,
      });
    } catch (err) {
      console.error('Failed to send interaction response:', err);
      setError(err instanceof Error ? err.message : 'Failed to respond');
    }
  };

  const cancelStream = async () => {
    if (props.sessionId) {
      try {
        await cancel.mutateAsync(props.sessionId);
      } catch (err) {
        console.error('Failed to cancel session:', err);
      }
    }
    
    if (currentStreamingMessageId) {
      updateMessage(currentStreamingMessageId, {
        content: messages.find((m) => m.id === currentStreamingMessageId)?.content + ' [cancelled]',
      });
    }
    setIsStreaming(false);
    setIsLoading(false);
    currentStreamingMessageId = null;
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
    const id = streamSessionId;
    if (!id) return;
    sessionEvents(id).reconnect();
  };

  const value: ChatContextValue = {
    sessionId: () => props.sessionId,
    messages: () => messages,
    isLoading,
    isStreaming,
    pendingInteraction,
    error,
    connectionStatus,
    retryConnection,
    subagentEvents: () => subagentEvents,
    contextUsage,
    chatMode,
    availableModes,
    isLoadingHistory,
    setChatMode,
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
  contextUsage: () => null,
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
