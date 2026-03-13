import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  createEffect,
  onCleanup,
  Accessor,
} from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { statusBarActions } from '@/stores/statusBarStore';
import type {
  Message,
  ChatEvent,
  InteractionRequest,
  InteractionResponse,
  ToolCallDisplay,
  SubagentEvent,
  ContextUsage,
  ChatMode,
} from '@/lib/types';
import {
  sendChatMessage,
  subscribeToEvents,
  respondToInteraction as apiRespondToInteraction,
  cancelSession as apiCancelSession,
  generateMessageId,
  getSession,
  getSessionHistory,
  getConfig,
  listSessions,
  setSessionTitle as apiSetSessionTitle,
  generateSessionTitle,
} from '@/lib/api';
import { findTabBySessionId } from '@/lib/session-actions';
import { windowActions } from '@/stores/windowStore';

export interface ChatContextValue {
  messages: Accessor<Message[]>;
  isLoading: Accessor<boolean>;
  isStreaming: Accessor<boolean>;
  pendingInteraction: Accessor<InteractionRequest | null>;
  error: Accessor<string | null>;
  activeTools: Accessor<ToolCallDisplay[]>;
  subagentEvents: Accessor<SubagentEvent[]>;
  contextUsage: Accessor<ContextUsage | null>;
  chatMode: Accessor<ChatMode>;
  isLoadingHistory: Accessor<boolean>;
  setChatMode: (mode: ChatMode) => void;
  sendMessage: (content: string) => Promise<void>;
  respondToInteraction: (response: InteractionResponse) => Promise<void>;
  clearMessages: () => void;
  cancelStream: () => Promise<void>;
  addSystemMessage: (content: string) => void;
}

interface ChatProviderProps {
  sessionId: string;
  children: any;
}

const ChatContext = createContext<ChatContextValue>();

