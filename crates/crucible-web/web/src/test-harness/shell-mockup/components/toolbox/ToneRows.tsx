/** The toolbox rows for the two tones and the accent. True black applies to the dark theme only. */
import { Show, type Component } from 'solid-js';
import { FieldRow } from '../primitives/FieldRow';
import { Segmented } from '../primitives/Segmented';
import { Slider } from '../primitives/Slider';
import { AccentSwatches } from './AccentSwatches';
import type { AccentOption, TweakRowsProps } from './types';

export const ToneRows: Component<TweakRowsProps & { accents: AccentOption[] }> = (props) => (
  <>
    <FieldRow label="Contrast" hint="Nav against main">
      <Slider value={props.tweaks.contrast} min={0} max={14} onInput={(v) => props.onSet('contrast', v)} />
    </FieldRow>
    <Show when={props.tweaks.theme === 'dark'}>
      <FieldRow label="True black" hint="The focus cards are #000 (OLED)">
        <Segmented
          value={props.tweaks.black ? 'on' : 'off'}
          options={[['off', 'Off'], ['on', 'On']]}
          onChange={(v) => props.onSet('black', v === 'on')}
        />
      </FieldRow>
    </Show>
    <FieldRow label="Nav tint" hint="Accent in the nav tone">
      <Slider value={props.tweaks.tint} min={0} max={20} unit="%" onInput={(v) => props.onSet('tint', v)} />
    </FieldRow>
    <FieldRow label="Accent">
      <AccentSwatches options={props.accents} value={props.tweaks.accent} onPick={(id) => props.onSet('accent', id)} />
    </FieldRow>
  </>
);
