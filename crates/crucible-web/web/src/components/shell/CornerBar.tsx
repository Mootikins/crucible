import { Component, Show } from 'solid-js';
import { useEditorSafe } from '@/contexts/EditorContext';
import { useSettingsSafe } from '@/contexts/SettingsContext';

/**
 * Floating chip cluster at the bottom-right of the center workspace — what
 * replaced the status bar. Everything here is about the ACTIVE BUFFER and is
 * transient: currently the save affordance for a dirty buffer. With nothing to
 * say the cluster renders nothing at all, which the notification bell used to
 * prevent — it was global state parked in a per-document corner, so it kept an
 * empty chip row over every clean file. The bell now sits at the bottom of the
 * right ribbon, beside the panels it belongs with, where collapsing the panel
 * cannot take it away.
 *
 * The attention count left for the same reason, one step later. It read the
 * same `attentionStore.attentionCount()` the titlebar badge reads, so two
 * corners showed one number in two colours with two destinations — and the
 * design has one rule for that colour: brass means bound to the active
 * session. One count, one control. The Inbox is reachable from the switcher
 * flyout and from the right ribbon.
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
            class="flex items-center gap-1.5 h-6 px-2 rounded-md border border-hairline bg-surface-elevated/90 backdrop-blur text-floor text-attention hover:bg-hover-wash transition-colors"
            title={`Save ${file().path.split('/').pop()} (Ctrl+S / Alt+S / :w)`}
            onClick={() => void editor.saveFile(file().path)}
          >
            <span>●</span>
            <span>Save</span>
          </button>
        )}
      </Show>
    </div>
  );
};
