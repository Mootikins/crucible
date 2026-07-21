import { Component, Show, For } from 'solid-js';
import { MessageList } from './MessageList';
import { ChatInput } from './ChatInput';
import { SubagentCard } from './SubagentCard';
import { DelegationCard } from './DelegationCard';
import { useChatSafe } from '@/contexts/ChatContext';

export const ChatContent: Component = () => {
  const chatCtx = useChatSafe();
  const { isLoadingHistory } = chatCtx;

  const hasSubagentEvents = () => chatCtx.subagentEvents().length > 0;
  return (
    <div class="h-full flex flex-col overflow-hidden" data-message-renderer="markdown-it">
      <div class="flex-1 min-h-0 flex flex-col">
        <Show when={isLoadingHistory()}>
          {/* Skeleton mirrors the real transcript: full-width, left-aligned
              rows (no bubble-era right-aligned fakes). */}
          <div class="flex flex-col gap-3 p-4">
            <div class="animate-pulse bg-surface-elevated rounded-md h-8 w-full" />
            <div class="animate-pulse bg-surface-elevated rounded-md h-16 w-full" />
            <div class="animate-pulse bg-surface-elevated rounded-md h-8 w-3/4" />
            <div class="animate-pulse bg-surface-elevated rounded-md h-20 w-full" />
          </div>
        </Show>
        <Show when={!isLoadingHistory()}>
          {/* Tool calls and permission prompts render inline in the
              transcript (MessageList), not in strips above the input. */}
          <MessageList />
        </Show>
      </div>

      <Show when={hasSubagentEvents()}>
        <div class="px-4 py-2 border-t border-hairline max-h-64 overflow-y-auto">
          <For each={chatCtx.subagentEvents()}>
            {(evt) => (
              <Show when={evt.targetAgent} fallback={<SubagentCard event={evt} />}>
                <DelegationCard event={evt} />
              </Show>
            )}
          </For>
        </div>
      </Show>

      <ChatInput />
    </div>
  );
};
