// src/components/settings/AppearanceSettings.tsx
//
// The interface fonts. Both apply live through CSS variables.
import { Component } from 'solid-js';
import { Palette } from '@/lib/icons';

import { SectionHeader, SettingRow } from './primitives';
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

/** Typography: choose the UI + code fonts (applied live via CSS vars). */
export const AppearanceSettingsSection: Component = () => (
  <>
    <SectionHeader title="Appearance" icon={Palette} />
    <SettingRow label="UI font" description="Font for the interface and prose. Applies instantly.">
      <FontControl field="fontSans" presets={SANS_PRESETS} testid="settings-font-sans" />
    </SettingRow>
    <SettingRow label="Code font" description="Monospace font for code and the editor.">
      <FontControl field="fontMono" presets={MONO_PRESETS} testid="settings-font-mono" />
    </SettingRow>
  </>
);
