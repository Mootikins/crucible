/**
 * The A/B variants: a switch for each region where the review mixes the
 * current app (A) and the mockup (B), so a reader can try the mix.
 */
import type { Component } from 'solid-js';
import { FieldRow } from '../primitives/FieldRow';
import { Segmented } from '../primitives/Segmented';
import type { TweakRowsProps } from './types';

export const VariantRows: Component<TweakRowsProps> = (props) => (
  <>
    <FieldRow label="Changes view" hint="A/B variant">
      <Segmented
        value={props.tweaks.changesControls}
        options={[['b', 'B'], ['ab', "B + A's controls"]]}
        onChange={(v) => props.onSet('changesControls', v)}
      />
    </FieldRow>
  </>
);
