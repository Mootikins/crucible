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
import type {
  Message,
  ChatEvent,
  InteractionRequest,
  InteractionResponse,
  Session,
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
  getSessionHistory,
} from '@/lib/api';

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
  sendMessage: (content: string) => Promise<void>;
  respondToInteraction: (response: InteractionResponse) => Promise<void>;
  clearMessages: () => void;
  cancelStream: () => Promise<void>;
}

interface ChatProviderProps {
  session: Accessor<Session | null>;
  setSessionTitle: (title: string) => Promise<void>;
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
  
  let eventSourceCleanup: (() => void) | null = null;
  let currentStreamingMessageId: string | null = null;
  let firstUserMessage: string | null = null;
  let hasReceivedFirstResponse = false;
  let previousSessionId: string | null = null;

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
         // Thinking content is tracked but not yet rendered — placeholder for Wave 2 ThinkingBlock component
         break;

       case 'message_complete':
         if (currentStreamingMessageId) {
           updateMessage(currentStreamingMessageId, {
             id: event.id,
             content: event.content,
             toolCalls: event.tool_calls,
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
         break;

       case 'session_event':
         console.log('[SessionEvent]', event.event_type, event.data);
         break;
     }
   };

  const loadHistory = async (session: Session) => {
    try {
      const response = await getSessionHistory(session.id, session.kiln);
      const loadedMessages: Message[] = [];
      
      for (const evt of response.history) {
        const rawEvt = evt as unknown as { 
          event?: string; 
          data?: { 
            full_response?: string; 
            content?: string;
            message_id?: string;
          } 
        };
        
        if (rawEvt.event === 'user_message' && rawEvt.data?.content) {
          loadedMessages.push({
            id: rawEvt.data.message_id || `user-${loadedMessages.length}`,
            role: 'user',
            content: rawEvt.data.content,
            timestamp: Date.now() - (response.history.length - loadedMessages.length) * 1000,
          });
        } else if (rawEvt.event === 'message_complete' && rawEvt.data?.full_response) {
          loadedMessages.push({
            id: rawEvt.data.message_id || `assistant-${loadedMessages.length}`,
            role: 'assistant',
            content: rawEvt.data.full_response,
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
      console.error('Failed to load session history:', err);
    }
  };

  createEffect(() => {
    const session = props.session();
    const newSessionId = session?.id ?? null;
    
    if (eventSourceCleanup) {
      eventSourceCleanup();
      eventSourceCleanup = null;
    }
    
    if (newSessionId !== previousSessionId && previousSessionId !== null) {
      clearMessages();
    }
    previousSessionId = newSessionId;
    
    if (session) {
      loadHistory(session);
      if (session.state === 'active') {
        eventSourceCleanup = subscribeToEvents(session.id, handleEvent);
      }
    }
  });

  onCleanup(() => {
    if (eventSourceCleanup) {
      eventSourceCleanup();
      eventSourceCleanup = null;
    }
  });

  const autoGenerateTitle = async () => {
    const session = props.session();
    if (!session || !firstUserMessage) return;
    
    if (session.title && session.title.trim() !== '') {
      return;
    }
    
    const truncated = firstUserMessage.slice(0, 50);
    const lastSpace = truncated.lastIndexOf(' ');
    const title = lastSpace > 0 ? truncated.slice(0, lastSpace) + '...' : truncated;
    
    try {
      await props.setSessionTitle(title);
    } catch (err) {
      console.error('Failed to auto-generate title:', err);
    }
  };

  const sendMessage = async (content: string) => {
    const session = props.session();
    if (!content.trim() || isLoading() || !session) return;

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
      await sendChatMessage(session.id, content);
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
    const session = props.session();
    const request = pendingInteraction();
    if (!request || !session) return;

    setPendingInteraction(null);

    try {
      await apiRespondToInteraction(session.id, request.id, response);
    } catch (err) {
      console.error('Failed to send interaction response:', err);
      setError(err instanceof Error ? err.message : 'Failed to respond');
    }
  };

  const cancelStream = async () => {
    const session = props.session();
    if (session) {
      try {
        await apiCancelSession(session.id);
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
    sendMessage,
    respondToInteraction,
    clearMessages,
    cancelStream,
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
  sendMessage: noopAsync,
  respondToInteraction: noopAsync,
  clearMessages: () => {},
  cancelStream: noopAsync,
};

export function useChatSafe(): ChatContextValue {
  const context = useContext(ChatContext);
  return context ?? fallbackChatContext;
}
