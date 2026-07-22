import { Component, Show, createSignal } from 'solid-js';
import { windowStore } from '@/stores/windowStore';
import { statusBarStore, pathBasename } from '@/stores/statusBarStore';
import { useEditorSafe } from '@/contexts/EditorContext';
import { useSettingsSafe } from '@/contexts/SettingsContext';
import { shellStore, shellActions } from '@/stores/shellStore';
import { attentionStore } from '@/stores/attentionStore';
import { notificationStore } from '@/stores/notificationStore';
import { IconBell } from './icons';
import { NotificationCenter } from '@/components/NotificationCenter';

export const StatusBar: Component = () => {
  const minimizedCount = () =>
    windowStore.floatingWindows.filter((w) => w.isMinimized).length;

  const [drawerOpen, setDrawerOpen] = createSignal(false);
  const unreadCount = () => notificationStore.notificationCount();

  // Configurable save affordance (Settings → Editor): the active buffer's
  // dirty state + one-click save, replacing the per-panel save toolbar.
  const editor = useEditorSafe();
  const { settings } = useSettingsSafe();
  const activeDirtyFile = () => {
    const path = editor.activeFile();
    if (!path) return null;
    const file = editor.openFiles().find((f) => f.path === path);
    return file?.dirty ? file : null;
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

  // Where you are in the shell (Crucible Shell design turn 5 status bar).
  const surfaceIndicator = () => {
    const kiln = pathBasename(statusBarStore.kilnPath());
    switch (shellStore.activeSurface()) {
      case 'inbox':
        return '▤ inbox';
      case 'edit':
        return kiln ? `✎ editing ${kiln}` : '✎ editing';
      case 'session': {
        const title = statusBarStore.activeSessionTitle();
        return title ? `◆ ${title}` : '◆ session';
      }
    }
  };

  return (
    <>
      <div class="flex items-center justify-between px-2 h-5 bg-shell-bg border-t border-hairline text-[10px] text-muted-dark select-none">
        <div class="flex items-center gap-3">
          <span class="font-mono text-primary" data-testid="status-surface">
            {surfaceIndicator()}
          </span>
          {/* Chat mode lives in each composer (ChatModeControl); tab count
              and the workspace/kiln context line carried no signal — gone. */}
          {minimizedCount() > 0 && (
            <span class="text-attention">{minimizedCount()} minimized</span>
          )}
        </div>
        <div class="flex items-center gap-3">
          {/* Attention chip: pending approvals/interactions waiting on the
              user (the old header's Inbox badge). Hidden at zero — the Inbox
              panel stays reachable from the command palette. */}
          <Show when={attentionStore.attentionCount() > 0}>
            <button
              type="button"
              data-testid="status-inbox"
              title="Open Inbox"
              class="flex items-center gap-1 px-1.5 rounded-sm bg-attention/15 text-attention font-mono hover:bg-attention/25 transition-colors"
              onClick={() => shellActions.goInbox()}
            >
              ▤ {attentionStore.attentionCount()}
            </button>
          </Show>
          <Show when={settings.editor.showSaveButton && activeDirtyFile()}>
            {(file) => (
              <button
                type="button"
                data-testid="status-save"
                class="flex items-center gap-1.5 px-2 rounded text-attention hover:text-attention hover:bg-hover-wash transition-colors"
                title={`Save ${file().path.split('/').pop()} (Ctrl+S / Ctrl+Enter)`}
                onClick={() => void editor.saveFile(file().path)}
              >
                <span>●</span>
                <span>Save</span>
              </button>
            )}
          </Show>
          {/* Context usage */}
          <Show when={statusBarStore.contextUsage()}>
            {(usage) => (
              <div class="flex items-center gap-1.5" data-testid="status-context-usage">
                <span class="text-muted tabular-nums">
                  {formatTokens(usage().used)} / {formatTokens(usage().total)}
                </span>
                <div class="w-12 h-1.5 bg-control rounded-full overflow-hidden">
                  <div
                    class="h-full rounded-full transition-all duration-300"
                    classList={{
                      'bg-ok': usagePercent() < 60,
                      'bg-attention': usagePercent() >= 60 && usagePercent() < 85,
                      'bg-error': usagePercent() >= 85,
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
              <span class="text-muted font-mono" data-testid="status-model">{model()}</span>
            )}
          </Show>
          {/* Notification bell */}
          <button
            type="button"
            class="relative p-0.5 text-muted-dark hover:text-shell-body transition-colors"
            onClick={() => setDrawerOpen(!drawerOpen())}
            aria-label="Toggle notifications"
          >
            <IconBell class="w-3.5 h-3.5" />
            <Show when={unreadCount() > 0}>
              <span class="absolute -top-1 -right-1 px-0.5 min-w-[12px] text-center rounded-full bg-error text-white text-[8px] font-bold leading-[12px]">
                {unreadCount() > 99 ? '99+' : unreadCount()}
              </span>
            </Show>
          </button>
        </div>
      </div>
      <NotificationCenter open={drawerOpen()} onClose={() => setDrawerOpen(false)} />
    </>
  );
};
