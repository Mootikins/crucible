import { Component, Show } from 'solid-js';
import { MessageList } from './MessageList';
import { ChatInput } from './ChatInput';
import { InteractionHandler } from './interactions';
import { useChatSafe } from '@/contexts/ChatContext';

export const ChatContent: Component = () => {
  const { pendingInteraction, respondToInteraction } = useChatSafe();

  return (
    <div class="h-full flex flex-col overflow-hidden">
      <div class="flex-1 min-h-0">
        <MessageList />
      </div>

      <Show when={pendingInteraction()}>
        {(request) => (
          <div class="px-4">
            <InteractionHandler request={request()} onRespond={respondToInteraction} />
          </div>
        )}
      </Show>

      <ChatInput />
    </div>
  );
};