export const ChatProvider: ParentComponent<ChatProviderProps> = (props) => {
  const [messages, setMessages] = createStore<Message[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);
  const [isStreaming, setIsStreaming] = createSignal(false);
  const [pendingInteraction, setPendingInteraction] = createSignal<InteractionRequest | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [activeTools, setActiveTools] = createStore<ToolCallDisplay[]>([]);
  const [subagentEvents, setSubagentEvents] = createStore<SubagentEvent[]>([]);
  const [contextUsage, setContextUsage] = createSignal<ContextUsage | null>(null);
  const [chatMode, setChatMode] = createSignal<ChatMode>('normal');
  const [isLoadingHistory, setIsLoadingHistory] = createSignal(false);
  
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

   const clearMessages = () => {
     setMessages([]);
     setActiveTools([]);
     setSubagentEvents([]);
     setContextUsage(null);
     setError(null);
     setPendingInteraction(null);
     currentStreamingMessageId = null;
     firstUserMessage = null;
     hasReceivedFirstResponse = false;
   };

   const addSystemMessage = (content: string) => {
     addMessage({
       id: generateMessageId(),
       role: 'system',
       content,
       timestamp: Date.now(),
     });
   };

   const handleEvent = (event: ChatEvent) => {
     switch (event.type) {
       case 'token':
         if (currentStreamingMessageId) {
           appendToMessage(currentStreamingMessageId, event.content);
         }
         break;

       case 'tool_call':
       case 'tool_call_start': {
         const toolName = 'title' in event ? event.title : ('name' in event ? event.name : '');
         const toolArgs = 'arguments' in event ? JSON.stringify(event.arguments ?? '') : '';
         const tool: ToolCallDisplay = {
           id: event.id,
           name: toolName,
           args: toolArgs,
           status: 'running',
           callId: event.id,
         };
         setActiveTools((prev) => [...prev, tool]);
         break;
       }

       case 'tool_result':
         setActiveTools(produce((tools) => {
           const tool = tools.find((t) => t.callId === event.id || t.id === event.id);
           if (tool) {
             tool.result = event.result ?? '';
             tool.status = 'complete';
           }
         }));
         break;

       case 'tool_result_delta':
         setActiveTools(produce((tools) => {
           const tool = tools.find((t) => t.callId === event.id || t.id === event.id);
           if (tool) {
             tool.result = (tool.result ?? '') + event.delta;
           }
         }));
         break;

       case 'tool_result_complete':
         setActiveTools(produce((tools) => {
           const tool = tools.find((t) => t.callId === event.id || t.id === event.id);
           if (tool) {
             tool.status = 'complete';
           }
         }));
         break;

       case 'tool_result_error':
         setActiveTools(produce((tools) => {
           const tool = tools.find((t) => t.callId === event.id || t.id === event.id);
           if (tool) {
             tool.result = event.error;
             tool.status = 'error';
           }
         }));
         break;

       case 'thinking':
         if (currentStreamingMessageId) {
           updateMessage(currentStreamingMessageId, {
             thinking: {
               content: (messages.find((m) => m.id === currentStreamingMessageId)?.thinking?.content ?? '') + event.content,
               isStreaming: true,
             },
           });
         }
         break;

       case 'message_complete': {
         const thinkingData = currentStreamingMessageId
           ? messages.find((m) => m.id === currentStreamingMessageId)?.thinking
           : undefined;
         if (currentStreamingMessageId) {
           updateMessage(currentStreamingMessageId, {
             id: event.id,
             content: event.content,
             toolCalls: event.tool_calls,
             ...(thinkingData ? {
               thinking: {
                 content: thinkingData.content,
                 isStreaming: false,
                 tokenCount: thinkingData.content.length,
               },
             } : {}),
           });
         }
         setIsStreaming(false);
         setIsLoading(false);
         // Reset per-turn tool state on message completion
         setActiveTools([]);
         currentStreamingMessageId = null;
         
         if (!hasReceivedFirstResponse && firstUserMessage) {
           hasReceivedFirstResponse = true;
           autoGenerateTitle();
         }
         break;
       }

       case 'error':
         setError(`${event.message} (${event.code})`);
         if (currentStreamingMessageId) {
           updateMessage(currentStreamingMessageId, {
             content: `Error: ${event.message}`,
           });
         }
         setIsStreaming(false);
         setIsLoading(false);
         currentStreamingMessageId = null;
         break;

       case 'interaction_requested': {
         const { type: _eventType, ...requestData } = event;
         setPendingInteraction(requestData as unknown as InteractionRequest);
         break;
       }

       case 'subagent_spawned':
         setSubagentEvents((prev) => [...prev, {
           id: event.id,
           prompt: event.prompt,
           status: 'spawned' as const,
         }]);
         break;

       case 'subagent_completed':
         setSubagentEvents(produce((events) => {
           const existing = events.find((e) => e.id === event.id);
           if (existing) {
             existing.status = 'completed';
             existing.summary = event.summary;
           } else {
             events.push({
               id: event.id,
               prompt: '',
               status: 'completed',
               summary: event.summary,
             });
           }
         }));
         break;

       case 'subagent_failed':
         setSubagentEvents(produce((events) => {
           const existing = events.find((e) => e.id === event.id);
           if (existing) {
             existing.status = 'failed';
             existing.error = event.error;
           } else {
             events.push({
               id: event.id,
               prompt: '',
               status: 'failed',
               error: event.error,
             });
           }
         }));
         break;

       case 'delegation_spawned':
         setSubagentEvents((prev) => [...prev, {
           id: event.id,
           prompt: event.prompt,
           status: 'spawned' as const,
           targetAgent: event.target_agent,
         }]);
         break;

       case 'delegation_completed':
         setSubagentEvents(produce((events) => {
           const existing = events.find((e) => e.id === event.id);
           if (existing) {
             existing.status = 'completed';
             existing.summary = event.summary;
           } else {
             events.push({
               id: event.id,
               prompt: '',
               status: 'completed',
               summary: event.summary,
             });
           }
         }));
         break;

       case 'delegation_failed':
         setSubagentEvents(produce((events) => {
           const existing = events.find((e) => e.id === event.id);
           if (existing) {
             existing.status = 'failed';
             existing.error = event.error;
           } else {
             events.push({
               id: event.id,
               prompt: '',
               status: 'failed',
               error: event.error,
             });
           }
         }));
         break;

       case 'context_usage':
        setContextUsage({ used: event.used, total: event.total });
        statusBarActions.setContextUsage({ used: event.used, total: event.total });
         break;

       case 'precognition_result': {
         const noteNames = event.notes.map((n) => n.name);
         const noteList = noteNames.length > 0 ? noteNames.join(', ') : 'none';
         addMessage({
           id: generateMessageId(),
           role: 'system' as Message['role'],
           content: `Auto-enriched with ${event.notes_count} notes: [${noteList}]`,
           timestamp: Date.now(),
           type: 'precognition',
         } as Message);
         break;
       }

       case 'mode_changed':
        setChatMode(event.mode);
        statusBarActions.setChatMode(event.mode);
         break;

       case 'session_event':
         // Session-level event (state change, etc.) — no client-side action needed
         break;
     }
   };

  const loadHistory = async (sessionId: string, kiln: string, signal?: AbortSignal) => {
    setIsLoadingHistory(true);
    try {
      const response = await getSessionHistory(sessionId, kiln, undefined, undefined, signal);
      const loadedMessages: Message[] = [];
      
      for (const evt of response.history) {
        if (evt.event === 'user_message' && evt.data?.content) {
          loadedMessages.push({
            id: evt.data.message_id as string || `user-${loadedMessages.length}`,
            role: 'user',
            content: evt.data.content,
            timestamp: Date.now() - (response.history.length - loadedMessages.length) * 1000,
          });
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
    }
    previousSessionId = newSessionId;
    
    if (!newSessionId) {
      return;
    }

    const abortController = new AbortController();
    historyAbortController = abortController;

    void (async () => {
      try {
        const session = await getSession(newSessionId);
        setSessionTitle(session.title);
        statusBarActions.setActiveModel(session.agent_model ?? null);
        statusBarActions.setActiveSessionId(session.id);
        statusBarActions.setActiveSessionTitle(session.title);
        await loadHistory(session.id, session.kiln, abortController.signal);
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          return;
        }

        try {
          const config = await getConfig();
          const sessions = await listSessions({ kiln: config.kiln_path });
          const persistedSession = sessions.find((s) => s.id === newSessionId) ?? null;
          const sessionKiln = persistedSession?.kiln || config.kiln_path;

          setSessionTitle(persistedSession?.title ?? null);
          statusBarActions.setActiveModel(persistedSession?.agent_model ?? null);
          statusBarActions.setActiveSessionId(newSessionId);
          statusBarActions.setActiveSessionTitle(
            persistedSession?.title ?? `Session ${newSessionId.slice(0, 8)}`,
          );

          await loadHistory(newSessionId, sessionKiln, abortController.signal);
        } catch (fallbackErr) {
          if (fallbackErr instanceof Error && fallbackErr.name === 'AbortError') {
            return;
          }
          console.error('Failed to load session metadata:', fallbackErr);
        }
      }
    })();

    eventSourceCleanup = subscribeToEvents(newSessionId, handleEvent);
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
  });

  const autoGenerateTitle = async () => {
    if (!props.sessionId) return;

    const currentTitle = sessionTitle();
    if (currentTitle && currentTitle.trim() !== '') return;

    try {
      const title = await generateSessionTitle(props.sessionId);
      await apiSetSessionTitle(props.sessionId, title);
      setSessionTitle(title);
      const tabInfo = findTabBySessionId(props.sessionId);
      if (tabInfo) {
        windowActions.updateTab(tabInfo.groupId, tabInfo.tab.id, { title });
      }
    } catch (err) {
      // Fallback to truncation on API failure
      if (firstUserMessage) {
        const truncated = firstUserMessage.slice(0, 50);
        const lastSpace = truncated.lastIndexOf(' ');
        const title = lastSpace > 0 ? truncated.slice(0, lastSpace) + '...' : truncated;
        try {
          await apiSetSessionTitle(props.sessionId, title);
          setSessionTitle(title);
          const tabInfo = findTabBySessionId(props.sessionId);
          if (tabInfo) {
            windowActions.updateTab(tabInfo.groupId, tabInfo.tab.id, { title });
          }
        } catch {
          console.error('Failed to auto-generate title:', err);
        }
      }
    }
  };

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
    messages: () => messages,
    isLoading,
    isStreaming,
    pendingInteraction,
    error,
    activeTools: () => activeTools,
    subagentEvents: () => subagentEvents,
    contextUsage,
    chatMode,
    isLoadingHistory,
    setChatMode,
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
  messages: () => [],
  isLoading: () => false,
  isStreaming: () => false,
  pendingInteraction: () => null,
  error: () => null,
  activeTools: () => [],
  subagentEvents: () => [],
  contextUsage: () => null,
  chatMode: () => 'normal',
  isLoadingHistory: () => false,
  setChatMode: () => {},
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
