/** Message in the chat */
export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
}

/** Chat context value exposed to components */
export interface ChatContextValue {
  messages: () => Message[];
  isLoading: () => boolean;
  sendMessage: (content: string) => Promise<void>;
  clearMessages: () => void;
}
