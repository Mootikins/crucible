// src/components/settings/EditorSettings.tsx
//
// The note/file editor's own preferences. Each one is per device: it lives in
// localStorage, and it applies to the open editors immediately.
import { Component } from 'solid-js';
import { Pencil } from '@/lib/icons';

import { SectionHeader, SettingRow } from './primitives';
import { useSettings } from '@/contexts/SettingsContext';

/** Editor preferences (persisted locally, applied to open editors live). */
export const EditorSettingsSection: Component = () => {
  const { settings, updateSetting } = useSettings();
  return (
    <>
      <SectionHeader title="Editor" icon={Pencil} />
      {/* One toggle per shell, each labelled with the shell it governs. A
          single toggle would change whichever key the CURRENT shell reads, and
          a phone user would see a desktop switch that seems to do nothing. */}
      <SettingRow
        label="Vim keybindings (desktop)"
        description="Modal editing in the note/file editor, on a desktop-width window."
      >
        <input
          type="checkbox"
          checked={settings.editor.vimMode}
          onChange={(e) => updateSetting('editor', 'vimMode', e.currentTarget.checked)}
          class="h-4 w-4 cursor-pointer"
          data-testid="settings-editor-vim"
        />
      </SettingRow>
      <SettingRow
        label="Vim keybindings (phone)"
        description="Off by default: a phone has no Escape key and no modifier row."
      >
        <input
          type="checkbox"
          checked={settings.editor.vimModeCompact}
          onChange={(e) => updateSetting('editor', 'vimModeCompact', e.currentTarget.checked)}
          class="h-4 w-4 cursor-pointer"
          data-testid="settings-editor-vim-compact"
        />
      </SettingRow>
      <SettingRow
        label="Autosave notes"
        description="Save a note this many seconds after the last edit. Files outside a kiln save only when you ask. 0 turns autosave off."
      >
        <input
          type="number"
          min="0"
          max="600"
          value={settings.editor.autosaveSeconds}
          onChange={(e) =>
            updateSetting(
              'editor',
              'autosaveSeconds',
              Math.max(0, Number(e.currentTarget.value) || 0),
            )
          }
          class="w-20 rounded border border-hairline bg-surface-base px-2 py-1 text-sm"
          data-testid="settings-editor-autosave"
        />
      </SettingRow>
      <SettingRow
        label="Readable line width"
        description="Max prose column width in px for editing and reading views. 0 = full width."
      >
        <input
          type="number"
          min="0"
          max="3000"
          step="10"
          value={settings.editor.maxLineWidth}
          onChange={(e) =>
            updateSetting(
              'editor',
              'maxLineWidth',
              Math.max(0, Number(e.currentTarget.value) || 0),
            )
          }
          class="w-24 rounded border border-hairline bg-surface-base px-2 py-1 text-sm"
          data-testid="settings-editor-line-width"
        />
      </SettingRow>
      <SettingRow
        label="Hover window mode"
        description="What wikilink hover windows open as."
      >
        <select
          value={settings.editor.hoverMode}
          onChange={(e) =>
            updateSetting(
              'editor',
              'hoverMode',
              e.currentTarget.value as 'reading' | 'live' | 'source',
            )
          }
          class="cru-select rounded border border-hairline bg-surface-base px-2 py-1 text-sm"
          data-testid="settings-editor-hover-mode"
        >
          <option value="reading">Reading view</option>
          <option value="live">Live preview</option>
          <option value="source">Source</option>
        </select>
      </SettingRow>
      <SettingRow
        label="Floating save button"
        description="Show a dirty indicator + Save action at the bottom-right of the workspace."
      >
        <input
          type="checkbox"
          checked={settings.editor.showSaveButton}
          onChange={(e) => updateSetting('editor', 'showSaveButton', e.currentTarget.checked)}
          class="h-4 w-4 cursor-pointer"
          data-testid="settings-editor-save-button"
        />
      </SettingRow>
      <SettingRow
        label="Render math in editor"
        description="Show $…$ / $$…$$ as KaTeX in live preview. Off keeps the raw source. (Reading view always renders it.)"
      >
        <input
          type="checkbox"
          checked={settings.editor.renderMath}
          onChange={(e) => updateSetting('editor', 'renderMath', e.currentTarget.checked)}
          class="h-4 w-4 cursor-pointer"
          data-testid="settings-editor-render-math"
        />
      </SettingRow>
      <SettingRow
        label="Render diagrams in editor"
        description="Show ```mermaid fences as diagrams in live preview. Off keeps the raw source. (Reading view always renders them.)"
      >
        <input
          type="checkbox"
          checked={settings.editor.renderDiagrams}
          onChange={(e) => updateSetting('editor', 'renderDiagrams', e.currentTarget.checked)}
          class="h-4 w-4 cursor-pointer"
          data-testid="settings-editor-render-diagrams"
        />
      </SettingRow>
      <SettingRow
        label="Hide frontmatter gap"
        description="Hide the blank lines between frontmatter and the first line of content, in live preview. The reading view never shows them."
      >
        <input
          type="checkbox"
          checked={settings.editor.hideFrontmatterGap}
          onChange={(e) =>
            updateSetting('editor', 'hideFrontmatterGap', e.currentTarget.checked)
          }
          class="h-4 w-4 cursor-pointer"
          data-testid="settings-editor-hide-frontmatter-gap"
        />
      </SettingRow>
    </>
  );
};
