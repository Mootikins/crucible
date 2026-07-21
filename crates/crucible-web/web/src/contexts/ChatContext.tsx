import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  createEffect,
  onCleanup,
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
} from '@/lib/types';
import type { ChatContextValue } from '@/lib/types/context';
import {
  setSessionMode,
  sendChatMessage,
  subscribeToEvents,
  respondToInteraction as apiRespondToInteraction,
  cancelSession as apiCancelSession,
  generateMessageId,
  getSessionHistory,
  turnResponseId,
  turnSegmentId,
  stripFrozenPrefix,
} from '@/lib/api';
import { findTabBySessionId } from '@/lib/session-actions';
import { consumePendingFirstMessage } from '@/lib/draft-session';
import { windowActions } from '@/stores/windowStore';
import { statusBarStore } from '@/stores/statusBarStore';
import { notificationActions } from '@/stores/notificationStore';
import { attentionActions } from '@/stores/attentionStore';
import { createChatEventReducer } from './chatEventReducer';
import { bootstrapSessionWithFallback } from './sessionBootstrap';


interface ChatProviderProps {
  sessionId: string;
  children: any;
}

const ChatContext = createContext<ChatContextValue>();

export const ChatProvider: ParentComponent<ChatProviderProps> = (props) => {
  const [messages, setMessages] = createStore<Message[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);
  const [isStreaming, setIsStreamingRaw] = createSignal(false);
  const [pendingInteraction, setPendingInteractionRaw] = createSignal<InteractionRequest | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [subagentEvents, setSubagentEvents] = createStore<SubagentEvent[]>([]);
  const [contextUsage, setContextUsage] = createSignal<ContextUsage | null>(null);
  const [chatMode, setChatMode] = createSignal<ChatMode>('normal');
  const [isLoadingHistory, setIsLoadingHistory] = createSignal(false);
  
  // Mirror interaction/streaming state into the global attention store so
  // the Inbox and header badge see every session with an open tab, not just
  // the focused one. Entries are cleared when this provider unmounts.
  const setIsStreaming = (value: boolean) => {
    setIsStreamingRaw(value);
    if (props.sessionId) {
      attentionActions.report(props.sessionId, { isStreaming: value, title: sessionTitle() });
    }
  };
  const setPendingInteraction = (request: InteractionRequest | null) => {
    setPendingInteractionRaw(request);
    if (props.sessionId) {
      attentionActions.report(props.sessionId, {
        pendingInteraction: request,
        title: sessionTitle(),
      });
    }
  };

  let eventSourceCleanup: (() => void) | null = null;
  let historyAbortController: AbortController | null = null;
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
     void setSessionMode(props.sessionId, mode).catch((err) => {
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
    messages: () => messages,
    currentStreamingMessageId: () => currentStreamingMessageId,
    setCurrentStreamingMessageId: (id) => {
      currentStreamingMessageId = id;
    },
    onTitleChanged: (title: string) => {
      setSessionTitle(title);
      const tabInfo = findTabBySessionId(props.sessionId);
      if (tabInfo) {
        windowActions.updateTab(tabInfo.groupId, tabInfo.tab.id, { title });
      }
      attentionActions.report(props.sessionId, { title });
      // Let the session list (Home resume, Inbox) pick up the new name.
      window.dispatchEvent(
        new CustomEvent('crucible:session-title-changed', {
          detail: { sessionId: props.sessionId, title },
        })
      );
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
    setIsLoading,
    setIsStreaming,
  });

  const loadHistory = async (sessionId: string, kiln: string, signal?: AbortSignal) => {
    setIsLoadingHistory(true);
    try {
      // Explicit high limit: the server pages from the FRONT, and a long
      // agentic turn can log hundreds of thinking events — the default page
      // cut off the tail, which holds the tool results and message_complete
      // (i.e. the assistant's actual text).
      const response = await getSessionHistory(sessionId, kiln, 10000, undefined, signal);
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

      for (const evt of response.history) {
        if (evt.event === 'user_message' && evt.data?.content) {
          // New turn: drop any segments a prior turn left uncollected.
          pendingSegments = [];
          loadedMessages.push({
            id: evt.data.message_id as string || `user-${loadedMessages.length}`,
            role: 'user',
            content: evt.data.content,
            timestamp: Date.now() - (response.history.length - loadedMessages.length) * 1000,
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
            timestamp: Date.now() - (response.history.length - loadedMessages.length) * 1000,
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
              timestamp: Date.now() - (response.history.length - loadedMessages.length) * 1000,
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
    } catch (err) {
      if (err instanceof Error && err.name === 'AbortError') {
        // Silently ignore — expected when session switches rapidly
        return;
      }
      console.error('Failed to load session history:', err);
    } finally {
      setIsLoadingHistory(false);
    }
  };

  createEffect(() => {
    const newSessionId = props.sessionId;
    
    if (eventSourceCleanup) {
      eventSourceCleanup();
      eventSourceCleanup = null;
    }
    
    // Abort any in-flight history load from a previous session
    if (historyAbortController) {
      historyAbortController.abort();
      historyAbortController = null;
    }
    
    if (newSessionId !== previousSessionId && previousSessionId !== null) {
      clearMessages();
      attentionActions.clear(previousSessionId);
    }
    previousSessionId = newSessionId;
    
    if (!newSessionId) {
      return;
    }

    const abortController = new AbortController();
    historyAbortController = abortController;

    const bootstrapPromise = bootstrapSessionWithFallback({
      sessionId: newSessionId,
      signal: abortController.signal,
      setSessionTitle,
      setChatMode,
      loadHistory,
    });

    // Resolves when the SSE stream is open (daemon subscribed). Sending
    // before that drops the response's first tokens — the turn then looks
    // frozen until message_complete backfills the full text.
    let resolveSseOpen: () => void = () => {};
    const sseOpen = new Promise<void>((resolve) => {
      resolveSseOpen = resolve;
    });
    eventSourceCleanup = subscribeToEvents(newSessionId, handleEvent, resolveSseOpen);

    // Lazy creation handoff: the draft surface staged the user's first
    // message before opening this session. Send it only after (a) bootstrap
    // — loadHistory replaces the whole message list, so sending earlier
    // would let the (empty) history load wipe the optimistic message — and
    // (b) the SSE stream is open, so the response streams from token one.
    // The timeout keeps the message from being stuck if SSE can't connect.
    const pendingFirstMessage = consumePendingFirstMessage(newSessionId);
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
        void dispatchTurn(pendingFirstMessage, temps);
      });
    }
  });

  onCleanup(() => {
    if (eventSourceCleanup) {
      eventSourceCleanup();
      eventSourceCleanup = null;
    }
    if (historyAbortController) {
      historyAbortController.abort();
      historyAbortController = null;
    }
    if (props.sessionId) {
      attentionActions.clear(props.sessionId);
    }
  });

  // The Inbox answers interactions on this session's behalf (raw API call);
  // it broadcasts so the owning provider drops its in-chat prompt too.
  const onInteractionResolved = (e: Event) => {
    const { sessionId, requestId } = (e as CustomEvent<{ sessionId: string; requestId: string }>)
      .detail;
    if (sessionId === props.sessionId && pendingInteraction()?.id === requestId) {
      setPendingInteraction(null);
    }
  };
  window.addEventListener('crucible:interaction-resolved', onInteractionResolved);
  onCleanup(() =>
    window.removeEventListener('crucible:interaction-resolved', onInteractionResolved)
  );

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
    addMessage({ id: tempResponseId, role: 'assistant', content: '', timestamp: Date.now() });
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
      const messageId = await sendChatMessage(props.sessionId, trimmed);

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
      await apiRespondToInteraction(props.sessionId, request.id, response);
    } catch (err) {
      console.error('Failed to send interaction response:', err);
      setError(err instanceof Error ? err.message : 'Failed to respond');
    }
  };

  const cancelStream = async () => {
    if (props.sessionId) {
      try {
        await apiCancelSession(props.sessionId);
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

  const value: ChatContextValue = {
    sessionId: () => props.sessionId,
    messages: () => messages,
    isLoading,
    isStreaming,
    pendingInteraction,
    error,
    subagentEvents: () => subagentEvents,
    contextUsage,
    chatMode,
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
  subagentEvents: () => [],
  contextUsage: () => null,
  chatMode: () => 'normal',
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
