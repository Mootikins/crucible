import { Component, Show, For } from 'solid-js';
import { MessageList } from './MessageList';
import { ChatInput } from './ChatInput';
import { ToolCard } from './ToolCard';
import { SubagentCard } from './SubagentCard';
import { DelegationCard } from './DelegationCard';
import { InteractionHandler } from './interactions';
import { useChatSafe } from '@/contexts/ChatContext';
import { useSessionSafe } from '@/contexts/SessionContext';

export const ChatContent: Component = () => {
  const chatCtx = useChatSafe();
  const { pendingInteraction, respondToInteraction, isLoadingHistory } = chatCtx;
  const { currentSession, createSession } = useSessionSafe();

  const hasActiveTools = () => chatCtx.activeTools().length > 0;
  const hasSubagentEvents = () => chatCtx.subagentEvents().length > 0;
  const isSessionEnded = () => currentSession()?.state === 'ended';

  const handleContinue = async () => {
    const session = currentSession();
    if (!session) return;
    await createSession({ kiln: session.kiln, workspace: session.workspace });
  };
  return (
    <div class="h-full flex flex-col overflow-hidden" data-message-renderer="markdown-it">
      <div class="flex-1 min-h-0 flex flex-col">
        <Show when={isLoadingHistory()}>
          <div class="flex flex-col gap-3 p-4">
            <div class="animate-pulse bg-neutral-700 rounded-lg h-12 w-3/4" />
            <div class="animate-pulse bg-neutral-700 rounded-lg h-16 w-full ml-auto" style="max-width: 80%" />
            <div class="animate-pulse bg-neutral-700 rounded-lg h-12 w-2/3" />
            <div class="animate-pulse bg-neutral-700 rounded-lg h-20 w-full ml-auto" style="max-width: 85%" />
          </div>
        </Show>
        <Show when={!isLoadingHistory()}>
          <MessageList />
        </Show>
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

      <Show when={isSessionEnded()}>
        <div class="flex flex-col items-center justify-center gap-2 py-3 px-4 border-t border-neutral-800 bg-neutral-900/50">
          <span class="text-sm text-neutral-500">This session has ended</span>
          <button
            class="text-xs text-neutral-400 border border-neutral-700 rounded px-3 py-1.5 hover:text-neutral-200 hover:border-neutral-500 transition-colors"
            onClick={handleContinue}
          >
            Continue as new session
          </button>
        </div>
      </Show>

      <Show when={!isSessionEnded()}>
        <ChatInput />
      </Show>
    </div>
  );
};
