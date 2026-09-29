/** The toolbox rows for the note text size and the example plugin stylesheet. */
import type { Component } from 'solid-js';
import { FieldRow } from '../primitives/FieldRow';
import { Segmented } from '../primitives/Segmented';
import { Slider } from '../primitives/Slider';
import type { TweakRowsProps } from './types';

export const TextRows: Component<TweakRowsProps> = (props) => (
  <>
    <FieldRow label="Note text">
      <Slider value={props.tweaks.reading} min={13} max={19} unit="px" onInput={(v) => props.onSet('reading', v)} />
    </FieldRow>
    <FieldRow label="Plugin CSS">
      <Segmented
        value={props.tweaks.plugin}
        options={[['none', 'None'], ['docs', 'Docs example']]}
        onChange={(v) => props.onSet('plugin', v)}
      />
    </FieldRow>
  </>
);
