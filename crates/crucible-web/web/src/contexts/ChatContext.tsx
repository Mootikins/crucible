import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
} from 'solid-js';
import { createStore } from 'solid-js/store';
import type { Message, ChatContextValue } from '@/lib/types';
import { sendChatMessage, generateMessageId } from '@/lib/api';

const ChatContext = createContext<ChatContextValue>();

export const ChatProvider: ParentComponent = (props) => {
  const [messages, setMessages] = createStore<Message[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);

  const addMessage = (message: Message) => {
    setMessages((prev) => [...prev, message]);
  };

  const updateLastMessage = (content: string) => {
    setMessages((prev) => {
      if (prev.length === 0) return prev;
      const updated = [...prev];
      updated[updated.length - 1] = {
        ...updated[updated.length - 1],
        content,
      };
      return updated;
    });
  };

  const sendMessage = async (content: string) => {
    if (!content.trim() || isLoading()) return;

    // Add user message
    const userMessage: Message = {
      id: generateMessageId(),
      role: 'user',
      content: content.trim(),
      timestamp: Date.now(),
    };
    addMessage(userMessage);

    // Create placeholder for assistant response
    const assistantMessage: Message = {
      id: generateMessageId(),
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
    };
    addMessage(assistantMessage);

    setIsLoading(true);

    try {
      let accumulatedContent = '';
      await sendChatMessage(content, (chunk) => {
        accumulatedContent += chunk;
        updateLastMessage(accumulatedContent);
      });
    } catch (error) {
      console.error('Failed to send message:', error);
      updateLastMessage('Error: Failed to get response. Please try again.');
    } finally {
      setIsLoading(false);
    }
  };

  const clearMessages = () => {
    setMessages([]);
  };

  const value: ChatContextValue = {
    messages: () => messages,
    isLoading,
    sendMessage,
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
