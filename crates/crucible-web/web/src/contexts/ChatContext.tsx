import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  createEffect,
  onCleanup,
  Accessor,
} from 'solid-js';
import { createStore } from 'solid-js/store';
import type {
  Message,
  ChatEvent,
  InteractionRequest,
  InteractionResponse,
  Session,
} from '@/lib/types';
import {
  sendChatMessage,
  subscribeToEvents,
  respondToInteraction as apiRespondToInteraction,
  cancelSession as apiCancelSession,
  generateMessageId,
} from '@/lib/api';

export interface ChatContextValue {
  messages: Accessor<Message[]>;
  isLoading: Accessor<boolean>;
  isStreaming: Accessor<boolean>;
  pendingInteraction: Accessor<InteractionRequest | null>;
  error: Accessor<string | null>;
  sendMessage: (content: string) => Promise<void>;
  respondToInteraction: (response: InteractionResponse) => Promise<void>;
  clearMessages: () => void;
  cancelStream: () => Promise<void>;
}

interface ChatProviderProps {
  session: Accessor<Session | null>;
  children: any;
}

const ChatContext = createContext<ChatContextValue>();

export const ChatProvider: ParentComponent<ChatProviderProps> = (props) => {
  const [messages, setMessages] = createStore<Message[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);
  const [isStreaming, setIsStreaming] = createSignal(false);
  const [pendingInteraction, setPendingInteraction] = createSignal<InteractionRequest | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  
  let eventSourceCleanup: (() => void) | null = null;
  let currentStreamingMessageId: string | null = null;

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

  const handleEvent = (event: ChatEvent) => {
    switch (event.type) {
      case 'token':
        if (currentStreamingMessageId) {
          appendToMessage(currentStreamingMessageId, event.content);
        }
        break;

      case 'tool_call':
        console.log('[ToolCall]', event.title, event.id, event.arguments);
        break;

      case 'tool_result':
        console.log('[ToolResult]', event.id, event.result);
        break;

      case 'thinking':
        console.log('[Thinking]', event.content);
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
        currentStreamingMessageId = null;
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
        setPendingInteraction(requestData as InteractionRequest);
        break;
      }

      case 'session_event':
        console.log('[SessionEvent]', event.event_type, event.data);
        break;
    }
  };

  createEffect(() => {
    const session = props.session();
    
    if (eventSourceCleanup) {
      eventSourceCleanup();
      eventSourceCleanup = null;
    }
    
    if (session && session.state === 'active') {
      eventSourceCleanup = subscribeToEvents(session.id, handleEvent);
    }
  });

  onCleanup(() => {
    if (eventSourceCleanup) {
      eventSourceCleanup();
      eventSourceCleanup = null;
    }
  });

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

  const clearMessages = () => {
    setMessages([]);
    setError(null);
    setPendingInteraction(null);
    currentStreamingMessageId = null;
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
