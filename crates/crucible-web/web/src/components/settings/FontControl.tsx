// src/components/settings/FontControl.tsx
//
// The font picker the Appearance and Terminal sections share. Each section
// supplies its own presets; the control writes one settings field.
import { Component, Show, For, createSignal } from 'solid-js';
import { useSettings } from '@/contexts/SettingsContext';

/** One entry of a font dropdown. An empty value means "keep the default". */
export interface FontPreset {
  label: string;
  value: string;
}

const CUSTOM_FONT = '__custom__';

/** Preset dropdown + a "Custom…" free-text CSS font-family for one font
 * setting (Appearance vars, or the terminal's xterm option). */
export const FontControl: Component<{
  section?: 'appearance' | 'terminal';
  field: 'fontSans' | 'fontMono' | 'fontFamily';
  presets: FontPreset[];
  testid: string;
}> = (props) => {
  const { settings, updateSetting } = useSettings();
  const section = () => props.section ?? 'appearance';
  const val = () =>
    (settings[section()] as unknown as Record<string, string>)[props.field] ?? '';
  const isPreset = () => props.presets.some((p) => p.value === val());
  const [custom, setCustom] = createSignal(val() !== '' && !isPreset());
  return (
    <div class="flex flex-col items-end gap-2">
      <select
        value={custom() && !isPreset() ? CUSTOM_FONT : val()}
        onChange={(e) => {
          const v = e.currentTarget.value;
          if (v === CUSTOM_FONT) {
            setCustom(true);
          } else {
            setCustom(false);
            updateSetting(section() as 'appearance', props.field as 'fontSans', v);
          }
        }}
        class="cru-select rounded border border-hairline bg-surface-base px-2 py-1 text-sm"
        data-testid={props.testid}
      >
        <For each={props.presets}>{(p) => <option value={p.value}>{p.label}</option>}</For>
        <option value={CUSTOM_FONT}>Custom…</option>
      </select>
      <Show when={custom()}>
        <input
          type="text"
          value={isPreset() ? '' : val()}
          onInput={(e) =>
            updateSetting(section() as 'appearance', props.field as 'fontSans', e.currentTarget.value)
          }
          placeholder='e.g. "Inter", sans-serif'
          class="w-56 rounded border border-hairline bg-surface-base px-2 py-1 text-sm"
          data-testid={`${props.testid}-custom`}
        />
      </Show>
    </div>
  );
};
