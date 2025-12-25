import { Component } from 'solid-js';
import { MessageList } from './MessageList';
import { ChatInput } from './ChatInput';

/**
 * The chat interface content, designed to be placed inside a dock panel.
 * Contains the message list and input area.
 */
export const ChatContent: Component = () => {
  return (
    <div class="h-full flex flex-col">
      {/* Messages area */}
      <div class="flex-1 overflow-hidden">
        <MessageList />
      </div>

      {/* Input area */}
      <ChatInput />
    </div>
  );
};
