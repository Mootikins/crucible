import { Component, Show, For } from 'solid-js';
import { MessageList } from './MessageList';
import { ChatInput } from './ChatInput';
import { SubagentCard } from './SubagentCard';
import { DelegationCard } from './DelegationCard';
import { useSessionSafe } from '@/contexts/SessionContext';
import { windowActions, windowStore } from '@/stores/windowStore';
import { collectLeafGroupIds } from '@/windowing';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { menuContent, menuItem, menuTrigger } from '@/components/ui/menu-style';
import { notificationActions } from '@/stores/notificationStore';
import { Maximize2, Minimize2, MoreHorizontal } from '@/lib/icons';
import { useChatSafe } from '@/contexts/ChatContext';

export const ChatContent: Component = () => {
  const chatCtx = useChatSafe();
  const { isLoadingHistory } = chatCtx;
  const sessions = useSessionSafe();
  const session = () => sessions.sessions().find((session) => session.session_id === chatCtx.sessionId?.());
  const title = () => session()?.title || 'Session';
  const rail = () => (['left', 'right'] as const).find((side) =>
    collectLeafGroupIds(windowStore.edgePanels[side].layout).some((id) =>
      windowStore.tabGroups[id]?.tabs.some((tab) => tab.contentType === 'chat' && tab.metadata?.sessionId === chatCtx.sessionId?.()),
    ),
  );

  const sessionAction = async (action: string) => {
    const id = chatCtx.sessionId?.();
    if (!id) return;
    try {
      if (action === 'archive') await sessions.archiveSession(id);
      if (action === 'restore') await sessions.unarchiveSession(id);
      if (action === 'stop') await chatCtx.cancelStream();
      if (action === 'copy') await navigator.clipboard.writeText(id);
    } catch (error) {
      notificationActions.addNotification('error', error instanceof Error ? error.message : 'Session action failed');
    }
  };

  const hasSubagentEvents = () => chatCtx.subagentEvents().length > 0;
  return (
    <div class="h-full flex flex-col overflow-hidden" data-message-renderer="markdown-it">
      <div class="session-header flex h-9 shrink-0 items-center gap-2 px-4">
        <span class="min-w-0 flex-1 truncate text-xs font-semibold text-shell-ink">{title()}</span>
        <Show when={rail()}>{(side) => <button type="button" class="text-muted hover:text-shell-ink focus-ring" aria-label={windowStore.expandedEdge === side() ? 'Restore session width' : 'Expand session'} onClick={() => windowActions.toggleEdgeExpanded(side())}>
          <Show when={windowStore.expandedEdge === side()} fallback={<Maximize2 class="h-4 w-4" />}><Minimize2 class="h-4 w-4" /></Show>
        </button>}</Show>
        <Show when={chatCtx.sessionId?.()}>
          <Menu.Root onSelect={(event) => void sessionAction(event.value)}>
            <Menu.Trigger aria-label="Session actions" title="More session actions" class={menuTrigger}><MoreHorizontal class="h-4 w-4" /></Menu.Trigger>
            <Portal><Menu.Positioner><Menu.Content class={menuContent}>
              <Show when={chatCtx.isStreaming?.() || chatCtx.isLoading?.()}><Menu.Item value="stop" class={menuItem}>Stop current turn</Menu.Item></Show>
              <Menu.Item value={session()?.archived ? 'restore' : 'archive'} class={menuItem}>{session()?.archived ? 'Restore session' : 'Archive session'}</Menu.Item>
              <Menu.Item value="copy" class={menuItem}>Copy session ID</Menu.Item>
            </Menu.Content></Menu.Positioner></Portal>
          </Menu.Root>
        </Show>
      </div>
      <div class="flex-1 min-h-0 flex flex-col">
        <Show when={isLoadingHistory()}>
          {/* Skeleton mirrors the real transcript: full-width, left-aligned
              rows (no bubble-era right-aligned fakes), spaced on the same
              rhythm the loaded transcript uses, so nothing shifts when the
              history arrives. */}
          <div class="flex flex-col gap-[var(--cru-turn-gap)] p-4">
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
