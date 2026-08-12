import { Component, Show } from 'solid-js';
import { useEditorSafe } from '@/contexts/EditorContext';
import { useSettingsSafe } from '@/contexts/SettingsContext';
import { shellActions } from '@/stores/shellStore';
import { attentionStore } from '@/stores/attentionStore';

/**
 * Floating chip cluster at the bottom-right of the center workspace — what
 * replaced the status bar. Everything here is about the ACTIVE BUFFER and is
 * transient: the save affordance (dirty buffer) and the attention chip (things
 * waiting on you). With nothing to say the cluster renders nothing at all,
 * which the notification bell used to prevent — it was global state parked in a
 * per-document corner, so it kept an empty chip row over every clean file. The
 * bell now sits at the bottom of the right ribbon, beside the panels it belongs
 * with, where collapsing the panel cannot take it away.
 */
export const CornerBar: Component = () => {
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
    </div>
  );
};
