import type { Component } from 'solid-js';
import { ChatProvider } from '@/contexts/ChatContext';
import { ChatContent } from '@/components/ChatContent';

interface ChatPanelProps {
  sessionId?: string;
}

export const ChatPanel: Component<ChatPanelProps> = (props) => {
  if (!props.sessionId) {
    return (
      <div class="h-full bg-shell-panel p-4 flex items-center justify-center text-muted text-sm">
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
