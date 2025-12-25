import { Component, For, createEffect } from 'solid-js';
import { Message } from './Message';
import { useChat } from '@/contexts/ChatContext';

export const MessageList: Component = () => {
  const { messages } = useChat();
  let containerRef: HTMLDivElement | undefined;

  const scrollToBottom = () => {
    if (containerRef) {
      containerRef.scrollTop = containerRef.scrollHeight;
    }
  };

  // Auto-scroll to bottom when messages change
  createEffect(() => {
    const _ = messages(); // Track messages
    scrollToBottom();
  });

  return (
    <div
      ref={containerRef}
      class="flex-1 overflow-y-auto px-4 py-6"
      data-testid="message-list"
    >
      <For each={messages()}>
        {(message) => <Message message={message} />}
      </For>

      {/* Empty state */}
      {messages().length === 0 && (
        <div class="h-full flex items-center justify-center">
          <p class="text-neutral-500 text-center">
            Start a conversation by typing a message or using voice input.
          </p>
        </div>
      )}
    </div>
  );
};
