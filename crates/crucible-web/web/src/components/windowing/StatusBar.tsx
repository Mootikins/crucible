import { Component, Show, createSignal } from 'solid-js';
import { createDraggable, createDroppable } from '@thisbeyond/solid-dnd';
import { windowStore } from '@/stores/windowStore';
import { statusBarStore } from '@/stores/statusBarStore';
import { notificationStore } from '@/stores/notificationStore';
import type { ChatMode } from '@/lib/types';
import { IconLayout, IconBell } from './icons';
import { NotificationCenter } from '@/components/NotificationCenter';

export const StatusBar: Component = () => {
  const totalTabs = () =>
    Object.values(windowStore.tabGroups).reduce(
      (sum, group) => sum + group.tabs.length,
      0
    );
  const minimizedCount = () =>
    windowStore.floatingWindows.filter((w) => w.isMinimized).length;

  const newFloatingId = 'newFloating';
  const dropNewFloatingId = 'dropNewFloating';
  // SolidJS directives: variables referenced via use:draggable / use:droppable in JSX
  const draggable = createDraggable(newFloatingId, { type: 'newFloating' });
  void draggable;
  const droppable = createDroppable(dropNewFloatingId, { type: 'newFloating' });
  void droppable;

  const [drawerOpen, setDrawerOpen] = createSignal(false);
  const unreadCount = () => notificationStore.notificationCount();

  const modeColor = (mode: ChatMode): string => {
    switch (mode) {
      case 'normal': return 'bg-emerald-600/80 text-emerald-100';
      case 'plan': return 'bg-blue-600/80 text-blue-100';
      case 'auto': return 'bg-amber-600/80 text-amber-100';
    }
  };

  const formatTokens = (n: number): string => {
    if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
    return String(n);
  };

  const usagePercent = () => {
    const u = statusBarStore.contextUsage();
    if (!u || u.total === 0) return 0;
    return Math.min(100, (u.used / u.total) * 100);
  };

  return (
    <>
      <div class="flex items-center justify-between px-2 h-5 bg-zinc-950 border-t border-zinc-800 text-[10px] text-zinc-500 select-none">
        <div class="flex items-center gap-3">
          {/* Mode badge */}
          <span
            class={`px-1.5 rounded-sm font-medium uppercase tracking-wider text-[9px] leading-tight ${modeColor(statusBarStore.chatMode())}`}
            data-testid="status-mode"
          >
            {statusBarStore.chatMode()}
          </span>
          <span>Ready</span>
          <span>{totalTabs()} tabs</span>
          {minimizedCount() > 0 && (
            <span class="text-amber-500">{minimizedCount()} minimized</span>
          )}
        </div>
        <div class="flex items-center gap-3">
          <div
            use:droppable
            class="flex items-center gap-2 px-2 py-1 rounded"
          >
            <div
              use:draggable
              class="flex items-center gap-2 px-2 py-1 text-xs text-zinc-500 hover:text-zinc-300 cursor-grab active:cursor-grabbing transition-colors"
            >
              <IconLayout class="w-3.5 h-3.5" />
              <span>New Window</span>
            </div>
          </div>
          {/* Context usage */}
          <Show when={statusBarStore.contextUsage()}>
            {(usage) => (
              <div class="flex items-center gap-1.5" data-testid="status-context-usage">
                <span class="text-zinc-400 tabular-nums">
                  {formatTokens(usage().used)} / {formatTokens(usage().total)}
                </span>
                <div class="w-12 h-1.5 bg-zinc-800 rounded-full overflow-hidden">
                  <div
                    class="h-full rounded-full transition-all duration-300"
                    classList={{
                      'bg-emerald-500': usagePercent() < 60,
                      'bg-amber-500': usagePercent() >= 60 && usagePercent() < 85,
                      'bg-red-500': usagePercent() >= 85,
                    }}
                    style={{ width: `${usagePercent()}%` }}
                  />
                </div>
              </div>
            )}
          </Show>
          {/* Active model */}
          <Show when={statusBarStore.activeModel()}>
            {(model) => (
              <span class="text-zinc-400 font-mono" data-testid="status-model">{model()}</span>
            )}
          </Show>
          {/* Notification bell */}
          <button
            type="button"
            class="relative p-0.5 text-zinc-500 hover:text-zinc-300 transition-colors"
            onClick={() => setDrawerOpen(!drawerOpen())}
            aria-label="Toggle notifications"
          >
            <IconBell class="w-3.5 h-3.5" />
            <Show when={unreadCount() > 0}>
              <span class="absolute -top-1 -right-1 px-0.5 min-w-[12px] text-center rounded-full bg-red-600 text-white text-[8px] font-bold leading-[12px]">
                {unreadCount() > 99 ? '99+' : unreadCount()}
              </span>
            </Show>
          </button>
          <div class="w-px h-3 bg-zinc-800" />
          <span>UTF-8</span>
          <span>TypeScript</span>
        </div>
      </div>
      <NotificationCenter open={drawerOpen()} onClose={() => setDrawerOpen(false)} />
    </>
  );
};
