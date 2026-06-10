import type { Component } from 'solid-js';
import { ChatProvider } from '@/contexts/ChatContext';
import { ChatContent } from '@/components/ChatContent';

interface ChatPanelProps {
  sessionId?: string;
}

export const ChatPanel: Component<ChatPanelProps> = (props) => {
  if (!props.sessionId) {
    return (
      <div class="h-full bg-neutral-900 p-4 flex items-center justify-center text-neutral-400 text-sm">
        No session selected for this chat tab.
      </div>
    );
  }

  return (
    <ChatProvider sessionId={props.sessionId}>
      <ChatContent />
    </ChatProvider>
  );
};
