/** The toolbox rows for the cards: gap, radius, edge, lines, shadow, and the grain of glass. */
import { Show, type Component } from 'solid-js';
import { FieldRow } from '../primitives/FieldRow';
import { Segmented } from '../primitives/Segmented';
import { Slider } from '../primitives/Slider';
import type { TweakRowsProps } from './types';

export const CardRows: Component<TweakRowsProps> = (props) => (
  <>
    <FieldRow label="Gap">
      <Slider value={props.tweaks.gap} min={0} max={16} unit="px" onInput={(v) => props.onSet('gap', v)} />
    </FieldRow>
    <FieldRow label="Radius">
      <Slider value={props.tweaks.radius} min={0} max={20} unit="px" onInput={(v) => props.onSet('radius', v)} />
    </FieldRow>
    <FieldRow label="Card edge">
      <Segmented
        value={props.tweaks.edges}
        options={[['none', 'None'], ['hairline', 'Hairline'], ['strong', 'Strong']]}
        onChange={(v) => props.onSet('edges', v)}
      />
    </FieldRow>
    <FieldRow label="Inner lines" hint="Tab bars, headers, rail edges">
      <Segmented
        value={props.tweaks.lines ? 'on' : 'off'}
        options={[['off', 'Off'], ['on', 'On']]}
        onChange={(v) => props.onSet('lines', v === 'on')}
      />
    </FieldRow>
    <FieldRow label="Shadow">
      <Segmented
        value={props.tweaks.shadow}
        options={[['none', 'None'], ['soft', 'Soft'], ['deep', 'Deep']]}
        onChange={(v) => props.onSet('shadow', v)}
      />
    </FieldRow>
    <Show when={props.tweaks.material === 'glass'}>
      <FieldRow label="Grain">
        <Slider value={props.tweaks.grain} min={0} max={10} onInput={(v) => props.onSet('grain', v)} />
      </FieldRow>
    </Show>
  </>
);
