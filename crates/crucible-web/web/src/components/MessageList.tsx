import { Component, For, Show, createEffect, createMemo } from 'solid-js';
import { Message } from './Message';
import { ToolCard } from './ToolCard';
import { InteractionHandler } from './interactions';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import type { Message as MessageType } from '@/lib/types';

/** Transcript row: a message, or a run of consecutive tool calls collapsed
 * into one block (sequential tools read as a single unit of activity). */
type TranscriptRow =
  | { kind: 'message'; message: MessageType }
  | { kind: 'tools'; key: string; items: MessageType[] };

export const MessageList: Component = () => {
  const { messages, isStreaming, pendingInteraction, respondToInteraction } = useChatSafe();
  const { currentSession } = useSessionSafe();
  let bottomRef: HTMLDivElement | undefined;

  const scrollToBottom = () => {
    bottomRef?.scrollIntoView({ behavior: 'instant', block: 'end' });
  };

  createEffect(() => {
    messages();
    pendingInteraction();
    queueMicrotask(scrollToBottom);
  });

  const session = () => currentSession();

  const rows = createMemo<TranscriptRow[]>(() => {
    const out: TranscriptRow[] = [];
    for (const message of messages()) {
      if (message.role === 'tool') {
        const last = out[out.length - 1];
        if (last?.kind === 'tools') {
          last.items.push(message);
        } else {
          out.push({ kind: 'tools', key: `tools-${message.id}`, items: [message] });
        }
      } else {
        out.push({ kind: 'message', message });
      }
    }
    return out;
  });

  return (
    <div
      class="flex-1 overflow-y-auto px-4 py-6"
      data-testid="message-list"
    >
      <For each={rows()}>
        {(row, index) => {
          if (row.kind === 'tools') {
            return (
              <div class="group relative mb-3 flex justify-start" data-role="tool">
                <div
                  class="w-full max-w-3xl border border-hairline rounded-lg overflow-hidden divide-y divide-hairline bg-surface-elevated"
                  data-testid="tool-group"
                >
                  <For each={row.items}>
                    {(m) => <ToolCard toolCall={m.toolCall!} grouped />}
                  </For>
                </div>
              </div>
            );
          }
          const message = row.message;
          const isLastAssistant = createMemo(() => {
            const msgs = messages();
            for (let i = msgs.length - 1; i >= 0; i--) {
              if (msgs[i].role === 'assistant') {
                return msgs[i].id === message.id;
              }
            }
            return false;
          });
          return (
            <Message
              message={message}
              isStreaming={isStreaming() && index() === rows().length - 1 && message.role === 'assistant'}
              isLast={isLastAssistant()}
            />
          );
        }}
      </For>

      {/* Permission/ask prompts appear inline at the point in the
          conversation where the agent is blocked, like other agent UIs. */}
      <Show when={pendingInteraction()}>
        {(request) => (
          <div class="max-w-3xl">
            <InteractionHandler request={request()} onRespond={respondToInteraction} />
          </div>
        )}
      </Show>
      <div ref={bottomRef} class="h-px" />

      <Show when={messages().length === 0}>
        <div class="h-full flex flex-col items-center justify-center gap-4">
          <Show
            when={session()}
            fallback={
              <>
                <div class="w-16 h-16 rounded-full bg-surface-elevated flex items-center justify-center">
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 24 24"
                    fill="currentColor"
                    class="w-8 h-8 text-muted-dark"
                  >
                    <path fill-rule="evenodd" d="M4.848 2.771A49.144 49.144 0 0112 2.25c2.43 0 4.817.178 7.152.52 1.978.292 3.348 2.024 3.348 3.97v6.02c0 1.946-1.37 3.678-3.348 3.97a48.901 48.901 0 01-3.476.383.39.39 0 00-.297.17l-2.755 4.133a.75.75 0 01-1.248 0l-2.755-4.133a.39.39 0 00-.297-.17 48.9 48.9 0 01-3.476-.384c-1.978-.29-3.348-2.024-3.348-3.97V6.741c0-1.946 1.37-3.68 3.348-3.97z" clip-rule="evenodd" />
                  </svg>
                </div>
                <p class="text-muted-dark text-center">
                  Select or create a session to start chatting
                </p>
              </>
            }
          >
            <>
              <div class="w-16 h-16 rounded-full bg-primary/15 flex items-center justify-center">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                  class="w-8 h-8 text-primary"
                >
                  <path fill-rule="evenodd" d="M4.848 2.771A49.144 49.144 0 0112 2.25c2.43 0 4.817.178 7.152.52 1.978.292 3.348 2.024 3.348 3.97v6.02c0 1.946-1.37 3.678-3.348 3.97a48.901 48.901 0 01-3.476.383.39.39 0 00-.297.17l-2.755 4.133a.75.75 0 01-1.248 0l-2.755-4.133a.39.39 0 00-.297-.17 48.9 48.9 0 01-3.476-.384c-1.978-.29-3.348-2.024-3.348-3.97V6.741c0-1.946 1.37-3.68 3.348-3.97z" clip-rule="evenodd" />
                </svg>
              </div>
              <p class="text-muted text-center">
                Start a conversation by typing a message or using voice input
              </p>
              <Show when={session()?.agent_model}>
                <p class="text-muted-dark text-sm">
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
