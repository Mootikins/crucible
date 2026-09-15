// src/components/settings/TerminalSettings.tsx
//
// The terminal panel's typography. Both settings apply to a running terminal.
import { Component } from 'solid-js';
import { Terminal } from '@/lib/icons';

import { SectionHeader, SettingRow } from './primitives';
import { FontControl, type FontPreset } from './FontControl';
import { useSettings } from '@/contexts/SettingsContext';

const TERMINAL_FONT_PRESETS: FontPreset[] = [
  { label: 'Code font (default)', value: '' },
  { label: 'System Mono', value: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace' },
];

/** Terminal panel typography (applies live to a running terminal). */
export const TerminalSettingsSection: Component = () => {
  const { settings, updateSetting } = useSettings();
  return (
    <>
      <SectionHeader title="Terminal" icon={Terminal} />
      <SettingRow
        label="Terminal font"
        description="Font family for the terminal. Default follows the Appearance code font."
      >
        <FontControl
          section="terminal"
          field="fontFamily"
          presets={TERMINAL_FONT_PRESETS}
          testid="settings-terminal-font"
        />
      </SettingRow>
      <SettingRow label="Terminal font size" description="Size in px. Applies instantly.">
        <input
          type="number"
          min="8"
          max="32"
          value={settings.terminal.fontSize}
          onChange={(e) =>
            updateSetting(
              'terminal',
              'fontSize',
              Math.min(32, Math.max(8, Number(e.currentTarget.value) || 13)),
            )
          }
          class="w-20 rounded border border-hairline bg-surface-base px-2 py-1 text-sm"
          data-testid="settings-terminal-font-size"
        />
      </SettingRow>
    </>
  );
};
