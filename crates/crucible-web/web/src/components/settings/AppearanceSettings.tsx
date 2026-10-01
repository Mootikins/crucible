import { Component, For } from 'solid-js';
import { useSettings } from '@/contexts/SettingsContext';
import { theme, applyTheme, currentAccent } from '@/lib/theme';
import { Palette } from '@/lib/icons';

import { SectionHeader, SettingRow, BooleanSetting, RangeSetting } from './primitives';
import { FontControl, type FontPreset } from './FontControl';

const SANS_PRESETS: FontPreset[] = [
  { label: 'Geist (default)', value: '' },
  { label: 'System UI', value: 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif' },
  { label: 'Serif', value: 'Georgia, Cambria, "Times New Roman", serif' },
];
const MONO_PRESETS: FontPreset[] = [
  { label: 'Geist Mono (default)', value: '' },
  { label: 'System Mono', value: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace' },
];

/** Shared shell preferences apply live through the settings and theme owners. */
export const AppearanceSettingsSection: Component = () => {
  const { settings, updateSetting } = useSettings();
  return (
  <>
    <SectionHeader title="Appearance" icon={Palette} />
    <SettingRow label="Theme" description="Light or dark surfaces.">
      <select aria-label="Theme" value={theme()} onChange={(e) => applyTheme(e.currentTarget.value as 'dark' | 'light')}>
        <option value="dark">Dark</option><option value="light">Light</option>
      </select>
    </SettingRow>
    <BooleanSetting label="True black" description="Black working surfaces in dark mode."
      checked={settings.appearance.trueBlack}
      onChange={(checked) => updateSetting('appearance', 'trueBlack', checked)} />
    <For each={[
      { key: 'contrast', label: 'Contrast', min: 0, max: 16 },
      { key: 'navTint', label: 'Navigation tint', min: 0, max: 25 },
      { key: 'gap', label: 'Gap', min: 0, max: 20 },
      { key: 'radius', label: 'Corner radius', min: 0, max: 24 },
      { key: 'noteTextSize', label: 'Note text size', min: 12, max: 24 },
    ] as const}>
      {(control) => <RangeSetting
        label={control.label} description="Applies instantly."
        min={control.min} max={control.max} value={settings.appearance[control.key]}
        onInput={(value) => updateSetting('appearance', control.key, value)}
      />}
    </For>
    <SettingRow label="Accent" description="Choose an RGB color, or use the theme default.">
      <input aria-label="Accent" type="color" value={settings.appearance.accent || currentAccent()} onInput={(e) => updateSetting('appearance', 'accent', e.currentTarget.value)} />
      <button type="button" class="ml-2 text-primary" onClick={() => updateSetting('appearance', 'accent', '')}>Default</button>
    </SettingRow>
    <BooleanSetting label="File labels" description="Show file type badges beside non-note files."
      checked={settings.appearance.fileLabels}
      onChange={(checked) => updateSetting('appearance', 'fileLabels', checked)} />
    <SettingRow label="UI font" description="Font for the interface and prose. Applies instantly.">
      <FontControl field="fontSans" presets={SANS_PRESETS} testid="settings-font-sans" />
    </SettingRow>
    <SettingRow label="Code font" description="Monospace font for code and the editor.">
      <FontControl field="fontMono" presets={MONO_PRESETS} testid="settings-font-mono" />
    </SettingRow>
  </>
  );
};
