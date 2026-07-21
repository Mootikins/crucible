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
  let firstUserMessage: string | null = null;
  let hasReceivedFirstResponse = false;
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
  // ToolCard's expanded state survives streaming result deltas.
  const updateToolMessage = (
    callId: string,
    updater: (tool: ToolCallDisplay) => ToolCallDisplay,
  ) => {
    setMessages(
      (m) => m.role === 'tool' && !!m.toolCall
        && (m.toolCall.callId === callId || m.toolCall.id === callId),
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
     firstUserMessage = null;
     hasReceivedFirstResponse = false;
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
    firstUserMessage: () => firstUserMessage,
    hasReceivedFirstResponse: () => hasReceivedFirstResponse,
    setHasReceivedFirstResponse: (value) => {
      hasReceivedFirstResponse = value;
    },
    // Titles are daemon-owned: on the first completed turn of an untitled
    // session the daemon generates a topic-based title and broadcasts
    // title_changed, handled below. Nothing to do client-side.
    onFirstResponse: () => {},
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
      const response = await getSessionHistory(sessionId, kiln, undefined, undefined, signal);
      const loadedMessages: Message[] = [];

      // Attach a result to the newest matching tool entry.
      const findToolMessage = (callId: string): Message | undefined =>
        [...loadedMessages].reverse().find((m) => {
          const tool = m.toolCall;
          return m.role === 'tool' && tool && tool.callId === callId;
        });

      for (const evt of response.history) {
        if (evt.event === 'user_message' && evt.data?.content) {
          loadedMessages.push({
            id: evt.data.message_id as string || `user-${loadedMessages.length}`,
            role: 'user',
            content: evt.data.content,
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
          loadedMessages.push({
            id: evt.data.message_id as string || `assistant-${loadedMessages.length}`,
            role: 'assistant',
            content: evt.data.full_response,
            timestamp: Date.now() - (response.history.length - loadedMessages.length) * 1000,
          });
        }
      }
      
      setMessages(loadedMessages);
      if (loadedMessages.length > 0) {
        hasReceivedFirstResponse = true;
        const userMsg = loadedMessages.find((m) => m.role === 'user');
        if (userMsg) {
          firstUserMessage = userMsg.content;
        }
      }
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
      const sseOpenOrTimeout = Promise.race([
        sseOpen,
        new Promise<void>((resolve) => setTimeout(resolve, 5000)),
      ]);
      void Promise.all([bootstrapPromise.catch(() => {}), sseOpenOrTimeout]).then(() => {
        void sendMessage(pendingFirstMessage);
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

  const sendMessage = async (content: string) => {
    if (!content.trim() || isLoading() || !props.sessionId) return;

    setError(null);

    const userMessage: Message = {
      id: generateMessageId(),
      role: 'user',
      content: content.trim(),
      timestamp: Date.now(),
    };
    addMessage(userMessage);
    
    if (!firstUserMessage) {
      firstUserMessage = content.trim();
    }

    const assistantMessageId = generateMessageId();
    const assistantMessage: Message = {
      id: assistantMessageId,
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
    };
    addMessage(assistantMessage);
    currentStreamingMessageId = assistantMessageId;

    setIsLoading(true);
    setIsStreaming(true);

    try {
      await sendChatMessage(props.sessionId, content);
    } catch (err) {
      console.error('Failed to send message:', err);
      const errorMsg = err instanceof Error ? err.message : 'Failed to connect to server';
      setError(errorMsg);
      updateMessage(assistantMessageId, {
        content: `Error: ${errorMsg}`,
      });
      setIsStreaming(false);
      setIsLoading(false);
      currentStreamingMessageId = null;
    }
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
