/** The toolbox rows for the theme, the material and the tab forms. */
import type { Component } from 'solid-js';
import { FieldRow } from '../primitives/FieldRow';
import { Segmented } from '../primitives/Segmented';
import type { TweakRowsProps } from './types';

export const SurfaceRows: Component<TweakRowsProps> = (props) => (
  <>
    <FieldRow label="Theme">
      <Segmented value={props.tweaks.theme} options={[['dark', 'Dark'], ['light', 'Light']]} onChange={(v) => props.onSet('theme', v)} />
    </FieldRow>
    <FieldRow label="Surface">
      <Segmented value={props.tweaks.material} options={[['flat', 'Flat'], ['glass', 'Glass']]} onChange={(v) => props.onSet('material', v)} />
    </FieldRow>
    <FieldRow label="Centre tabs" hint="Leaf: the same cutout as the rail tabs">
      <Segmented
        value={props.tweaks.tabs}
        options={[['leaf', 'Leaf'], ['flat', 'Flat'], ['pill', 'Pill']]}
        onChange={(v) => props.onSet('tabs', v)}
      />
    </FieldRow>
    <FieldRow label="Right rail" hint="Card: the focus tone, padded">
      <Segmented
        value={props.tweaks.darkPanes ? 'dark' : 'ground'}
        options={[['dark', 'Card'], ['ground', 'Ground']]}
        onChange={(v) => props.onSet('darkPanes', v === 'dark')}
      />
    </FieldRow>
  </>
);
