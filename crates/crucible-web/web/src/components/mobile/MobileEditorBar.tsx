import { Component, Show } from 'solid-js';
import { useEditorSafe } from '@/contexts/EditorContext';
import { compactEditorMode, setCompactEditorMode } from '@/stores/editorModeStore';
import type { CompactEditorMode } from '@/stores/editorModeStore';

/**
 * The compact shell's editor controls: Read or Write, and a save affordance.
 *
 * Both belong here rather than in the editor. The editor's own mode buttons
 * float over the text at about 26 px, well under a thumb's target, and the
 * dirty dot the desktop draws lives in a corner bar the phone does not have.
 */
export const MobileEditorBar: Component<{ filePath: string }> = (props) => {
  const editor = useEditorSafe();
  const dirty = () => editor.openFiles().some((f) => f.path === props.filePath && f.dirty);

  const segment = (mode: CompactEditorMode, label: string) => (
    <button
      type="button"
      aria-label={label}
      aria-pressed={compactEditorMode() === mode}
      class={`h-11 px-3 text-xs rounded transition-colors focus-ring ${
        compactEditorMode() === mode
          ? 'bg-control text-shell-ink font-medium'
          : 'text-muted-dark hover:text-shell-ink hover:bg-hover-wash'
      }`}
      onClick={() => setCompactEditorMode(mode)}
    >
      {label}
    </button>
  );

  return (
    <div class="flex items-center gap-1 shrink-0">
      {segment('reading', 'Read')}
      {segment('live', 'Write')}
      <Show when={dirty()}>
        <button
          type="button"
          aria-label="Save"
          class="h-11 px-3 flex items-center gap-1.5 text-xs rounded text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
          onClick={() => void editor.saveFile(props.filePath)}
        >
          <span aria-hidden="true" class="h-1.5 w-1.5 rounded-full bg-attention" />
          Save
        </button>
      </Show>
    </div>
  );
};
