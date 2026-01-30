import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
} from 'solid-js';
import { createStore } from 'solid-js/store';
import type {
  Message,
  ChatContextValue,
  ChatEvent,
  InteractionRequest,
  InteractionResponse,
} from '@/lib/types';
import { sendChatMessage, generateMessageId, respondToInteraction as apiRespondToInteraction } from '@/lib/api';

const ChatContext = createContext<ChatContextValue>();

export const ChatProvider: ParentComponent = (props) => {
  const [messages, setMessages] = createStore<Message[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);
  const [pendingInteraction, setPendingInteraction] =
    createSignal<InteractionRequest | null>(null);

  const addMessage = (message: Message) => {
    setMessages((prev) => [...prev, message]);
  };

  const updateLastMessage = (updates: Partial<Message>) => {
    setMessages((prev) => {
      if (prev.length === 0) return prev;
      const updated = [...prev];
      updated[updated.length - 1] = {
        ...updated[updated.length - 1],
        ...updates,
      };
      return updated;
    });
  };

  const handleEvent = (event: ChatEvent) => {
    switch (event.type) {
      case 'token':
        setMessages((prev) => {
          if (prev.length === 0) return prev;
          const updated = [...prev];
          const last = updated[updated.length - 1];
          updated[updated.length - 1] = {
            ...last,
            content: last.content + event.content,
          };
          return updated;
        });
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
        updateLastMessage({
          id: event.id,
          content: event.content,
          toolCalls: event.tool_calls,
        });
        break;

      case 'error':
        updateLastMessage({
          content: `Error: ${event.message} (${event.code})`,
        });
        break;

      case 'interaction_requested': {
        const request = event as unknown as InteractionRequest;
        setPendingInteraction(request);
        break;
      }
    }
  };

  const sendMessage = async (content: string) => {
    if (!content.trim() || isLoading()) return;

    const userMessage: Message = {
      id: generateMessageId(),
      role: 'user',
      content: content.trim(),
      timestamp: Date.now(),
    };
    addMessage(userMessage);

    const assistantMessage: Message = {
      id: generateMessageId(),
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
    };
    addMessage(assistantMessage);

    setIsLoading(true);

    try {
      await sendChatMessage(content, handleEvent);
    } catch (error) {
      console.error('Failed to send message:', error);
      updateLastMessage({
        content: 'Error: Failed to connect to server. Please try again.',
      });
    } finally {
      setIsLoading(false);
    }
  };

  const clearMessages = () => {
    setMessages([]);
  };

  const respondToInteraction = async (response: InteractionResponse) => {
    const request = pendingInteraction();
    if (!request) return;

    setPendingInteraction(null);

    try {
      await apiRespondToInteraction(request.id, response);
    } catch (error) {
      console.error('Failed to send interaction response:', error);
    }
  };

  const value: ChatContextValue = {
    messages: () => messages,
    isLoading,
    pendingInteraction,
    sendMessage,
    respondToInteraction,
    clearMessages,
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
