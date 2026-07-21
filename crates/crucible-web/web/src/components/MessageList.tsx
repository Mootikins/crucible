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
  | { kind: 'tools'; items: MessageType[] };

type MessageRow = Extract<TranscriptRow, { kind: 'message' }>;
type ToolsRow = Extract<TranscriptRow, { kind: 'tools' }>;

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

  // <For> keys rows by reference, so rebuilding fresh wrapper objects every
  // recompute would dispose and recreate EVERY row on each streamed token —
  // resetting ToolCard's expanded state and re-rendering all markdown. Cache
  // wrappers and reuse them when the underlying identity is unchanged: a
  // message row by its message reference, a tools group by the identity list
  // of its items (both stay stable across token appends because the store
  // preserves proxies for untouched entries). Only the row whose message
  // actually changed gets a new wrapper.
  const messageRowCache = new Map<string, MessageRow>();
  const toolsRowCache = new Map<string, ToolsRow>();

  const rows = createMemo<TranscriptRow[]>(() => {
    const out: TranscriptRow[] = [];
    const seenMessageIds = new Set<string>();
    const seenToolKeys = new Set<string>();
    let group: { key: string; items: MessageType[] } | null = null;

    const flushGroup = () => {
      if (!group) return;
      const { key, items } = group;
      const cached = toolsRowCache.get(key);
      const reusable = cached
        && cached.items.length === items.length
        && cached.items.every((m, i) => m === items[i]);
      const wrapper = reusable ? cached! : { kind: 'tools' as const, items };
      toolsRowCache.set(key, wrapper);
      seenToolKeys.add(key);
      out.push(wrapper);
      group = null;
    };

    for (const message of messages()) {
      if (message.role === 'tool') {
        if (group) {
          group.items.push(message);
        } else {
          group = { key: `tools-${message.id}`, items: [message] };
        }
      } else {
        flushGroup();
        const cached = messageRowCache.get(message.id);
        const wrapper = cached && cached.message === message
          ? cached
          : { kind: 'message' as const, message };
        messageRowCache.set(message.id, wrapper);
        seenMessageIds.add(message.id);
        out.push(wrapper);
      }
    }
    flushGroup();

    // Evict wrappers for messages/groups that vanished (clear, session switch)
    // so the caches don't grow unbounded across a long-lived provider.
    for (const id of messageRowCache.keys()) {
      if (!seenMessageIds.has(id)) messageRowCache.delete(id);
    }
    for (const key of toolsRowCache.keys()) {
      if (!seenToolKeys.has(key)) toolsRowCache.delete(key);
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
