import { Component, For, Show, createEffect } from 'solid-js';
import { Message } from './Message';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';

export const MessageList: Component = () => {
  const { messages, isStreaming } = useChatSafe();
  const { currentSession } = useSessionSafe();
  let bottomRef: HTMLDivElement | undefined;

  const scrollToBottom = () => {
    bottomRef?.scrollIntoView({ behavior: 'instant', block: 'end' });
  };

  createEffect(() => {
    const _ = messages();
    queueMicrotask(scrollToBottom);
  });

  const session = () => currentSession();

  return (
    <div
      class="flex-1 overflow-y-auto px-4 py-6"
      data-testid="message-list"
    >
      <For each={messages()}>
        {(message, index) => (
          <Message
            message={message}
            isStreaming={isStreaming() && index() === messages().length - 1 && message.role === 'assistant'}
          />
        )}
      </For>
      <div ref={bottomRef} class="h-px" />

      <Show when={messages().length === 0}>
        <div class="h-full flex flex-col items-center justify-center gap-4">
          <Show
            when={session()}
            fallback={
              <>
                <div class="w-16 h-16 rounded-full bg-neutral-800 flex items-center justify-center">
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 24 24"
                    fill="currentColor"
                    class="w-8 h-8 text-neutral-500"
                  >
                    <path fill-rule="evenodd" d="M4.848 2.771A49.144 49.144 0 0112 2.25c2.43 0 4.817.178 7.152.52 1.978.292 3.348 2.024 3.348 3.97v6.02c0 1.946-1.37 3.678-3.348 3.97a48.901 48.901 0 01-3.476.383.39.39 0 00-.297.17l-2.755 4.133a.75.75 0 01-1.248 0l-2.755-4.133a.39.39 0 00-.297-.17 48.9 48.9 0 01-3.476-.384c-1.978-.29-3.348-2.024-3.348-3.97V6.741c0-1.946 1.37-3.68 3.348-3.97z" clip-rule="evenodd" />
                  </svg>
                </div>
                <p class="text-neutral-500 text-center">
                  Select or create a session to start chatting
                </p>
              </>
            }
          >
            <>
              <div class="w-16 h-16 rounded-full bg-blue-900/30 flex items-center justify-center">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                  class="w-8 h-8 text-blue-400"
                >
                  <path fill-rule="evenodd" d="M4.848 2.771A49.144 49.144 0 0112 2.25c2.43 0 4.817.178 7.152.52 1.978.292 3.348 2.024 3.348 3.97v6.02c0 1.946-1.37 3.678-3.348 3.97a48.901 48.901 0 01-3.476.383.39.39 0 00-.297.17l-2.755 4.133a.75.75 0 01-1.248 0l-2.755-4.133a.39.39 0 00-.297-.17 48.9 48.9 0 01-3.476-.384c-1.978-.29-3.348-2.024-3.348-3.97V6.741c0-1.946 1.37-3.68 3.348-3.97z" clip-rule="evenodd" />
                </svg>
              </div>
              <p class="text-neutral-400 text-center">
                Start a conversation by typing a message or using voice input
              </p>
              <Show when={session()?.agent_model}>
                <p class="text-neutral-600 text-sm">
                  Model: {session()?.agent_model}
                </p>
              </Show>
            </>
          </Show>
        </div>
      </Show>
    </div>
  );
};
