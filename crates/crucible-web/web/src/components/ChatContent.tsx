import { Component, Show, For } from 'solid-js';
import { MessageList } from './MessageList';
import { ChatInput } from './ChatInput';
import { ToolCard } from './ToolCard';
import { SubagentCard } from './SubagentCard';
import { DelegationCard } from './DelegationCard';
import { InteractionHandler } from './interactions';
import { useChatSafe } from '@/contexts/ChatContext';

export const ChatContent: Component = () => {
  const chatCtx = useChatSafe();
  const { pendingInteraction, respondToInteraction } = chatCtx;

  const hasActiveTools = () => chatCtx.activeTools().length > 0;
  const hasSubagentEvents = () => chatCtx.subagentEvents().length > 0;

  return (
    <div class="h-full flex flex-col overflow-hidden">
      <div class="flex-1 min-h-0 flex flex-col">
        <MessageList />
      </div>

      {/* Active tool calls during streaming — shown above input */}
      <Show when={hasActiveTools()}>
        <div class="px-4 py-2 border-t border-neutral-800/50 max-h-64 overflow-y-auto">
          <For each={chatCtx.activeTools()}>
            {(tool) => <ToolCard toolCall={tool} />}
          </For>
        </div>
      </Show>

      <Show when={hasSubagentEvents()}>
        <div class="px-4 py-2 border-t border-neutral-800/50 max-h-64 overflow-y-auto">
          <For each={chatCtx.subagentEvents()}>
            {(evt) => (
              <Show when={evt.targetAgent} fallback={<SubagentCard event={evt} />}>
                <DelegationCard event={evt} />
              </Show>
            )}
          </For>
        </div>
      </Show>

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
