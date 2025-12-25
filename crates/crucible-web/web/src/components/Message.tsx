import { Component, Show } from 'solid-js';
import type { Message as MessageType } from '@/lib/types';

interface MessageProps {
  message: MessageType;
}

export const Message: Component<MessageProps> = (props) => {
  const isUser = () => props.message.role === 'user';

  return (
    <div
      class={`flex ${isUser() ? 'justify-end' : 'justify-start'} mb-4`}
      data-testid={`message-${props.message.id}`}
      data-role={props.message.role}
    >
      <div
        class={`max-w-[80%] rounded-2xl px-4 py-2 ${
          isUser()
            ? 'bg-blue-600 text-white rounded-br-md'
            : 'bg-neutral-800 text-neutral-100 rounded-bl-md'
        }`}
      >
        <Show
          when={props.message.content}
          fallback={
            <span class="inline-flex items-center gap-1">
              <span class="w-2 h-2 bg-neutral-500 rounded-full animate-pulse" />
              <span class="w-2 h-2 bg-neutral-500 rounded-full animate-pulse delay-75" />
              <span class="w-2 h-2 bg-neutral-500 rounded-full animate-pulse delay-150" />
            </span>
          }
        >
          <p class="whitespace-pre-wrap break-words">{props.message.content}</p>
        </Show>
      </div>
    </div>
  );
};
