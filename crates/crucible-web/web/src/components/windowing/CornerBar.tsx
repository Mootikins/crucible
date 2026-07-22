import { Component, Show, createSignal } from 'solid-js';
import { useEditorSafe } from '@/contexts/EditorContext';
import { useSettingsSafe } from '@/contexts/SettingsContext';
import { shellActions } from '@/stores/shellStore';
import { attentionStore } from '@/stores/attentionStore';
import { notificationStore } from '@/stores/notificationStore';
import { IconBell } from './icons';
import { NotificationCenter } from '@/components/NotificationCenter';

/**
 * Floating chip cluster at the bottom-right of the center workspace — what
 * replaced the status bar. Everything here is stateful and transient: the
 * save affordance (dirty buffer), the attention chip (things waiting on
 * you), and the notification bell whose panel pops out Adobe-style above
 * the button. Nothing renders when there is nothing to say except the
 * bell.
 */
export const CornerBar: Component = () => {
  const [drawerOpen, setDrawerOpen] = createSignal(false);
  const unreadCount = () => notificationStore.notificationCount();

  // Configurable save affordance (Settings → Editor): the active buffer's
  // dirty state + one-click save.
  const editor = useEditorSafe();
  const { settings } = useSettingsSafe();
  const activeDirtyFile = () => {
    const path = editor.activeFile();
    if (!path) return null;
    const file = editor.openFiles().find((f) => f.path === path);
    return file?.dirty ? file : null;
  };

  return (
    <div class="absolute bottom-2 right-2 z-40 flex items-end gap-1.5 select-none">
      <Show when={settings.editor.showSaveButton && activeDirtyFile()}>
        {(file) => (
          <button
            type="button"
            data-testid="status-save"
            class="flex items-center gap-1.5 h-6 px-2 rounded-md border border-hairline bg-surface-elevated/90 backdrop-blur text-[11px] text-attention hover:bg-hover-wash transition-colors"
            title={`Save ${file().path.split('/').pop()} (Ctrl+S / Alt+S / :w)`}
            onClick={() => void editor.saveFile(file().path)}
          >
            <span>●</span>
            <span>Save</span>
          </button>
        )}
      </Show>
      <Show when={attentionStore.attentionCount() > 0}>
        <button
          type="button"
          data-testid="status-inbox"
          title="Open Inbox"
          class="flex items-center gap-1 h-6 px-2 rounded-md border border-attention/40 bg-surface-elevated/90 backdrop-blur font-mono text-[11px] text-attention hover:bg-attention/15 transition-colors"
          onClick={() => shellActions.goInbox()}
        >
          ▤ {attentionStore.attentionCount()}
        </button>
      </Show>
      {/* Bell + its popout share this relative anchor. */}
      <div class="relative">
        <button
          type="button"
          data-testid="corner-bell"
          class="relative flex items-center justify-center w-6 h-6 rounded-md border border-hairline bg-surface-elevated/90 backdrop-blur text-muted-dark hover:text-shell-body transition-colors"
          classList={{ 'text-shell-body border-hairline-strong': drawerOpen() }}
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
        <NotificationCenter open={drawerOpen()} onClose={() => setDrawerOpen(false)} />
      </div>
    </div>
  );
};
